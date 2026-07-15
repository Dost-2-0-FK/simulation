use actix_session::Session;
use actix_web::{HttpResponse, Responder, get, patch, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{BaseId, BlocName, Loot, MilitaryBase, PlacementId, Target, TrustId, ZoneName},
    error::{Result, UserError},
    geometry::{Point, Positioned},
    handlers::{authenticated_user, can_read_bloc, require_bloc_write},
    services::credit_exchange_service::Share,
    simulation::Command,
};

const BASES: &str = "bases";

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema, derive_more::From, derive_more::Into)]
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
    pub(crate) position: Point,
    pub(crate) placement_id: PlacementId,
    pub(crate) bloc: BlocName,
    pub(crate) zone: ZoneName,
    pub(crate) enabled: bool,
    pub(crate) prioritized: bool,
    pub(crate) payment: Vec<Financing>,
    /// How much loot has accumulated since the last transfer. Omitted without read access to the bloc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) production_count: Option<Loot>,
    /// The base's configured target. Omitted without read access to the bloc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<BaseTargetResponse>,
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
            target: Some(base.target().into()),
            position: base.position(),
            production_count: Some(base.production_count().clone()),
        }
    }
}

impl BaseResponse {
    pub(crate) fn redact_protected_fields(&mut self) {
        self.production_count = None;
        self.target = None;
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
        (status = 200, description = "Base created successfully"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "No write permission for the bloc"),
        (status = 409, description = "Placement is already occupied"),
    ),
)]
#[post("/bases")]
pub(crate) async fn post(
    session: Session,
    body: web::Json<PostBaseBody>,
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
    let bloc = placements
        .iter()
        .find(|placement| placement.id() == &body.placement_id)
        .map(|placement| placement.zone().bloc_name())
        .ok_or(UserError::NotFound("Placement"))?;
    require_bloc_write(&session, bloc)?;

    let (create_base_tx, result_rx) = tokio::sync::oneshot::channel();

    tx.send(Command::CreateBase {
        user_id: user.user_id().clone(),
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

    if let Err(err) = &result {
        match err {
            UserError::InternalError => log::error!("internal error while creating base"),
            UserError::NotFound(err) => log::info!("not found: {err}"),
            UserError::Conflict(err) => log::info!("conflict: {err}"),
            UserError::Unauthorized => log::info!("unauthorized while creating base"),
            UserError::Forbidden => log::info!("forbidden while creating base"),
        }
    }

    result?;
    Ok(HttpResponse::Ok().finish())
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
pub(crate) async fn list(session: Session, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBases(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let bases = receiver.await.map_err(|e| {
        log::error!("Error receiving bases: {e}");
        UserError::InternalError
    })?;

    let bases = bases
        .iter()
        .map(|base| {
            let mut response = BaseResponse::from(base);
            if !can_read_bloc(&session, &response.bloc)? {
                response.redact_protected_fields();
            }
            Ok(response)
        })
        .collect::<Result<Vec<_>>>()?;
    let response = HttpResponse::Ok().json(bases);
    Ok(response)
}

/// Publish accumulated base loot to the credit service.
#[utoipa::path(
    operation_id = "publishBaseProduction",
    tag = BASES,
    responses(
        (status = 200, description = "Base production published successfully")
    )
)]
#[post("/bases/publish-production")]
pub(crate) async fn publish_production(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();

    tx.send(Command::PublishBaseProduction { response: sender })
        .await
        .map_err(|e| {
            log::error!("Error sending base production publish command: {e}");
            UserError::InternalError
        })?;

    let result = receiver.await.map_err(|e| {
        log::error!("Error receiving base production publish result: {e}");
        UserError::InternalError
    })?;

    result?;
    Ok(HttpResponse::Ok().finish())
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
pub(crate) async fn get(
    session: Session,
    path: web::Path<u64>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
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

    let mut base = base
        .as_ref()
        .map(BaseResponse::from)
        .ok_or(UserError::NotFound("Base"))?;
    if !can_read_bloc(&session, &base.bloc)? {
        base.redact_protected_fields();
    }
    Ok(HttpResponse::Ok().json(base))
}

/// Update a base.
#[utoipa::path(
    operation_id = "patchBase",
    tag = BASES,
    responses(
        (status = 200, description = "Base updated successfully"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "No write permission for the bloc")
    )
)]
#[patch("/bases/{id}")]
pub(crate) async fn patch(
    session: Session,
    path: web::Path<u64>,
    body: web::Json<PatchBaseBody>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    authenticated_user(&session)?.ok_or(UserError::Unauthorized)?;
    let id = BaseId(path.into_inner());

    let (base_tx, base_rx) = tokio::sync::oneshot::channel();
    tx.send(Command::GetBase(id, base_tx)).await.map_err(|e| {
        log::error!("Error sending base lookup command: {e}");
        UserError::InternalError
    })?;
    let base = base_rx.await.map_err(|e| {
        log::error!("Error receiving base: {e}");
        UserError::InternalError
    })?;
    let bloc = base
        .as_ref()
        .map(MilitaryBase::bloc_name)
        .ok_or(UserError::NotFound("Base"))?;
    require_bloc_write(&session, bloc)?;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let body = body.into_inner();
    tx.send(Command::PatchBase {
        id,
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

#[cfg(test)]
mod tests {
    use ordered_float::NotNan;

    use super::{BaseResponse, BaseTargetResponse};
    use crate::{
        domain::{BaseId, BlocName, Loot, PlacementId, ZoneName},
        geometry::Point,
    };

    #[test]
    fn redacted_base_omits_target_and_production_count() {
        let mut response = BaseResponse {
            id: BaseId(1),
            position: Point::new(NotNan::new(1.0).unwrap(), NotNan::new(2.0).unwrap()),
            placement_id: serde_json::from_str::<PlacementId>(r#""placement""#).unwrap(),
            bloc: BlocName::from("west".to_string()),
            zone: ZoneName::from("zone-w".to_string()),
            enabled: true,
            prioritized: false,
            payment: vec![],
            production_count: Some(Loot::default()),
            target: Some(BaseTargetResponse::None),
        };

        let visible_response = serde_json::to_value(&response).unwrap();
        assert!(visible_response.get("target").is_some());
        assert!(visible_response.get("productionCount").is_some());

        response.redact_protected_fields();
        let response = serde_json::to_value(response).unwrap();

        assert!(response.get("target").is_none());
        assert!(response.get("productionCount").is_none());
    }
}
