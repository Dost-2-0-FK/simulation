use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    config::Config,
    domain::{BaseId, Combat, CombatEvent, MilitaryBase, MilitaryUnit, Target, Trust, TrustId, UnitId, UnitState},
    geometry::Point,
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
    trusts: &mut HashMap<TrustId, Arc<RwLock<Trust>>>,
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
                destroy_base(*id, bases).await;
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

pub(crate) async fn destroy_base(id: BaseId, bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>) {
    if bases.remove(&id).is_some() {
        log::info!("base {id:?} was destroyed");
    }

    for base in bases.values() {
        let mut base = base.write().await;
        if matches!(base.target(), Target::Base { id: target_id, .. } if *target_id == id) {
            base.set_target(Target::None);
        }
    }
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
