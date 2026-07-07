use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{
        BaseId, Combat, CombatEvent, CombatState, MilitaryBase, MilitaryUnit, Target, Trust, TrustId, UnitId, UnitState,
    },
    geometry::Point,
    handlers::combats::CombatResponse,
};

pub(crate) async fn get_all(resp: Sender<Vec<CombatResponse>>, combats: &HashMap<Point, Arc<RwLock<Combat>>>) {
    let combat_responses: Vec<_> = stream::iter(combats.values())
        .then(async |combat| {
            let combat = combat.read().await;
            if combat.is_empty() {
                None
            } else {
                Some(CombatResponse::from_combat(&combat).await)
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
    for combat in combats.values() {
        let mut combat_guard = combat.write().await;
        let event = combat_guard.tick().await;
        if event != CombatEvent::None {
            events.push(event);
        }
    }

    events
}

pub(crate) async fn apply_events(
    events: &[CombatEvent],
    bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &mut HashMap<TrustId, Arc<RwLock<Trust>>>,
) {
    for event in events {
        match event {
            CombatEvent::None | CombatEvent::UnitsKilled { .. } => {}
            CombatEvent::BaseDestroyed { id } => destroy_base(*id, bases).await,
            CombatEvent::TrustDestroyed { id } => destroy_trust(*id, bases, trusts).await,
        }
    }
}

async fn destroy_base(id: BaseId, bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>) {
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

async fn destroy_trust(
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
