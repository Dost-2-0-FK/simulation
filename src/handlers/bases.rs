use actix_web::{HttpResponse, Responder, get, patch, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{BaseId, BlocName, MilitaryBase, PlacementId, Target, TrustId, ZoneName},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    services::credit_exchange_service::Share,
    state::Command,
};

const BASES: &str = "bases";

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct UserId(String);

impl UserId {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct Financing {
    #[serde(rename = "financierId")]
    pub(crate) financier: UserId,
    pub(crate) share: Share,
}

/// Serialized representation of a [Target] in HTTP responses for bases. Carries only the entity ID.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum BaseTargetResponse {
    None,
    Base { id: BaseId },
    Trust { id: TrustId },
}

impl From<&Target> for BaseTargetResponse {
    fn from(t: &Target) -> Self {
        match t {
            Target::None => Self::None,
            Target::Base { id, .. } => Self::Base { id: *id },
            Target::Trust { id, .. } => Self::Trust { id: *id },
        }
    }
}

/// Deserialized representation of a [Target] in PATCH request bodies. Carries only the entity ID.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum TargetBody {
    None,
    Base { id: BaseId },
    Trust { id: TrustId },
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct PostBaseBody {
    placement_id: PlacementId,
    payment: Vec<Financing>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BaseResponse {
    pub(crate) id: BaseId,
    pub(crate) placement_id: PlacementId,
    pub(crate) zone: ZoneName,
    pub(crate) bloc: BlocName,
    pub(crate) payment: Vec<Financing>,
    pub(crate) enabled: bool,
    pub(crate) prioritized: bool,
    pub(crate) target: BaseTargetResponse,
    pub(crate) position: Point,
}

impl From<&MilitaryBase> for BaseResponse {
    fn from(base: &MilitaryBase) -> Self {
        let placement = base.placement();
        let zone = placement.zone();
        Self {
            id: base.id(),
            placement_id: placement.id().clone(),
            zone: zone.name().clone(),
            bloc: zone.bloc_name().clone(),
            payment: base.financiers().to_vec(),
            enabled: base.enabled(),
            prioritized: base.prioritized(),
            target: base.target().into(),
            position: base.position(),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct PatchBaseBody {
    enabled: Option<bool>,
    prioritized: Option<bool>,
    target: Option<TargetBody>,
}

/// Create a base on a placement.
#[utoipa::path(
    operation_id = "createBase",
    tag = BASES,
    responses(
        (status = 200, description = "Base created successfully")
    ),
)]
#[post("/bases")]
pub(crate) async fn post(
    body: web::Json<PostBaseBody>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let (create_base_tx, result_rx) = tokio::sync::oneshot::channel();
    let body = body.into_inner();

    tx.send(Command::CreateBase {
        placement_id: body.placement_id,
        financing: body.payment,
        response: create_base_tx,
    })
    .await
    .map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    // Receive the response from the state.
    let result = result_rx.await.map_err(|e| {
        log::error!("Error receiving response: {e}");
        UserError::InternalError
    })?;

    result
        .inspect_err(|err| match err {
            UserError::InternalError => log::error!("internal error while creating base"),
            UserError::NotFound(err) => log::info!("not found: {err}"),
        })
        .map(|()| HttpResponse::Ok())
}

/// List all bases.
#[utoipa::path(
    operation_id = "listBases",
    tag = BASES,
    responses(
        (status = 200, description = "All existing bases")
    )
)]
#[get("/bases")]
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBases(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let bases = receiver.await.map_err(|e| {
        log::error!("Error receiving bases: {e}");
        UserError::InternalError
    })?;

    let bases = bases.iter().map(BaseResponse::from).collect::<Vec<_>>();
    let response = HttpResponse::Ok().json(bases);
    Ok(response)
}

/// Get a base by ID.
#[utoipa::path(
    operation_id = "getBase",
    tag = BASES,
    responses(
        (status = 200, description = "Existing base")
    )
)]
#[get("/bases/{id}")]
pub(crate) async fn get(path: web::Path<u64>, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBase(BaseId(path.into_inner()), sender))
        .await
        .map_err(|e| {
            log::error!("Error sending command: {e}");
            UserError::InternalError
        })?;

    let base = receiver.await.map_err(|e| {
        log::error!("Error receiving base: {e}");
        UserError::InternalError
    })?;

    let base = base
        .as_ref()
        .map(BaseResponse::from)
        .ok_or(UserError::NotFound("Base"))?;
    Ok(HttpResponse::Ok().json(base))
}

/// Update a base.
#[utoipa::path(
    operation_id = "patchBase",
    tag = BASES,
    responses(
        (status = 200, description = "Base updated successfully")
    )
)]
#[patch("/bases/{id}")]
pub(crate) async fn patch(
    path: web::Path<u64>,
    body: web::Json<PatchBaseBody>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let body = body.into_inner();
    tx.send(Command::PatchBase {
        id: BaseId(path.into_inner()),
        enabled: body.enabled,
        prioritized: body.prioritized,
        target: body.target,
        response: sender,
    })
    .await
    .map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let result = receiver.await.map_err(|e| {
        log::error!("Error receiving base patch result: {e}");
        UserError::InternalError
    })?;

    result.map(|()| HttpResponse::Ok())
}
