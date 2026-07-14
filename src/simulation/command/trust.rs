use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use super::CommandError;
use crate::{
    domain::{Loot, Placement, PlacementId, Trust, TrustId},
    handlers::bases::Financing,
    services::credit_exchange_service::{CreditExchangeService, ResourceName},
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
    resource: ResourceName,
    trust_production_income: &Loot,
    credit_exchange_service: &CreditExchangeService,
    mut placements: impl Iterator<Item = Arc<Placement>>,
) -> Result<Trust, CommandError> {
    log::debug!("received command to create trust on placement with id {placement_id:?}");
    let Some(placement) = placements.find(|p| p.id() == &placement_id) else {
        return Err(CommandError::NotFound("Placement"));
    };
    let Some(resource_amount) = trust_production_income.resource_amount(&resource) else {
        return Err(CommandError::NotFound("Resource"));
    };
    let payment = credit_exchange_service
        .pay_for_trust(placement.zone().name(), financing)
        .await
        .map_err(|e| CommandError::CreditExchange(e.to_string()))?;
    let payment_policy = payment.policy().clone();
    let trust = Trust::new(
        payment,
        credit_exchange_service.loot_factors(),
        placement,
        resource,
        resource_amount,
        trust_production_income.money(),
    );
    credit_exchange_service
        .register_trust(&trust, &payment_policy)
        .await
        .map_err(|err| CommandError::CreditExchange(err.to_string()))?;
    Ok(trust)
}

pub(crate) async fn publish_production(
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    credit_exchange_service: &CreditExchangeService,
) -> anyhow::Result<()> {
    for trust_arc in trusts.values() {
        let trust = trust_arc.read().await;

        if let Err(err) = credit_exchange_service.set_trust_production(&trust).await {
            log::error!(
                "failed to publish credit production for trust {trust_id:?}: {err}",
                trust_id = trust.id()
            );
        }
    }
    Ok(())
}
