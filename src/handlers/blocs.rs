use actix_web::{HttpResponse, Responder, get, patch, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{Bloc, BlocName, Chance},
    error::{Result, UserError},
    state::Command,
};

const BLOCS: &str = "blocs";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlocResponse {
    name: BlocName,
    chance: Chance,
    military_expense: u32,
}

impl From<&Bloc> for BlocResponse {
    fn from(bloc: &Bloc) -> Self {
        Self {
            name: bloc.name().clone(),
            chance: bloc.chance(),
            military_expense: bloc.military_expense(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PatchBlocBody {
    chance: Option<f32>,
    military_expense: Option<u32>,
}

/// List all blocs.
#[utoipa::path(
    operation_id = "listBlocs",
    tag = BLOCS,
    responses(
        (status = 200, description = "All existing blocs")
    )
)]
#[get("/blocs")]
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBlocs(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let blocs = receiver.await.map_err(|e| {
        log::error!("Error receiving blocs: {e}");
        UserError::InternalError
    })?;

    let blocs = blocs
        .iter()
        .map(BlocResponse::from)
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(blocs))
}

/// Get a bloc by ID.
#[utoipa::path(
    operation_id = "getBloc",
    tag = BLOCS,
    responses(
        (status = 200, description = "Existing bloc")
    )
)]
#[get("/blocs/{id}")]
pub(crate) async fn get(path: web::Path<String>, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBlocs(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let id = path.into_inner();
    let blocs = receiver.await.map_err(|e| {
        log::error!("Error receiving blocs: {e}");
        UserError::InternalError
    })?;

    let bloc = blocs
        .iter()
        .find(|bloc| bloc.name().to_string() == id)
        .map(BlocResponse::from)
        .ok_or(UserError::NotFound("Bloc"))?;

    Ok(HttpResponse::Ok().json(bloc))
}

/// Update a bloc.
#[utoipa::path(
    operation_id = "patchBloc",
    tag = BLOCS,
    responses(
        (status = 200, description = "Bloc updated successfully")
    )
)]
#[patch("/blocs/{id}")]
pub(crate) async fn patch(
    path: web::Path<String>,
    body: web::Json<PatchBlocBody>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::PatchBloc {
        id: BlocName::from(path.into_inner()),
        chance: body.chance.map(Chance::new),
        military_expense: body.military_expense,
        response: sender,
    })
    .await
    .map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let result = receiver.await.map_err(|e| {
        log::error!("Error receiving bloc patch result: {e}");
        UserError::InternalError
    })?;

    result.map(|()| HttpResponse::Ok())
}
