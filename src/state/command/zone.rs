use std::sync::Arc;

use tokio::sync::oneshot::Sender;

use crate::domain::Zone;

pub(crate) fn get(resp: Sender<Vec<Arc<Zone>>>, zones: impl Iterator<Item = Arc<Zone>>) {
    let _ = resp.send(zones.collect());
}
