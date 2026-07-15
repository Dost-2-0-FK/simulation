use actix_session::Session;
use actix_web::{HttpResponse, Responder, get, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{PlacementId, Trust, TrustId, ZoneName},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    handlers::{authenticated_user, bases::Financing, can_read_zone, require_zone_write},
    services::credit_exchange_service::{Money, ResourceName, Resources},
    simulation::Command,
};

const TRUSTS: &str = "trusts";

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct PostTrustBody {
    placement_id: PlacementId,
    resource: ResourceName,
    payment: Vec<Financing>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustResponse {
    id: TrustId,
    placement_id: PlacementId,
    zone: ZoneName,
    payment: Vec<Financing>,
    position: Point,
    /// The configured monetary income. Omitted without read access to the zone.
    #[serde(skip_serializing_if = "Option::is_none")]
    income: Option<Money>,
    /// The configured resource production. Omitted without read access to the zone.
    #[serde(skip_serializing_if = "Option::is_none")]
    producing: Option<Resources>,
}

impl From<&Trust> for TrustResponse {
    fn from(trust: &Trust) -> Self {
        let placement = trust.placement();
        Self {
            id: trust.id(),
            placement_id: placement.id().clone(),
            zone: placement.zone().name().clone(),
            payment: trust.financing().to_vec(),
            position: trust.position(),
            income: Some(trust.income()),
            producing: Some(trust.producing().clone()),
        }
    }
}

impl TrustResponse {
    fn redact_protected_fields(&mut self) {
        self.income = None;
        self.producing = None;
    }
}

/// Create a trust on a placement.
#[utoipa::path(
    operation_id = "createTrust",
    tag = TRUSTS,
    responses(
        (status = 200, description = "Trust created successfully"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "No write permission for the zone"),
        (status = 409, description = "Placement is already occupied"),
    ),
)]
#[post("/trusts")]
pub(crate) async fn post(
    session: Session,
    body: web::Json<PostTrustBody>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let body = body.into_inner();
    let user = authenticated_user(&session)?.ok_or(UserError::Unauthorized)?;

    let (placements_tx, placements_rx) = tokio::sync::oneshot::channel();
    tx.send(Command::GetPlacements(placements_tx)).await.map_err(|e| {
        log::error!("Error sending placements command: {e}");
        UserError::InternalError
    })?;
    let placements = placements_rx.await.map_err(|e| {
        log::error!("Error receiving placements: {e}");
        UserError::InternalError
    })?;
    let zone = placements
        .iter()
        .find(|placement| placement.id() == &body.placement_id)
        .map(|placement| placement.zone().name())
        .ok_or(UserError::NotFound("Placement"))?;
    require_zone_write(&session, zone)?;

    let (sender, receiver) = tokio::sync::oneshot::channel();

    tx.send(Command::CreateTrust {
        user_id: user.user_id().clone(),
        placement_id: body.placement_id,
        financing: body.payment,
        resource: body.resource,
        response: sender,
    })
    .await
    .map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let result = receiver.await.map_err(|e| {
        log::error!("Error receiving trust creation result: {e}");
        UserError::InternalError
    })?;

    result?;
    Ok(HttpResponse::Ok().finish())
}

/// Publish trust production to the credit service.
#[utoipa::path(
    operation_id = "publishTrustProduction",
    tag = TRUSTS,
    responses(
        (status = 200, description = "Trust production published successfully")
    )
)]
#[post("/trusts/publish-production")]
pub(crate) async fn publish_production(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();

    tx.send(Command::PublishTrustProduction { response: sender })
        .await
        .map_err(|e| {
            log::error!("Error sending trust production publish command: {e}");
            UserError::InternalError
        })?;

    let result = receiver.await.map_err(|e| {
        log::error!("Error receiving trust production publish result: {e}");
        UserError::InternalError
    })?;

    result?;
    Ok(HttpResponse::Ok().finish())
}

/// List all trusts.
#[utoipa::path(
    operation_id = "listTrusts",
    tag = TRUSTS,
    responses(
        (status = 200, description = "All existing trusts")
    )
)]
#[get("/trusts")]
pub(crate) async fn list(session: Session, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetTrusts(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let trusts = receiver.await.map_err(|e| {
        log::error!("Error receiving trusts: {e}");
        UserError::InternalError
    })?;

    let trusts = trusts
        .iter()
        .map(|trust| {
            let mut response = TrustResponse::from(trust);
            if !can_read_zone(&session, &response.zone)? {
                response.redact_protected_fields();
            }
            Ok(response)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(HttpResponse::Ok().json(trusts))
}

/// Get a trust by ID.
#[utoipa::path(
    operation_id = "getTrust",
    tag = TRUSTS,
    responses(
        (status = 200, description = "Existing trust")
    )
)]
#[get("/trusts/{id}")]
pub(crate) async fn get(
    session: Session,
    path: web::Path<u64>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetTrust(TrustId(path.into_inner()), sender))
        .await
        .map_err(|e| {
            log::error!("Error sending command: {e}");
            UserError::InternalError
        })?;

    let trust = receiver.await.map_err(|e| {
        log::error!("Error receiving trust: {e}");
        UserError::InternalError
    })?;

    let mut trust = trust
        .as_ref()
        .map(TrustResponse::from)
        .ok_or(UserError::NotFound("Trust"))?;
    if !can_read_zone(&session, &trust.zone)? {
        trust.redact_protected_fields();
    }
    Ok(HttpResponse::Ok().json(trust))
}

#[cfg(test)]
mod tests {
    use ordered_float::NotNan;

    use super::TrustResponse;
    use crate::{
        domain::{PlacementId, TrustId, ZoneName},
        geometry::Point,
        services::credit_exchange_service::{Money, ResourceName, Resources},
    };

    #[test]
    fn redacted_trust_omits_income_and_producing() {
        let mut response = TrustResponse {
            id: TrustId(1),
            placement_id: serde_json::from_str::<PlacementId>(r#""placement""#).unwrap(),
            zone: ZoneName::from("zone-w".to_string()),
            payment: vec![],
            position: Point::new(NotNan::new(1.0).unwrap(), NotNan::new(2.0).unwrap()),
            income: Some(Money::from(10.0)),
            producing: Some(Resources::new_single(ResourceName::new("oil".to_string()), 2.0)),
        };

        let visible_response = serde_json::to_value(&response).unwrap();
        assert!(visible_response.get("income").is_some());
        assert!(visible_response.get("producing").is_some());

        response.redact_protected_fields();
        let response = serde_json::to_value(response).unwrap();

        assert!(response.get("income").is_none());
        assert!(response.get("producing").is_none());
    }
}
