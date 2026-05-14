use actix_web::{HttpResponse, Responder, get, patch, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{BaseId, BlocName, MilitaryBase, PlacementId, Target, ZoneName},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    state::Command,
};

const BASES: &str = "bases";

/// A value between 0 and 1 representing a financier's share of a payment.
#[derive(Debug, Copy, Clone, PartialEq, utoipa::ToSchema)]
pub struct Share(f32);

impl<'de> Deserialize<'de> for Share {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let share = f32::deserialize(deserializer)?;

        if !(0.0..=1.0).contains(&share) {
            Err(serde::de::Error::custom("share must be between 0 and 1"))
        } else {
            Ok(Share(share))
        }
    }
}

impl Serialize for Share {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f32(self.0)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct UserId(String);

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct Financing {
    #[serde(rename = "financierId")]
    pub(crate) financier: UserId,
    pub(crate) share: Share,
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
    pub(crate) prioritized: bool,
    pub(crate) target: Target,
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
            bloc: zone.bloc().name().clone(),
            payment: base.financiers().to_vec(),
            prioritized: base.prioritized(),
            target: base.target(),
            position: base.position(),
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct PatchBaseBody {
    prioritized: Option<bool>,
    target: Option<Target>,
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
            UserError::InternalError => todo!(),
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
    tx.send(Command::PatchBase {
        id: BaseId(path.into_inner()),
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

pub(crate) fn base_by_id(bases: &[MilitaryBase], id: BaseId) -> Option<&MilitaryBase> {
    bases.iter().find(|base| base.id().0 == id.0)
}

pub(crate) fn base_response_by_id(bases: &[MilitaryBase], id: BaseId) -> Option<BaseResponse> {
    base_by_id(bases, id).map(BaseResponse::from)
}
