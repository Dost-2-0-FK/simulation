use actix_web::{HttpResponse, Responder, get, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    Command,
    error::{Result, UserError},
    placement::PlacementId,
};

const BASES: &str = "bases";

#[derive(Debug, Copy, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct Percentage(f32);

#[expect(dead_code)]
impl Percentage {
    pub(crate) fn as_factor(self) -> f32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Percentage {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let percent = f32::deserialize(deserializer)?;

        if !(0.0..=100.0).contains(&percent) {
            Err(serde::de::Error::custom("percentage must be between 0 and 100"))
        } else {
            Ok(Percentage(percent / 100.0))
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct UserId(String);

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct Financing {
    #[serde(rename = "financierId")]
    pub(crate) financier: UserId,
    pub(crate) percentage: Percentage,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct PostBaseBody {
    placement_id: PlacementId,
    financing: Financing,
}

/// Create a base on a placement.
#[utoipa::path(
    tag = BASES,
    responses(
        (status = 200, description = "Base created successfully")
    ),
)]
#[post("/bases")]
async fn post(body: web::Json<PostBaseBody>, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (create_base_tx, result_rx) = tokio::sync::oneshot::channel();
    let body = body.into_inner();

    tx.send(Command::CreateBase {
        placement_id: body.placement_id,
        financing: body.financing,
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
    tag = BASES,
    responses(
        (status = 200, description = "All existing bases")
    )
)]
#[get("/bases")]
async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBases(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let bases = receiver.await.map_err(|e| {
        log::error!("Error receiving bases: {e}");
        UserError::InternalError
    })?;

    let response = HttpResponse::Ok().json(bases.as_slice());
    Ok(response)
}
