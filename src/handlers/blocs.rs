use actix_session::Session;
use actix_web::{HttpResponse, Responder, get, patch, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{Bloc, BlocName, Chance},
    error::{Result, UserError},
    handlers::require_bloc_write,
    services::{
        coordination_service::{CoordinationCapability, OptionalCoordinationAuthorization},
        credit_exchange_service::Share,
    },
    simulation::Command,
};

const BLOCS: &str = "blocs";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlocResponse {
    key: BlocName,
    name: String,
    chance: Chance,
    military_expense: Share,
}

impl From<&Bloc> for BlocResponse {
    fn from(bloc: &Bloc) -> Self {
        Self {
            key: bloc.name().clone(),
            name: bloc.display_name().to_owned(),
            chance: bloc.chance(),
            military_expense: bloc.military_expense(),
        }
    }
}

fn resolve_key<'a>(blocs: &'a [Bloc], requested_id: &str) -> Option<&'a BlocName> {
    blocs
        .iter()
        .find(|bloc| bloc.name().to_string() == requested_id || bloc.display_name() == requested_id)
        .map(Bloc::name)
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PatchBlocBody {
    chance: Option<u32>,
    military_expense: Option<Share>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchBlocAuthorization {
    BlocWrite,
    CoordinationService,
}

impl PatchBlocBody {
    fn required_authorization(&self) -> Result<PatchBlocAuthorization> {
        match (self.chance.is_some(), self.military_expense.is_some()) {
            (true, true) => Err(UserError::BadRequest(
                "chance and militaryExpense must be updated in separate requests",
            )),
            (true, false) => Ok(PatchBlocAuthorization::CoordinationService),
            (false, _) => Ok(PatchBlocAuthorization::BlocWrite),
        }
    }
}

/// List all blocs.
#[utoipa::path(
    operation_id = "listBlocs",
    tag = BLOCS,
    responses(
        (status = 200, description = "All existing blocs", body = [BlocResponse]),
        (status = 500, description = "Failed to retrieve blocs", body = String, content_type = "text/html")
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

    let blocs = blocs.iter().map(BlocResponse::from).collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(blocs))
}

/// Get a bloc by ID.
#[utoipa::path(
    operation_id = "getBloc",
    tag = BLOCS,
    responses(
        (status = 200, description = "Existing bloc", body = BlocResponse),
        (status = 404, description = "Bloc not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to retrieve the bloc", body = String, content_type = "text/html")
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

    let resolved = resolve_key(&blocs, &id);
    let bloc = blocs
        .iter()
        .find(|bloc| Some(bloc.name()) == resolved)
        .map(BlocResponse::from)
        .ok_or(UserError::NotFound("Bloc"))?;

    Ok(HttpResponse::Ok().json(bloc))
}

/// Update a bloc.
#[utoipa::path(
    operation_id = "patchBloc",
    tag = BLOCS,
    responses(
        (status = 200, description = "Bloc updated successfully"),
        (status = 400, description = "Chance and military expense must be updated separately", body = String, content_type = "text/html"),
        (status = 401, description = "Missing or invalid user or coordination service credentials", body = String, content_type = "text/html"),
        (status = 403, description = "Missing required bloc-write or coordination-service permission", body = String, content_type = "text/html"),
        (status = 404, description = "Bloc not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to update the bloc", body = String, content_type = "text/html")
    )
)]
#[patch("/blocs/{id}")]
pub(crate) async fn patch(
    session: Session,
    coordination_authorization: OptionalCoordinationAuthorization,
    path: web::Path<String>,
    body: web::Json<PatchBlocBody>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let requested_id = path.into_inner();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBlocs(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;
    let blocs = receiver.await.map_err(|e| {
        log::error!("Error receiving blocs: {e}");
        UserError::InternalError
    })?;
    let id = resolve_key(&blocs, &requested_id)
        .cloned()
        .ok_or(UserError::NotFound("Bloc"))?;
    match body.required_authorization()? {
        PatchBlocAuthorization::BlocWrite => require_bloc_write(&session, &id)?,
        PatchBlocAuthorization::CoordinationService => {
            coordination_authorization.require(CoordinationCapability::UpdateBlocChance)?
        }
    }

    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::PatchBloc {
        id,
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

#[cfg(test)]
mod authorization_tests {
    use super::{PatchBlocAuthorization, PatchBlocBody};
    use crate::services::credit_exchange_service::Share;

    #[test]
    fn chance_requires_coordination_service_authorization() {
        let body = PatchBlocBody {
            chance: Some(42),
            military_expense: None,
        };

        assert_eq!(
            body.required_authorization().unwrap(),
            PatchBlocAuthorization::CoordinationService
        );
    }

    #[test]
    fn military_expense_and_empty_patches_require_bloc_write_authorization() {
        for body in [
            PatchBlocBody {
                chance: None,
                military_expense: Some(Share::from(0.5)),
            },
            PatchBlocBody {
                chance: None,
                military_expense: None,
            },
        ] {
            assert_eq!(
                body.required_authorization().unwrap(),
                PatchBlocAuthorization::BlocWrite
            );
        }
    }

    #[test]
    fn chance_and_military_expense_cannot_be_patched_together() {
        let body = PatchBlocBody {
            chance: Some(42),
            military_expense: Some(Share::from(0.5)),
        };

        assert!(body.required_authorization().is_err());
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::resolve_key;
    use crate::{
        domain::{Bloc, BlocName, Chance},
        services::credit_exchange_service::Share,
    };

    #[test]
    fn resolves_both_stable_key_and_display_name_to_key() {
        let blocs = [Bloc::new(
            BlocName::from("bloc-key".to_string()),
            "WEST".to_string(),
            Chance::new(10),
            Share::default(),
        )];

        for input in ["bloc-key", "WEST"] {
            assert_eq!(resolve_key(&blocs, input).unwrap().to_string(), "bloc-key");
        }
        assert!(resolve_key(&blocs, "unknown").is_none());
    }
}
