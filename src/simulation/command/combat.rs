use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    config::Config,
    domain::{
        BaseId, BlocKey, Combat, CombatEvent, CombatStructureSnapshot, MilitaryBase, MilitaryUnit, Target, Trust,
        TrustId, UnitId, UnitState,
    },
    geometry::{Distance, Point, Positioned, WorldBounds},
    handlers::combats::CombatResponse,
};

pub(crate) async fn get_all(
    resp: Sender<core::result::Result<Vec<CombatResponse>, crate::error::UserError>>,
    combats: &HashMap<Point, Arc<RwLock<Combat>>>,
    config: &Config,
) {
    let combat_responses: Vec<_> = stream::iter(combats.values())
        .then(async |combat| {
            let combat = combat.read().await;
            if combat.is_empty() {
                None
            } else {
                Some(CombatResponse::from_combat(&combat, config.name_mappings().as_ref()).await)
            }
        })
        .collect()
        .await;
    let combat_responses = combat_responses.into_iter().flatten().collect();

    let _ = resp.send(combat_responses);
}

/// Execute a [Combat::tick] on each combat and clear ended combats from the map.
pub(crate) async fn tick(combats: &mut HashMap<Point, Arc<RwLock<Combat>>>) -> Vec<CombatEvent> {
    let mut events = Vec::new();
    let mut ended_positions = Vec::new();
    for (position, combat) in combats.iter() {
        let mut combat_guard = combat.write().await;
        combat_guard.tick().await;
        if combat_guard.state() == crate::domain::CombatState::Ended {
            events.extend_from_slice(combat_guard.events());
            ended_positions.push(*position);
        }
    }

    combats.retain(|position, _| !ended_positions.contains(position));
    events
}

pub(crate) async fn apply_events(
    events: &[CombatEvent],
    bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    trusts: &mut HashMap<TrustId, Arc<RwLock<Trust>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    world_bounds: WorldBounds,
) {
    for event in events {
        match event {
            CombatEvent::None => {}
            CombatEvent::UnitsKilled { units } => {
                for unit in units {
                    transfer_loot(unit.loot(), bases).await;
                }
            }
            CombatEvent::BaseDestroyed { id, loot: transfers } => {
                for transfer in transfers {
                    transfer_loot(transfer, bases).await;
                }
                destroy_base(*id, bases, units, combats, world_bounds).await;
            }
            CombatEvent::TrustDestroyed { id, loot } => {
                for transfer in loot {
                    transfer_loot(transfer, bases).await;
                }
                destroy_trust(*id, bases, trusts).await;
            }
        }
    }
}

async fn transfer_loot(transfer: &crate::domain::LootTransfer, bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>) {
    if let Some(base) = bases.get(&transfer.base_id()) {
        log::debug!("base {:?}, receives loot: {:?}", transfer.base_id(), transfer.loot());
        base.write().await.add_production(transfer.loot());
    } else {
        log::info!(
            "cannot transfer loot to base {:?} because the base no longer exists",
            transfer.base_id()
        );
    }
}

pub(crate) async fn destroy_base(
    id: BaseId,
    bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    world_bounds: WorldBounds,
) {
    let Some(destroyed_base) = bases.remove(&id) else {
        return;
    };
    let (destroyed_base_bloc, destroyed_base_position) = {
        let base = destroyed_base.read().await;
        (base.bloc_key().clone(), base.position())
    };
    log::info!("base {id:?} was destroyed");

    let replacement = closest_base_in_bloc(&destroyed_base_bloc, destroyed_base_position, bases, world_bounds).await;

    let mut removed_unit_ids = HashSet::new();
    for (unit_id, unit) in units.iter() {
        let belongs_to_destroyed_base = unit.read().await.base().await.id() == id;
        if !belongs_to_destroyed_base {
            continue;
        }

        if let Some((replacement_id, replacement)) = &replacement {
            unit.write().await.rebase(replacement.clone());
            log::info!("unit {unit_id} was rebased from base {id:?} to base {replacement_id:?}");
        } else {
            removed_unit_ids.insert(*unit_id);
            log::info!("unit {unit_id} was removed because bloc {destroyed_base_bloc} has no remaining base");
        }
    }
    units.retain(|unit_id, _| !removed_unit_ids.contains(unit_id));

    let mut ended_combat_positions = Vec::new();
    for (position, combat) in combats.iter() {
        let combat = combat.read().await;
        let attacks_destroyed_base = matches!(
            combat.structure_snapshot().await,
            CombatStructureSnapshot::Base { id: target_id, .. } if target_id == id
        );
        let contains_removed_unit = combat
            .unit_ids_by_bloc()
            .await
            .into_iter()
            .flat_map(|(_, ids)| ids)
            .any(|unit_id| removed_unit_ids.contains(&unit_id));
        if attacks_destroyed_base || contains_removed_unit {
            ended_combat_positions.push(*position);
        }
    }
    combats.retain(|position, _| !ended_combat_positions.contains(position));

    for base in bases.values() {
        let mut base = base.write().await;
        if matches!(base.target(), Target::Base { id: target_id, .. } if *target_id == id) {
            base.set_target(Target::None);
        }
    }
}

async fn closest_base_in_bloc(
    bloc: &BlocKey,
    from: Point,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    world_bounds: WorldBounds,
) -> Option<(BaseId, Arc<RwLock<MilitaryBase>>)> {
    let mut closest: Option<(BaseId, Arc<RwLock<MilitaryBase>>, Distance)> = None;
    for (candidate_id, candidate) in bases {
        let candidate_guard = candidate.read().await;
        if candidate_guard.bloc_key() != bloc {
            continue;
        }
        let distance = world_bounds.distance_between(from, candidate_guard.position());
        let is_closer = closest.as_ref().is_none_or(|(closest_id, _, closest_distance)| {
            distance < *closest_distance || distance == *closest_distance && candidate_id < closest_id
        });
        if is_closer {
            closest = Some((*candidate_id, candidate.clone(), distance));
        }
    }
    closest.map(|(id, base, _)| (id, base))
}

pub(crate) async fn destroy_trust(
    id: TrustId,
    bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &mut HashMap<TrustId, Arc<RwLock<Trust>>>,
) {
    if trusts.remove(&id).is_some() {
        log::info!("trust {id:?} was destroyed");
    }

    for base in bases.values() {
        let mut base = base.write().await;
        if matches!(base.target(), Target::Trust { id: target_id, .. } if *target_id == id) {
            base.set_target(Target::None);
        }
    }
}

pub(crate) async fn clear_dead_units(units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>) {
    let dead_ids = stream::iter(units.values())
        .filter_map(|unit| async {
            let unit = unit.read().await;
            let id = unit.id();
            (unit.state() != UnitState::Alive).then_some(id)
        })
        .collect::<HashSet<_>>()
        .await;

    units.retain(|unit_id, _| !dead_ids.contains(unit_id));
}

#[cfg(test)]
mod tests {
    use mongodb::bson::Uuid;
    use ordered_float::NotNan;

    use super::*;
    use crate::{
        domain::{
            Bloc, BlocName, Chance, Loot, LootFactors, Placement, PlacementId, UnitState, Zone, ZoneKey, ZoneName,
        },
        services::credit_exchange_service::{Cost, Share},
    };

    fn point(x: f64, y: f64) -> Point {
        Point::new(NotNan::new(x).unwrap(), NotNan::new(y).unwrap())
    }

    fn world_bounds() -> WorldBounds {
        serde_json::from_value(serde_json::json!({
            "min_x": 0.0,
            "max_x": 30.0,
            "min_y": 0.0,
            "max_y": 30.0
        }))
        .unwrap()
    }

    fn base(bloc: &str, placement_name: &str, position: Point) -> Arc<RwLock<MilitaryBase>> {
        let bloc_key = BlocKey::from(bloc.to_owned());
        let bloc_name = BlocName::from(bloc.to_owned());
        let bloc_state = Arc::new(RwLock::new(Bloc::new(
            bloc_key.clone(),
            bloc_name.clone(),
            Chance::new(1),
            Share::default(),
        )));
        let zone = Arc::new(Zone::new_with_social_rules(
            ZoneKey::from(format!("{placement_name}-zone")),
            ZoneName::from(format!("{placement_name} zone")),
            bloc_key,
            bloc_name,
            bloc_state,
            Vec::new(),
        ));
        let placement = Arc::new(Placement::new(
            serde_json::from_value::<PlacementId>(serde_json::json!(placement_name)).unwrap(),
            zone,
            position,
        ));
        let cost: Cost<MilitaryBase> = serde_json::from_value(serde_json::json!({
            "money": 0.0,
            "resources": {}
        }))
        .unwrap();

        Arc::new(RwLock::new(MilitaryBase::new_prepaid(
            Vec::new(),
            &cost,
            &LootFactors::default(),
            placement,
        )))
    }

    fn unit(base: Arc<RwLock<MilitaryBase>>, position: Point) -> Arc<RwLock<MilitaryUnit>> {
        Arc::new(RwLock::new(MilitaryUnit::from_persisted(
            Uuid::new(),
            base,
            position,
            UnitState::Alive,
            Loot::default(),
        )))
    }

    async fn id(base: &RwLock<MilitaryBase>) -> BaseId {
        base.read().await.id()
    }

    #[tokio::test]
    async fn destruction_rebases_units_to_base_closest_to_destroyed_base() {
        let destroyed = base("same-bloc", "destroyed", point(2.0, 2.0));
        let closest = base("same-bloc", "closest", point(5.0, 2.0));
        let closest_id = id(&closest).await;
        let farther = base("same-bloc", "farther", point(14.0, 2.0));
        let unit = unit(destroyed.clone(), point(14.0, 2.0));
        let unit_id = unit.read().await.id();
        let destroyed_id = id(&destroyed).await;
        let mut bases = HashMap::from([
            (destroyed_id, destroyed),
            (closest_id, closest),
            (id(&farther).await, farther),
        ]);
        let mut units = HashMap::from([(unit_id, unit.clone())]);

        destroy_base(
            destroyed_id,
            &mut bases,
            &mut units,
            &mut HashMap::new(),
            world_bounds(),
        )
        .await;

        assert_eq!(unit.read().await.base().await.id(), closest_id);
    }

    #[tokio::test]
    async fn destruction_breaks_equal_distance_ties_by_lowest_base_id() {
        let destroyed = base("same-bloc", "tie-destroyed", point(10.0, 10.0));
        let lower_id_base = base("same-bloc", "tie-lower", point(8.0, 10.0));
        let lower_id = id(&lower_id_base).await;
        let higher_id_base = base("same-bloc", "tie-higher", point(12.0, 10.0));
        let higher_id = id(&higher_id_base).await;
        let unit = unit(destroyed.clone(), point(10.0, 10.0));
        let unit_id = unit.read().await.id();
        let destroyed_id = id(&destroyed).await;
        let mut bases = HashMap::from([
            (destroyed_id, destroyed),
            (higher_id, higher_id_base),
            (lower_id, lower_id_base),
        ]);
        let mut units = HashMap::from([(unit_id, unit.clone())]);

        destroy_base(
            destroyed_id,
            &mut bases,
            &mut units,
            &mut HashMap::new(),
            world_bounds(),
        )
        .await;

        assert!(lower_id < higher_id);
        assert_eq!(unit.read().await.base().await.id(), lower_id);
    }

    #[tokio::test]
    async fn destruction_removes_units_when_bloc_has_no_remaining_base() {
        let destroyed = base("destroyed-bloc", "last-base", point(10.0, 10.0));
        let enemy_base = base("enemy-bloc", "enemy-base", point(10.0, 11.0));
        let dependent = unit(destroyed.clone(), point(10.0, 10.0));
        let surviving_enemy = unit(enemy_base.clone(), point(10.0, 11.0));
        let dependent_id = dependent.read().await.id();
        let surviving_enemy_id = surviving_enemy.read().await.id();
        let destroyed_id = id(&destroyed).await;
        let mut bases = HashMap::from([(destroyed_id, destroyed), (id(&enemy_base).await, enemy_base)]);
        let mut units = HashMap::from([(dependent_id, dependent), (surviving_enemy_id, surviving_enemy)]);

        destroy_base(
            destroyed_id,
            &mut bases,
            &mut units,
            &mut HashMap::new(),
            world_bounds(),
        )
        .await;

        assert!(!units.contains_key(&dependent_id));
        assert!(units.contains_key(&surviving_enemy_id));
    }
}
