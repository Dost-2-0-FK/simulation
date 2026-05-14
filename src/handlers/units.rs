use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{BaseId, BlocName, MilitaryUnit},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    handlers::bases::BaseResponse,
    state::Command,
};

const UNITS: &str = "units";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnitResponse {
    base_id: BaseId,
    base: Option<BaseResponse>,
    bloc: Option<BlocName>,
    position: Point,
}

impl UnitResponse {
    pub(crate) fn new(unit: &MilitaryUnit, base: Option<BaseResponse>) -> Self {
        let bloc = base.as_ref().map(|base| base.bloc.clone());
        Self {
            base_id: unit.base_id(),
            base,
            bloc,
            position: unit.position(),
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
