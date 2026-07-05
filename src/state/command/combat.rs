use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{domain::Combat, geometry::Point, handlers::combats::CombatResponse};

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
