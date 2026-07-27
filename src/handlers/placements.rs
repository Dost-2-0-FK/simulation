use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{Placement, PlacementId},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    simulation::Command,
};

const PLACEMENTS: &str = "placements";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacementResponse {
    id: PlacementId,
    zone: String,
    position: Point,
}

impl From<&Placement> for PlacementResponse {
    fn from(placement: &Placement) -> Self {
        Self {
            id: placement.id().clone(),
            zone: placement.zone().display_name().to_owned(),
            position: placement.position(),
        }
    }
}

/// List all placements.
#[utoipa::path(
    operation_id = "listPlacements",
    tag = PLACEMENTS,
    responses(
        (status = 200, description = "All existing placements", body = [PlacementResponse]),
        (status = 500, description = "Failed to retrieve placements", body = String, content_type = "text/html")
    )
)]
#[get("/placements")]
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetPlacements(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let placements = receiver.await.map_err(|e| {
        log::error!("Error receiving placements: {e}");
        UserError::InternalError
    })?;

    let placements = placements
        .iter()
        .map(|placement| PlacementResponse::from(placement.as_ref()))
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(placements))
}

/// Get a placement by ID.
#[utoipa::path(
    operation_id = "getPlacement",
    tag = PLACEMENTS,
    responses(
        (status = 200, description = "Existing placement", body = PlacementResponse),
        (status = 404, description = "Placement not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to retrieve the placement", body = String, content_type = "text/html")
    )
)]
#[get("/placements/{id}")]
pub(crate) async fn get(path: web::Path<String>, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetPlacements(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let id = path.into_inner();
    let placements = receiver.await.map_err(|e| {
        log::error!("Error receiving placements: {e}");
        UserError::InternalError
    })?;

    let placement = placements
        .iter()
        .find(|placement| placement.id().to_string() == id)
        .map(|placement| PlacementResponse::from(placement.as_ref()))
        .ok_or(UserError::NotFound("Placement"))?;

    Ok(HttpResponse::Ok().json(placement))
}
