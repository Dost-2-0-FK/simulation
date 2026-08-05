mod movement;
mod production;

use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

pub(crate) use crate::simulation::command::unit::{movement::*, production::*};
use crate::{
    config::Config,
    domain::{BlocKey, MilitaryUnit, Target, UnitId},
    error::UserError,
    geometry::{Point, Positioned, WorldBounds},
    handlers::units::{UnitResponse, UnitTargetResponse},
};

pub(crate) async fn get(
    resp: Sender<core::result::Result<Vec<UnitResponse>, UserError>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    config: &Config,
) {
    let spatial_index = UnitSpatialIndex::snapshot(units).await;
    let unit_responses = stream::iter(units.values())
        .then(async |unit| {
            let unit_guard = unit.read().await;
            let base_guard = unit_guard.base().await;
            let bloc_key = base_guard.bloc_key().clone();
            let target = effective_target(
                unit_guard.position(),
                &bloc_key,
                base_guard.target(),
                &spatial_index,
                config.world_bounds(),
            )
            .await;
            let base_response =
                crate::handlers::bases::BaseResponse::new(&base_guard, config.name_mappings().as_ref())?;
            Ok(UnitResponse::new(&unit_guard, Some(base_response), target))
        })
        .collect::<Vec<core::result::Result<_, UserError>>>()
        .await;
    let _ = resp.send(unit_responses.into_iter().collect());
}

/// Computes the effective target a unit would move toward right now and returns it as a
/// `UnitTargetResponse` suitable for HTTP responses.
async fn effective_target(
    from: Point,
    unit_bloc: &BlocKey,
    target: &Target,
    spatial_index: &UnitSpatialIndex,
    world_bounds: WorldBounds,
) -> UnitTargetResponse {
    let target_point = match target {
        Target::None => None,
        Target::Base { base, .. } => Some(base.read().await.position()),
        Target::Trust { trust, .. } => Some(trust.read().await.position()),
    };

    match select_move_target(from, unit_bloc, target_point, spatial_index, world_bounds) {
        MoveTo::None => UnitTargetResponse::None,
        MoveTo::EnemyUnit(id, position) => UnitTargetResponse::Unit { id, position },
        MoveTo::Designated(position) => match target {
            Target::Base { id, .. } => UnitTargetResponse::Base { id: *id, position },
            Target::Trust { id, .. } => UnitTargetResponse::Trust { id: *id, position },
            Target::None => panic!("Designated requires a non-None target"),
        },
    }
}
