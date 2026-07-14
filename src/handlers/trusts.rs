use actix_web::{HttpResponse, Responder, get, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{PlacementId, Trust, TrustId, ZoneName},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    handlers::bases::Financing,
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
    income: Money,
    producing: Resources,
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
            income: trust.income(),
            producing: trust.producing().clone(),
        }
    }
}

/// Create a trust on a placement.
#[utoipa::path(
    operation_id = "createTrust",
    tag = TRUSTS,
    responses(
        (status = 200, description = "Trust created successfully")
    ),
)]
#[post("/trusts")]
pub(crate) async fn post(
    body: web::Json<PostTrustBody>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let body = body.into_inner();

    tx.send(Command::CreateTrust {
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
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetTrusts(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let trusts = receiver.await.map_err(|e| {
        log::error!("Error receiving trusts: {e}");
        UserError::InternalError
    })?;

    let trusts = trusts.iter().map(TrustResponse::from).collect::<Vec<_>>();
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
pub(crate) async fn get(path: web::Path<u64>, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
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

    let trust = trust
        .as_ref()
        .map(TrustResponse::from)
        .ok_or(UserError::NotFound("Trust"))?;
    Ok(HttpResponse::Ok().json(trust))
}
