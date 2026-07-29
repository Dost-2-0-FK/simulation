use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use super::CommandError;
use crate::{
    domain::{BaseId, BlocKey, MilitaryBase, Placement, PlacementId, Target},
    error::UserError,
    handlers::bases::Financing,
    services::credit_exchange_service::CreditExchangeService,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaseTargetValidationError {
    SameBloc,
}

impl From<BaseTargetValidationError> for UserError {
    fn from(error: BaseTargetValidationError) -> Self {
        match error {
            BaseTargetValidationError::SameBloc => Self::InvalidBaseTarget,
        }
    }
}

pub(crate) fn validate_target_bloc(
    base_bloc: &BlocKey,
    target_bloc: &BlocKey,
) -> Result<(), BaseTargetValidationError> {
    if base_bloc == target_bloc {
        Err(BaseTargetValidationError::SameBloc)
    } else {
        Ok(())
    }
}

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
    let Some(placement) = placements.find(|p| p.id() == &placement_id) else {
        return Err(CommandError::NotFound("Placement"));
    };
    let payment = credit_exchange_service
        .pay_for_military_base(placement.zone().bloc_key(), financing)
        .await
        .map_err(CommandError::CreditExchange)?;
    let payment_policy = payment.policy().clone();
    let base = MilitaryBase::new(payment, credit_exchange_service.loot_factors(), placement);
    credit_exchange_service
        .register_military_base(&base, payment_policy)
        .await
        .map_err(CommandError::CreditExchange)?;
    Ok(base)
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

pub(crate) async fn publish_production(
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    credit_exchange_service: &CreditExchangeService,
) -> anyhow::Result<()> {
    for base_arc in bases.values() {
        let (base_id, production_count) = {
            let base = base_arc.read().await;
            (base.id(), base.production_count().clone())
        };

        if let Err(err) = credit_exchange_service
            .set_base_production(base_id, &production_count)
            .await
        {
            log::error!("failed to publish credit production for base {base_id:?}: {err}");
            continue;
        }

        base_arc.write().await.clear_production_count();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BaseTargetValidationError, validate_target_bloc};
    use crate::domain::BlocKey;

    #[test]
    fn rejects_target_in_same_bloc() {
        let bloc = BlocKey::from("bloc".to_owned());

        assert_eq!(
            validate_target_bloc(&bloc, &bloc),
            Err(BaseTargetValidationError::SameBloc)
        );
    }

    #[test]
    fn accepts_target_in_different_bloc() {
        let base_bloc = BlocKey::from("base-bloc".to_owned());
        let target_bloc = BlocKey::from("target-bloc".to_owned());

        assert_eq!(validate_target_bloc(&base_bloc, &target_bloc), Ok(()));
    }
}
