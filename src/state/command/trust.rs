use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use super::CommandError;
use crate::{
    domain::{Placement, PlacementId, Trust, TrustId},
    handlers::bases::Financing,
    services::credit_exchange_service::CreditExchangeService,
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
    credit_exchange_service: &CreditExchangeService,
    mut placements: impl Iterator<Item = Arc<Placement>>,
) -> Result<Trust, CommandError> {
    log::debug!("received command to create trust on placement with id {placement_id:?}");
    let Some(placement) = placements.find(|p| p.id() == &placement_id) else {
        return Err(CommandError::NotFound("Placement"));
    };
    let payment = credit_exchange_service
        .pay_for_trust(placement.zone().name(), financing)
        .await
        .map_err(|e| CommandError::CreditExchange(e.to_string()))?;
    let payment_policy = payment.policy().clone();
    let trust = Trust::new(payment, credit_exchange_service.loot_factors(), placement);
    credit_exchange_service
        .register_trust(&trust, &payment_policy)
        .await
        .map_err(|err| CommandError::CreditExchange(err.to_string()))?;
    Ok(trust)
}
