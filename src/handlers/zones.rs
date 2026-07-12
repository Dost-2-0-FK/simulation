use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{BlocName, Zone, ZoneName},
    error::{Result, UserError},
    simulation::Command,
};

const ZONES: &str = "zones";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZoneResponse {
    name: ZoneName,
    bloc: BlocName,
}

impl From<&Zone> for ZoneResponse {
    fn from(zone: &Zone) -> Self {
        Self {
            name: zone.name().clone(),
            bloc: zone.bloc_name().clone(),
        }
    }
}

/// List all zones.
#[utoipa::path(
    operation_id = "listZones",
    tag = ZONES,
    responses(
        (status = 200, description = "All existing zones")
    )
)]
#[get("/zones")]
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetZones(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let zones = receiver.await.map_err(|e| {
        log::error!("Error receiving zones: {e}");
        UserError::InternalError
    })?;

    let zones = zones
        .iter()
        .map(|zone| ZoneResponse::from(zone.as_ref()))
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(zones))
}
