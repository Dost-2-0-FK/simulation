use std::{collections::HashMap, sync::Arc};

use rstar::{AABB, PointDistance, RTree, RTreeObject};
use tokio::sync::RwLock;

use crate::{
    domain::{BlocKey, Combat, CombatParameters, CombatState, MilitaryUnit, Target, UnitId},
    geometry::{Distance, Point, Positioned, WorldBounds},
};

/// The resolved destination a unit will move toward this tick.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MoveTo {
    /// An enemy unit is the closest target.
    EnemyUnit(UnitId, Point),
    /// The designated secondary target (base or trust) is the closest.
    Designated(Point),
    /// No appropriate target exists (e.g., when there are no enemy units)
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct IndexedUnit {
    id: UnitId,
    position: Point,
}

impl RTreeObject for IndexedUnit {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.position.coordinates())
    }
}

impl PointDistance for IndexedUnit {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let position = self.position.coordinates();
        let dx = position[0] - point[0];
        let dy = position[1] - point[1];
        dx * dx + dy * dy
    }
}

/// Immutable positions and bloc membership used for every target lookup in one operation.
pub(super) struct UnitSpatialIndex {
    by_bloc: HashMap<BlocKey, RTree<IndexedUnit>>,
}

impl UnitSpatialIndex {
    pub(super) async fn snapshot(units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>) -> Self {
        let mut positions_by_bloc = HashMap::<BlocKey, HashMap<Point, UnitId>>::new();
        for unit in units.values() {
            let unit = unit.read().await;
            let bloc = unit.base().await.bloc_key().clone();
            positions_by_bloc
                .entry(bloc)
                .or_default()
                .entry(unit.position())
                .and_modify(|id| *id = (*id).min(unit.id()))
                .or_insert_with(|| unit.id());
        }

        let by_bloc = positions_by_bloc
            .into_iter()
            .map(|(bloc, positions)| {
                let units = positions
                    .into_iter()
                    .map(|(position, id)| IndexedUnit { id, position })
                    .collect();
                (bloc, RTree::bulk_load(units))
            })
            .collect();
        Self { by_bloc }
    }

    fn closest_enemy(&self, from: Point, unit_bloc: &BlocKey, world_bounds: WorldBounds) -> Option<IndexedUnit> {
        let mut closest: Option<(IndexedUnit, Distance)> = None;
        for (candidate_bloc, index) in &self.by_bloc {
            if candidate_bloc == unit_bloc {
                continue;
            }

            for image in world_bounds.periodic_images(from) {
                for candidate in index.nearest_neighbors(&image.coordinates()) {
                    let distance = world_bounds.distance_between(from, candidate.position);
                    let is_closer = closest
                        .as_ref()
                        .map(|(closest, closest_distance)| {
                            distance < *closest_distance || distance == *closest_distance && candidate.id < closest.id
                        })
                        .unwrap_or(true);
                    if is_closer {
                        closest = Some((*candidate, distance));
                    }
                }
            }
        }
        closest.map(|(unit, _)| unit)
    }
}

struct UnitMovement {
    unit: Arc<RwLock<MilitaryUnit>>,
    id: UnitId,
    bloc: BlocKey,
    target: Target,
    position: Point,
    reached_designated_target: bool,
}

type UnitsByBloc = HashMap<BlocKey, HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>>;

/// Moves `unit` one `step` toward its effective target (see [`select_move_target`]).
/// Returns whether the unit reached its designated base or trust target.
async fn move_toward_target(
    unit: &mut MilitaryUnit,
    unit_bloc: &BlocKey,
    target: &Target,
    spatial_index: &UnitSpatialIndex,
    step: Distance,
    world_bounds: WorldBounds,
) -> bool {
    let target_position = match target {
        Target::None => None,
        Target::Base { base, .. } => Some(base.read().await.position()),
        Target::Trust { trust, .. } => Some(trust.read().await.position()),
    };

    let from = unit.position();
    let (to, is_designated) = match select_move_target(from, unit_bloc, target_position, spatial_index, world_bounds) {
        MoveTo::None => return false,
        MoveTo::EnemyUnit(_, position) => (position, false),
        MoveTo::Designated(position) => (position, true),
    };
    let to = world_bounds.wrap(to);
    unit.move_toward(to, step, world_bounds);
    is_designated && unit.position() == to
}

/// Runs one movement tick: each unit moves one `step` toward its designated target (or the
/// closest enemy unit if one is nearer than the target).
///
/// Moved units may cause units to join an existing combat or create a new one in their position.
pub(crate) async fn move_units(
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    step: Distance,
    world_bounds: WorldBounds,
    base_destruction_threshold: u32,
    trust_destruction_threshold: u32,
) {
    let mut ended_combats = Vec::new();
    for (position, combat) in combats.iter() {
        if combat.read().await.state() == CombatState::Ended {
            ended_combats.push(*position);
        }
    }
    combats.retain(|position, _| !ended_combats.contains(position));

    let spatial_index = UnitSpatialIndex::snapshot(units).await;
    let mut movements = Vec::with_capacity(units.len());
    for unit in units.values() {
        let mut unit_write_guard = unit.write().await;
        let (unit_bloc, target) = {
            let base = unit_write_guard.base().await;
            (base.bloc_key().clone(), base.target().clone())
        };

        let reached_designated_target = move_toward_target(
            &mut unit_write_guard,
            &unit_bloc,
            &target,
            &spatial_index,
            step,
            world_bounds,
        )
        .await;
        movements.push(UnitMovement {
            unit: unit.clone(),
            id: unit_write_guard.id(),
            bloc: unit_bloc,
            target,
            position: unit_write_guard.position(),
            reached_designated_target,
        });
    }

    movements.sort_unstable_by_key(|movement| movement.id);
    resolve_combats(
        &movements,
        combats,
        base_destruction_threshold,
        trust_destruction_threshold,
    )
    .await;
}

async fn resolve_combats(
    movements: &[UnitMovement],
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    base_destruction_threshold: u32,
    trust_destruction_threshold: u32,
) {
    let mut units_by_position = HashMap::<Point, UnitsByBloc>::new();
    for movement in movements {
        units_by_position
            .entry(movement.position)
            .or_default()
            .entry(movement.bloc.clone())
            .or_default()
            .insert(movement.id, movement.unit.clone());
    }

    for movement in movements {
        if let Some(existing_combat) = combats.get(&movement.position) {
            existing_combat.write().await.include_unit(movement.unit.clone()).await;
        }
    }

    for movement in movements.iter().filter(|movement| movement.reached_designated_target) {
        if combats.contains_key(&movement.position) {
            continue;
        }

        let combat_params = match &movement.target {
            Target::Base { base, .. } => {
                CombatParameters::Base(movement.unit.clone(), base.clone(), base_destruction_threshold)
            }
            Target::Trust { trust, .. } => {
                CombatParameters::Trust(movement.unit.clone(), trust.clone(), trust_destruction_threshold)
            }
            Target::None => unreachable!("only designated targets can be reached here"),
        };

        let mut new_combat = Combat::new(combat_params).await;
        for units in units_by_position
            .get(&movement.position)
            .expect("every moved unit was indexed by its final position")
            .values()
        {
            for unit in units.values() {
                new_combat.include_unit(unit.clone()).await;
            }
        }
        combats.insert(movement.position, Arc::new(RwLock::new(new_combat)));
    }

    for (position, units_by_bloc) in units_by_position {
        if combats.contains_key(&position) || units_by_bloc.len() < 2 {
            continue;
        }

        let initiating_unit_id = units_by_bloc
            .values()
            .flat_map(HashMap::keys)
            .min()
            .expect("a position with multiple blocs has at least one unit");
        log::debug!("Unit {} engages in new combat with other units", initiating_unit_id);
        let new_combat = Combat::new(CombatParameters::Units(units_by_bloc)).await;
        combats.insert(position, Arc::new(RwLock::new(new_combat)));
    }
}

/// Core helper: given a unit's current position, its optional designated `target_point`, and
/// all live units, returns where the unit should move — or `None` when there is nothing to
/// move toward (no designated target and no enemies).
///
/// An enemy unit that is strictly closer than `target_point` always wins over the designated
/// target. When `target_point` is `None` the closest enemy unit is returned unconditionally.
pub(super) fn select_move_target(
    from: Point,
    unit_bloc: &BlocKey,
    target_point: Option<Point>,
    spatial_index: &UnitSpatialIndex,
    world_bounds: WorldBounds,
) -> MoveTo {
    let closest_enemy = spatial_index.closest_enemy(from, unit_bloc, world_bounds);

    match target_point {
        None => closest_enemy
            .map(|unit| MoveTo::EnemyUnit(unit.id, unit.position))
            .unwrap_or(MoveTo::None),
        Some(target) => {
            let target_dist = world_bounds.distance_between(from, target);
            match closest_enemy {
                Some(unit) if world_bounds.distance_between(from, unit.position) < target_dist => {
                    MoveTo::EnemyUnit(unit.id, unit.position)
                }
                _ => MoveTo::Designated(target),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::Uuid;
    use ordered_float::NotNan;

    use super::*;
    use crate::{
        domain::{
            Bloc, BlocName, Chance, Loot, MilitaryBase, Placement, PlacementId, SimulationStats, UnitState, Zone,
            ZoneKey, ZoneName,
        },
        services::credit_exchange_service::{Cost, Share},
        simulation::command::combat,
    };

    fn point(x: f64, y: f64) -> Point {
        Point::new(NotNan::new(x).unwrap(), NotNan::new(y).unwrap())
    }

    fn distance(value: f64) -> Distance {
        serde_json::from_value(serde_json::json!(value)).unwrap()
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

    fn base(bloc: &str, position: Point) -> Arc<RwLock<MilitaryBase>> {
        let bloc_key = BlocKey::from(bloc.to_string());
        let bloc_name = BlocName::from(bloc.to_string());
        let bloc_state = Arc::new(RwLock::new(Bloc::new(
            bloc_key.clone(),
            bloc_name.clone(),
            Chance::new(1),
            Share::default(),
        )));
        let zone = Arc::new(Zone::new_with_social_rules(
            ZoneKey::from(format!("{bloc}-zone")),
            ZoneName::from(format!("{bloc} zone")),
            bloc_key,
            bloc_name,
            bloc_state,
            Vec::new(),
        ));
        let placement = Arc::new(Placement::new(
            serde_json::from_value::<PlacementId>(serde_json::json!(format!("{bloc}-placement"))).unwrap(),
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
            &Default::default(),
            placement,
        )))
    }

    fn unit(base: Arc<RwLock<MilitaryBase>>, position: Point) -> Arc<RwLock<MilitaryUnit>> {
        unit_with_uuid(base, position, Uuid::new())
    }

    fn unit_with_id(base: Arc<RwLock<MilitaryBase>>, position: Point, id: u8) -> Arc<RwLock<MilitaryUnit>> {
        unit_with_uuid(base, position, Uuid::from_bytes([id; 16]))
    }

    fn unit_with_uuid(base: Arc<RwLock<MilitaryBase>>, position: Point, id: Uuid) -> Arc<RwLock<MilitaryUnit>> {
        Arc::new(RwLock::new(MilitaryUnit::from_persisted(
            id,
            base,
            position,
            UnitState::Alive,
            Loot::default(),
        )))
    }

    async fn unit_map(
        units: impl IntoIterator<Item = Arc<RwLock<MilitaryUnit>>>,
    ) -> HashMap<UnitId, Arc<RwLock<MilitaryUnit>>> {
        let mut result = HashMap::new();
        for unit in units {
            let id = unit.read().await.id();
            result.insert(id, unit);
        }
        result
    }

    #[tokio::test]
    async fn nearest_enemy_uses_wrapped_distance() {
        let enemy_base = base("enemy", point(0.0, 0.0));
        let wrapped_enemy = unit_with_id(enemy_base.clone(), point(29.0, 10.0), 1);
        let direct_enemy = unit_with_id(enemy_base, point(10.0, 10.0), 2);
        let wrapped_enemy_id = wrapped_enemy.read().await.id();
        let units = unit_map([wrapped_enemy, direct_enemy]).await;
        let index = UnitSpatialIndex::snapshot(&units).await;

        let target = select_move_target(
            point(1.0, 10.0),
            &BlocKey::from("friendly".to_string()),
            None,
            &index,
            world_bounds(),
        );

        assert_eq!(target, MoveTo::EnemyUnit(wrapped_enemy_id, point(29.0, 10.0)));
    }

    #[tokio::test]
    async fn equal_distance_enemy_ties_use_lowest_unit_id() {
        let enemy_base = base("enemy", point(0.0, 0.0));
        let lower_id_enemy = unit_with_id(enemy_base.clone(), point(11.0, 10.0), 1);
        let higher_id_enemy = unit_with_id(enemy_base, point(9.0, 10.0), 2);
        let lower_id = lower_id_enemy.read().await.id();
        let units = unit_map([higher_id_enemy, lower_id_enemy]).await;
        let index = UnitSpatialIndex::snapshot(&units).await;

        let target = select_move_target(
            point(10.0, 10.0),
            &BlocKey::from("friendly".to_string()),
            None,
            &index,
            world_bounds(),
        );

        assert_eq!(target, MoveTo::EnemyUnit(lower_id, point(11.0, 10.0)));
    }

    #[tokio::test]
    async fn designated_target_wins_an_equal_distance_tie() {
        let enemy = unit(base("enemy", point(0.0, 0.0)), point(12.0, 10.0));
        let units = unit_map([enemy]).await;
        let index = UnitSpatialIndex::snapshot(&units).await;

        let target = select_move_target(
            point(10.0, 10.0),
            &BlocKey::from("friendly".to_string()),
            Some(point(8.0, 10.0)),
            &index,
            world_bounds(),
        );

        assert_eq!(target, MoveTo::Designated(point(8.0, 10.0)));
    }

    #[tokio::test]
    async fn simultaneous_movement_creates_combat_at_the_final_shared_position() {
        let unit_a = unit(base("a", point(10.0, 10.0)), point(10.0, 10.0));
        let unit_b = unit(base("b", point(12.0, 10.0)), point(12.0, 10.0));
        let mut units = unit_map([unit_a.clone(), unit_b.clone()]).await;
        let mut combats = HashMap::new();

        move_units(&mut units, &mut combats, distance(1.0), world_bounds(), 1, 1).await;

        assert_eq!(unit_a.read().await.position(), point(11.0, 10.0));
        assert_eq!(unit_b.read().await.position(), point(11.0, 10.0));
        let combat = combats.get(&point(11.0, 10.0)).expect("collocated enemies fight");
        let unit_count = combat
            .read()
            .await
            .unit_ids_by_bloc()
            .await
            .into_iter()
            .map(|(_, ids)| ids.len())
            .sum::<usize>();
        assert_eq!(unit_count, 2);
    }

    #[tokio::test]
    async fn movement_tick_handles_eight_thousand_collocated_units() {
        let base_a = base("a", point(10.0, 10.0));
        let base_b = base("b", point(12.0, 10.0));
        let mut units = Vec::with_capacity(8_000);
        for _ in 0..4_000 {
            units.push(unit(base_a.clone(), point(10.0, 10.0)));
            units.push(unit(base_b.clone(), point(12.0, 10.0)));
        }
        let mut units = unit_map(units).await;
        let mut combats = HashMap::new();

        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            move_units(&mut units, &mut combats, distance(1.0), world_bounds(), 1, 1),
        )
        .await
        .expect("an 8,000-unit movement tick should complete promptly");

        let combat = combats.get(&point(11.0, 10.0)).expect("collocated enemies fight");
        let unit_count = combat
            .read()
            .await
            .unit_ids_by_bloc()
            .await
            .into_iter()
            .map(|(_, ids)| ids.len())
            .sum::<usize>();
        assert_eq!(unit_count, 8_000);
    }

    #[tokio::test]
    async fn movement_after_mutual_kill_does_not_target_dead_units_or_create_combat() {
        let combat_position = point(10.0, 10.0);
        let surviving_position = point(20.0, 20.0);
        let bloc_a = BlocKey::from("a".to_string());
        let bloc_b = BlocKey::from("b".to_string());
        let base_a = base("a", combat_position);
        let unit_a = unit(base_a.clone(), combat_position);
        let unit_b = unit(base("b", combat_position), combat_position);
        let surviving_unit = unit(base_a, surviving_position);
        let unit_a_id = unit_a.read().await.id();
        let unit_b_id = unit_b.read().await.id();
        let surviving_unit_id = surviving_unit.read().await.id();

        let mut units = HashMap::from([
            (unit_a_id, unit_a.clone()),
            (unit_b_id, unit_b.clone()),
            (surviving_unit_id, surviving_unit.clone()),
        ]);
        let combat = Combat::new(CombatParameters::Units(HashMap::from([
            (bloc_a, HashMap::from([(unit_a_id, unit_a)])),
            (bloc_b, HashMap::from([(unit_b_id, unit_b)])),
        ])))
        .await;
        let mut combats = HashMap::from([(combat_position, Arc::new(RwLock::new(combat)))]);

        let events = combat::tick(&mut combats, &mut SimulationStats::default()).await;

        assert!(matches!(
            events.as_slice(),
            [crate::domain::CombatEvent::UnitsKilled { units }] if units.len() == 2
        ));
        combat::clear_dead_units(&mut units).await;
        assert_eq!(units.len(), 1);
        assert!(units.contains_key(&surviving_unit_id));

        move_units(&mut units, &mut combats, distance(1.0), world_bounds(), 1, 1).await;

        assert!(combats.is_empty());
        assert_eq!(surviving_unit.read().await.position(), surviving_position);
    }
}
