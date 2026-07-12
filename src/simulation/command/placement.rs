use std::sync::Arc;

use tokio::sync::oneshot::Sender;

use crate::domain::Placement;

pub(crate) fn get(resp: Sender<Vec<Arc<Placement>>>, placements: impl Iterator<Item = Arc<Placement>>) {
    let _ = resp.send(placements.collect());
}
