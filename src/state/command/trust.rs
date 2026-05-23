use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use super::CommandError;
use crate::{
    domain::{Placement, PlacementId, Trust, TrustId},
    handlers::bases::Financing,
    services::payment_service::PaymentService,
};

pub(crate) async fn get_all(resp: Sender<Vec<Trust>>, trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>) {
    let mut result = Vec::with_capacity(trusts.len());
    for trust in trusts.values() {
        result.push(trust.read().await.clone());
    }
    let _ = resp.send(result);
}

pub(crate) async fn get(id: TrustId, resp: Sender<Option<Trust>>, trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>) {
    let trust = match trusts.get(&id) {
        Some(trust) => Some(trust.read().await.clone()),
        None => None,
    };
    let _ = resp.send(trust);
}

pub(crate) async fn create(
    placement_id: PlacementId,
    financing: Vec<Financing>,
    payment_service: &PaymentService,
    mut placements: impl Iterator<Item = Arc<Placement>>,
) -> Result<Trust, CommandError> {
    log::debug!("received command to create trust on placement with id {placement_id:?}");
    let payment = payment_service.pay_for_trust(financing).await;
    let Some(placement) = placements.find(|p| p.id() == &placement_id) else {
        return Err(CommandError::NotFound("Placement"));
    };
    Ok(Trust::new(payment, placement))
}
