use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use super::combat;
use crate::{
    domain::{BaseId, Combat, CombatStructureSnapshot, MilitaryBase, MilitaryUnit, Trust, TrustId, UnitId},
    error::UserError,
    geometry::{Point, WorldBounds},
    services::credit_exchange_service::CreditExchangeService,
};

pub(crate) async fn delete_base(
    id: BaseId,
    bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    credit_exchange_service: &CreditExchangeService,
    world_bounds: WorldBounds,
) -> Result<(), UserError> {
    if !bases.contains_key(&id) {
        return Err(UserError::NotFound("Base"));
    }

    credit_exchange_service
        .delete_base_subscriptions(id)
        .await
        .map_err(|error| {
            log::error!("failed to delete credit subscriptions for base {id:?}: {error:#}");
            UserError::InternalError
        })?;

    combat::destroy_base(id, bases, units, combats, world_bounds).await;
    Ok(())
}

pub(crate) async fn delete_trust(
    id: TrustId,
    bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &mut HashMap<TrustId, Arc<RwLock<Trust>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    credit_exchange_service: &CreditExchangeService,
) -> Result<(), UserError> {
    if !trusts.contains_key(&id) {
        return Err(UserError::NotFound("Trust"));
    }

    credit_exchange_service
        .delete_trust_subscriptions(id)
        .await
        .map_err(|error| {
            log::error!("failed to delete credit subscriptions for trust {id:?}: {error:#}");
            UserError::InternalError
        })?;

    let mut combat_positions = Vec::new();
    for (position, combat) in combats.iter() {
        if matches!(
            combat.read().await.structure_snapshot().await,
            CombatStructureSnapshot::Trust { id: target_id, .. } if target_id == id
        ) {
            combat_positions.push(*position);
        }
    }

    combats.retain(|position, _| !combat_positions.contains(position));
    combat::destroy_trust(id, bases, trusts).await;
    Ok(())
}
