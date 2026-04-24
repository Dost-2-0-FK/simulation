use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    Command,
    error::{Result, UserError},
    geometry::{Point, Positioned},
    placement::{Placement, PlacementId},
    politics::ZoneName,
};

const PLACEMENTS: &str = "placements";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacementResponse {
    id: PlacementId,
    zone: ZoneName,
    position: Point,
}

impl From<&Placement> for PlacementResponse {
    fn from(placement: &Placement) -> Self {
        Self {
            id: placement.id().clone(),
            zone: placement.zone().name().clone(),
            position: placement.position(),
        }
    }
}

/// List all placements.
#[utoipa::path(
    operation_id = "listPlacements",
    tag = PLACEMENTS,
    responses(
        (status = 200, description = "All existing placements")
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
        (status = 200, description = "Existing placement")
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
