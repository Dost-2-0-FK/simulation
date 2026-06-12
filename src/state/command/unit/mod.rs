mod movement;
mod production;

use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

pub(crate) use crate::state::command::unit::{movement::*, production::*};
use crate::{
    domain::{BlocName, MilitaryUnit, Target, UnitId},
    geometry::{Point, Positioned},
    handlers::units::{UnitResponse, UnitTargetResponse},
};

pub(crate) async fn get(resp: Sender<Vec<UnitResponse>>, units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>) {
    // Pre-collect (BlocName, UnitId, Point) for enemy-unit detection.
    let mut all_units_snapshot: Vec<(BlocName, UnitId, Point)> = Vec::with_capacity(units.len());
    for (unit_id, unit_arc) in units.iter() {
        let unit = unit_arc.read().await;
        let base = unit.base().await;
        let bloc_name = base.placement().zone().bloc().name().clone();
        all_units_snapshot.push((bloc_name, unit_id.clone(), unit.position()));
    }

    let unit_responses = stream::iter(units.values())
        .then(async |unit| {
            let unit_guard = unit.read().await;
            let base_guard = unit_guard.base().await;
            let bloc_name = base_guard.placement().zone().bloc().name().clone();
            let target = effective_target(
                unit_guard.id(),
                unit_guard.position(),
                &bloc_name,
                base_guard.target(),
                &all_units_snapshot,
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
    unit_id: &UnitId,
    from: Point,
    unit_bloc: &BlocName,
    target: &Target,
    all_units: &[(BlocName, UnitId, Point)],
) -> UnitTargetResponse {
    let target_point = match target {
        Target::None => None,
        Target::Base { arc, .. } => Some(arc.read().await.position()),
        Target::Trust { arc, .. } => Some(arc.read().await.position()),
    };

    match select_move_target(from, unit_id, unit_bloc, target_point, all_units) {
        MoveTo::None => UnitTargetResponse::None,
        MoveTo::EnemyUnit(id, position) => UnitTargetResponse::Unit { id, position },
        MoveTo::Designated(position) => match target {
            Target::Base { id, .. } => UnitTargetResponse::Base { id: *id, position },
            Target::Trust { id, .. } => UnitTargetResponse::Trust { id: *id, position },
            Target::None => panic!("Designated requires a non-None target"),
        },
    }
}
