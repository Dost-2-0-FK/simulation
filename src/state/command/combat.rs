use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{Combat, CombatState},
    geometry::Point,
    handlers::combats::CombatResponse,
};

pub(crate) async fn get_all(resp: Sender<Vec<CombatResponse>>, combats: &HashMap<Point, Arc<RwLock<Combat>>>) {
    let combat_responses = stream::iter(combats.values())
        .then(async |combat| {
            let combat = combat.read().await;
            CombatResponse::from_combat(&combat).await
        })
        .collect()
        .await;

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
