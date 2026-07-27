use actix_session::Session;
use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{ProductionUnit, ProductionUnitKey, ZoneName},
    error::{Result, UserError},
    handlers::can_read_zone,
    services::credit_exchange_service::{Money, ResourceName, Resources},
    simulation::Command,
};

const PRODUCTION_UNITS: &str = "production-units";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductionUnitResponse {
    key: ProductionUnitKey,
    zone: ZoneName,
    resource: ResourceName,
    #[serde(skip_serializing_if = "Option::is_none")]
    income: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    producing: Option<Resources>,
}

impl ProductionUnitResponse {
    pub(crate) fn new(unit: &ProductionUnit, income: Money, producing: Resources) -> Self {
        Self {
            key: unit.key().clone(),
            zone: unit.zone().name().clone(),
            resource: unit.resource_name().clone(),
            income: Some(income),
            producing: Some(producing),
        }
    }

    fn redact_protected_fields(&mut self) {
        self.income = None;
        self.producing = None;
    }
}

#[utoipa::path(
    operation_id = "listProductionUnits",
    tag = PRODUCTION_UNITS,
    responses(
        (status = 200, description = "All production units", body = [ProductionUnitResponse]),
        (status = 500, description = "Failed to retrieve production units", body = String, content_type = "text/html")
    )
)]
#[get("/production-units")]
pub(crate) async fn list(session: Session, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetProductionUnits(sender)).await.map_err(|error| {
        log::error!("Error sending production-unit list command: {error}");
        UserError::InternalError
    })?;
    let production_units = receiver.await.map_err(|error| {
        log::error!("Error receiving production units: {error}");
        UserError::InternalError
    })??;

    let production_units = production_units
        .into_iter()
        .map(|mut response| {
            if !can_read_zone(&session, &response.zone)? {
                response.redact_protected_fields();
            }
            Ok(response)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(HttpResponse::Ok().json(production_units))
}

#[utoipa::path(
    operation_id = "getProductionUnit",
    tag = PRODUCTION_UNITS,
    responses(
        (status = 200, description = "Existing production unit", body = ProductionUnitResponse),
        (status = 404, description = "Production unit not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to retrieve the production unit", body = String, content_type = "text/html")
    )
)]
#[get("/production-units/{key}")]
pub(crate) async fn get(
    session: Session,
    path: web::Path<ProductionUnitKey>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetProductionUnit(path.into_inner(), sender))
        .await
        .map_err(|error| {
            log::error!("Error sending production-unit get command: {error}");
            UserError::InternalError
        })?;
    let production_unit = receiver.await.map_err(|error| {
        log::error!("Error receiving production unit: {error}");
        UserError::InternalError
    })??;

    let mut production_unit = production_unit.ok_or(UserError::NotFound("Production unit"))?;
    if !can_read_zone(&session, &production_unit.zone)? {
        production_unit.redact_protected_fields();
    }
    Ok(HttpResponse::Ok().json(production_unit))
}

#[cfg(test)]
mod tests {
    use super::ProductionUnitResponse;
    use crate::{
        domain::{ProductionUnitKey, ZoneName},
        services::credit_exchange_service::{Money, ResourceName, Resources},
    };

    #[test]
    fn redacted_production_unit_omits_income_and_producing() {
        let mut response = ProductionUnitResponse {
            key: serde_json::from_str::<ProductionUnitKey>(r#""humans-0-0-e""#).unwrap(),
            zone: ZoneName::from("zone-e".to_string()),
            resource: ResourceName::new("humans".to_string()),
            income: Some(Money::from(0.0)),
            producing: Some(Resources::new_single(ResourceName::new("humans".to_string()), 1.0)),
        };

        response.redact_protected_fields();
        let response = serde_json::to_value(response).unwrap();

        assert!(response.get("income").is_none());
        assert!(response.get("producing").is_none());
        assert_eq!(response["key"], "humans-0-0-e");
        assert_eq!(response["resource"], "humans");
    }
}
