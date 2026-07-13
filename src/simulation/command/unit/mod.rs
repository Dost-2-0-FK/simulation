mod movement;
mod production;

use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

pub(crate) use crate::simulation::command::unit::{movement::*, production::*};
use crate::{
    domain::{BlocName, MilitaryUnit, Target, UnitId},
    geometry::{Point, Positioned, WorldBounds},
    handlers::units::{UnitResponse, UnitTargetResponse},
};

pub(crate) async fn get(
    resp: Sender<Vec<UnitResponse>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    world_bounds: WorldBounds,
) {
    let unit_responses = stream::iter(units.values())
        .then(async |unit| {
            let unit_guard = unit.read().await;
            let base_guard = unit_guard.base().await;
            let bloc_name = base_guard.bloc_name().clone();
            let target = effective_target(
                unit_guard.id(),
                unit_guard.position(),
                &bloc_name,
                base_guard.target(),
                units,
                world_bounds,
            )
            .await;
            let base_response = (&(*base_guard)).into();
            UnitResponse::new(&unit_guard, Some(base_response), target)
        })
        .collect()
        .await;
    let _ = resp.send(unit_responses);
}

/// Computes the effective target a unit would move toward right now and returns it as a
/// `UnitTargetResponse` suitable for HTTP responses.
async fn effective_target(
    unit_id: UnitId,
    from: Point,
    unit_bloc: &BlocName,
    target: &Target,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    world_bounds: WorldBounds,
) -> UnitTargetResponse {
    let target_point = match target {
        Target::None => None,
        Target::Base { base, .. } => Some(base.read().await.position()),
        Target::Trust { trust, .. } => Some(trust.read().await.position()),
    };

    match select_move_target(from, unit_id, unit_bloc, target_point, units, world_bounds).await {
        MoveTo::None => UnitTargetResponse::None,
        MoveTo::EnemyUnit(id, position) => UnitTargetResponse::Unit { id, position },
        MoveTo::Designated(position) => match target {
            Target::Base { id, .. } => UnitTargetResponse::Base { id: *id, position },
            Target::Trust { id, .. } => UnitTargetResponse::Trust { id: *id, position },
            Target::None => panic!("Designated requires a non-None target"),
        },
    }
}
