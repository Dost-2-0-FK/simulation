use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tokio::sync::RwLock;

use crate::{
    domain::{BaseId, Combat, CombatStructureSnapshot, MilitaryBase, MilitaryUnit, Trust, TrustId, UnitId},
    error::UserError,
    geometry::Point,
    services::credit_exchange_service::CreditExchangeService,
};

use super::combat;

pub(crate) async fn delete_base(
    id: BaseId,
    bases: &mut HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    credit_exchange_service: &CreditExchangeService,
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

    let mut unit_ids = HashSet::new();
    for (unit_id, unit) in units.iter() {
        if unit.read().await.base().await.id() == id {
            unit_ids.insert(*unit_id);
        }
    }

    let mut combat_positions = Vec::new();
    for (position, combat) in combats.iter() {
        let combat = combat.read().await;
        let attacks_base = matches!(
            combat.structure_snapshot().await,
            CombatStructureSnapshot::Base { id: target_id, .. } if target_id == id
        );
        let contains_deleted_unit = combat
            .unit_ids_by_bloc()
            .await
            .into_iter()
            .flat_map(|(_, ids)| ids)
            .any(|unit_id| unit_ids.contains(&unit_id));
        if attacks_base || contains_deleted_unit {
            combat_positions.push(*position);
        }
    }

    units.retain(|unit_id, _| !unit_ids.contains(unit_id));
    combats.retain(|position, _| !combat_positions.contains(position));
    combat::destroy_base(id, bases).await;
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
