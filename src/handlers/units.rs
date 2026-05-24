use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{BaseId, BlocName, MilitaryUnit, TrustId, UnitId},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    handlers::bases::BaseResponse,
    state::Command,
};

const UNITS: &str = "units";

/// Serialized representation of a unit's effective current target in HTTP responses. Carries the
/// entity's ID and its current position. `Unit` is used when an enemy unit is being chased,
/// `None` when there is no designated target and no enemies are present.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum UnitTargetResponse {
    None,
    Unit { id: UnitId, position: Point },
    Base { id: BaseId, position: Point },
    Trust { id: TrustId, position: Point },
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnitResponse {
    base_id: Option<BaseId>,
    base: Option<BaseResponse>,
    bloc: Option<BlocName>,
    position: Point,
    target: UnitTargetResponse,
}

impl UnitResponse {
    pub(crate) fn new(unit: &MilitaryUnit, base: Option<BaseResponse>, target: UnitTargetResponse) -> Self {
        let bloc = base.as_ref().map(|b| b.bloc.clone());
        let base_id = base.as_ref().map(|b| b.id);
        Self {
            base_id,
            base,
            bloc,
            position: unit.position(),
            target,
        }
    }
}

/// List all units.
#[utoipa::path(
    operation_id = "listUnits",
    tag = UNITS,
    responses(
        (status = 200, description = "All existing units")
    )
)]
#[get("/units")]
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    // This channel is one-shot: it is only used once and gets re-created on every request
    let (get_units_tx, get_units_rx) = tokio::sync::oneshot::channel();

    // Via the global channel, send a command to the state loop to query the count. The command includes the
    // one-shot channel sender from which we're going to get the response.
    tx.send(Command::GetUnits(get_units_tx)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    // Receive the response from the state.
    let units = get_units_rx.await.map_err(|e| {
        log::error!("Error receiving count: {e}");
        UserError::InternalError
    })?;

    let response = HttpResponse::Ok().json(units);
    Ok(response)
}
