use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    config::PoliticsDirectory,
    domain::{BlocName, Zone, ZoneName},
    error::{Result, UserError},
    simulation::Command,
};

const ZONES: &str = "zones";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZoneResponse {
    key: ZoneName,
    name: String,
    bloc: BlocName,
}

impl From<&Zone> for ZoneResponse {
    fn from(zone: &Zone) -> Self {
        Self {
            key: zone.name().clone(),
            name: zone.display_name().to_owned(),
            bloc: zone.bloc_name().clone(),
        }
    }
}

impl ZoneResponse {
    fn display_politics(&mut self, politics: &PoliticsDirectory) -> Result<()> {
        self.bloc = BlocName::from(
            politics
                .bloc_name(&self.bloc)
                .ok_or(UserError::InternalError)?
                .to_owned(),
        );
        Ok(())
    }
}

/// List all zones.
#[utoipa::path(
    operation_id = "listZones",
    tag = ZONES,
    responses(
        (status = 200, description = "All existing zones", body = [ZoneResponse]),
        (status = 500, description = "Failed to retrieve zones", body = String, content_type = "text/html")
    )
)]
#[get("/zones")]
pub(crate) async fn list(
    tx: web::Data<mpsc::Sender<Command>>,
    politics: web::Data<PoliticsDirectory>,
) -> Result<impl Responder> {
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
        .map(|zone| {
            let mut response = ZoneResponse::from(zone.as_ref());
            response.display_politics(&politics)?;
            Ok(response)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(HttpResponse::Ok().json(zones))
}
