use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{Combat, CombatState, MilitaryUnit, UnitId, UnitState},
    geometry::Point,
    handlers::combats::CombatResponse,
};

pub(crate) async fn get_all(resp: Sender<Vec<CombatResponse>>, combats: &HashMap<Point, Arc<RwLock<Combat>>>) {
    let combat_responses: Vec<_> = stream::iter(combats.values())
        .then(async |combat| {
            let combat = combat.read().await;
            if combat.state() == CombatState::Ended || combat.is_empty() {
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
pub(crate) async fn tick(combats: &mut HashMap<Point, Arc<RwLock<Combat>>>) {
    let mut positions_to_clear = HashSet::new();
    for (position, combat) in combats.iter() {
        let mut combat_guard = combat.write().await;
        // TODO? Persist events on the combat struct?
        let _event = combat_guard.tick().await;
        if combat_guard.state() == CombatState::Ended {
            positions_to_clear.insert(*position);
        }
    }

    combats.retain(|position, _| !positions_to_clear.contains(position));
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
