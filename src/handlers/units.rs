use actix_session::Session;
use actix_web::{HttpResponse, Responder, get, post, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{BaseId, BlocName, MilitaryUnit, TrustId, UnitId},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    handlers::{bases::BaseResponse, can_read_bloc},
    services::coordination_service::{CoordinationAuthorization, CoordinationCapability},
    simulation::Command,
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
    id: UnitId,
    position: Point,
    base: Option<BaseResponse>,
    bloc: Option<BlocName>,
    target: UnitTargetResponse,
}

impl UnitResponse {
    pub(crate) fn new(unit: &MilitaryUnit, base: Option<BaseResponse>, target: UnitTargetResponse) -> Self {
        let bloc = base.as_ref().map(|b| b.bloc.clone());
        Self {
            id: unit.id(),
            base,
            bloc,
            position: unit.position(),
            target,
        }
    }

    fn redact_protected_base_fields(&mut self) {
        if let Some(base) = &mut self.base {
            base.redact_protected_fields();
        }
    }
}

/// List all units.
#[utoipa::path(
    operation_id = "listUnits",
    tag = UNITS,
    responses(
        (status = 200, description = "All existing units", body = [UnitResponse]),
        (status = 500, description = "Failed to retrieve units", body = String, content_type = "text/html")
    )
)]
#[get("/units")]
pub(crate) async fn list(session: Session, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    // This channel is one-shot: it is only used once and gets re-created on every request
    let (get_units_tx, get_units_rx) = tokio::sync::oneshot::channel();

    // Via the global channel, send a command to the state loop to query the count. The command includes the
    // one-shot channel sender from which we're going to get the response.
    tx.send(Command::GetUnits(get_units_tx)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    // Receive the response from the state.
    let mut units = get_units_rx.await.map_err(|e| {
        log::error!("Error receiving count: {e}");
        UserError::InternalError
    })?;

    for unit in &mut units {
        if let Some(bloc) = &unit.bloc
            && !can_read_bloc(&session, bloc)?
        {
            unit.redact_protected_base_fields();
        }
    }

    let response = HttpResponse::Ok().json(units);
    Ok(response)
}

/// Produce military units.
#[utoipa::path(
    operation_id = "produceMilitaryUnits",
    tag = UNITS,
    responses(
        (status = 200, description = "Production task completed successfully"),
        (status = 401, description = "Missing or invalid coordination service credentials", body = String, content_type = "text/html"),
        (status = 403, description = "Coordination service lacks permission to trigger unit production", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to produce military units", body = String, content_type = "text/html")
    )
)]
#[post("/units/produce")]
pub(crate) async fn produce(
    authorization: CoordinationAuthorization,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    authorization.require(CoordinationCapability::TriggerUnitProduction)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();

    tx.send(Command::ProduceMilitaryUnits { response: sender })
        .await
        .map_err(|e| {
            log::error!("Error sending production command: {e}");
            UserError::InternalError
        })?;

    let result = receiver.await.map_err(|e| {
        log::error!("Error receiving production result: {e}");
        UserError::InternalError
    })?;

    result?;
    Ok(HttpResponse::Ok().finish())
}
