use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use super::CommandError;
use crate::{
    domain::{BaseId, MilitaryBase, Placement, PlacementId, Target},
    handlers::bases::Financing,
    services::credit_exchange_service::CreditExchangeService,
};

pub(crate) async fn get_all(resp: Sender<Vec<MilitaryBase>>, bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>) {
    let mut out = Vec::with_capacity(bases.len());
    for base in bases.values() {
        out.push(base.read().await.clone());
    }
    let _ = resp.send(out);
}

pub(crate) async fn get(
    id: BaseId,
    resp: Sender<Option<MilitaryBase>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
) {
    let base = match bases.get(&id) {
        Some(base) => Some(base.read().await.clone()),
        None => None,
    };
    let _ = resp.send(base);
}

pub(crate) async fn create(
    placement_id: PlacementId,
    financing: Vec<Financing>,
    credit_exchange_service: &CreditExchangeService,
    mut placements: impl Iterator<Item = Arc<Placement>>,
) -> Result<MilitaryBase, CommandError> {
    log::debug!("received command to create base on placement with id {placement_id:?}");
    let payment = credit_exchange_service.pay_for_military_base(financing).await;
    let Some(placement) = placements.find(|p| p.id() == &placement_id) else {
        return Err(CommandError::NotFound("Placement"));
    };
    Ok(MilitaryBase::new(payment, placement))
}

pub(crate) fn patch(
    mut base: MilitaryBase,
    enabled: Option<bool>,
    prioritized: Option<bool>,
    target: Option<Target>,
) -> MilitaryBase {
    if let Some(enabled) = enabled {
        base.set_enabled(enabled);
    }
    if let Some(prioritized) = prioritized {
        base.set_prioritized(prioritized);
    }
    if let Some(target) = target {
        base.set_target(target);
    }
    base
}
