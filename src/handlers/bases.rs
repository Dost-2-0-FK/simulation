use actix_session::Session;
use actix_web::{HttpResponse, Responder, delete, get, patch, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{
        BaseId, BlocName, CharacterKey, CharacterName, DestructionSource, Loot, MilitaryBase, NameMappings,
        PlacementId, Target, TrustId, ZoneName,
    },
    error::{Result, UserError},
    geometry::{Point, Positioned},
    handlers::{authenticated_user, can_read_bloc, require_bloc_write},
    services::{
        coordination_service::{CoordinationAuthorization, CoordinationCapability, OptionalCoordinationAuthorization},
        credit_exchange_service::Share,
    },
    simulation::Command,
};

const BASES: &str = "bases";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Financing {
    #[serde(rename = "financierId")]
    pub(crate) financier: CharacterKey,
    pub(crate) share: Share,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub(crate) struct FinancingRequest {
    #[serde(rename = "financierId")]
    financier: CharacterName,
    share: Share,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct FinancingResponse {
    #[serde(rename = "financierId")]
    financier: CharacterName,
    share: Share,
}

pub(crate) fn resolve_financing(requests: Vec<FinancingRequest>, mappings: &NameMappings) -> Result<Vec<Financing>> {
    requests
        .into_iter()
        .map(|request| {
            let financier = mappings
                .character_key(&request.financier)
                .cloned()
                .ok_or(UserError::BadRequest("Unknown financier"))?;
            Ok(Financing {
                financier,
                share: request.share,
            })
        })
        .collect()
}

pub(crate) fn financing_response(
    financing: &[Financing],
    mappings: &NameMappings,
) -> core::result::Result<Vec<FinancingResponse>, UserError> {
    financing
        .iter()
        .map(|financing| {
            let financier = mappings.character_name(&financing.financier).cloned().ok_or_else(|| {
                log::error!("Unknown configured character key {}", financing.financier);
                UserError::InternalError
            })?;
            Ok(FinancingResponse {
                financier,
                share: financing.share,
            })
        })
        .collect()
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
    payment: Vec<FinancingRequest>,
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
    pub(crate) payment: Vec<FinancingResponse>,
    /// How much loot has accumulated since the last transfer. Omitted without read access to the bloc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) production_count: Option<Loot>,
    /// The base's configured target. Omitted without read access to the bloc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<BaseTargetResponse>,
}

impl BaseResponse {
    pub(crate) fn new(base: &MilitaryBase, mappings: &NameMappings) -> Result<Self> {
        let placement = base.placement();
        let zone = placement.zone();
        Ok(Self {
            id: base.id(),
            placement_id: placement.id().clone(),
            zone: zone.name().clone(),
            bloc: zone.bloc_name().clone(),
            payment: financing_response(base.financiers(), mappings)?,
            enabled: base.enabled(),
            prioritized: base.prioritized(),
            target: Some(base.target().into()),
            position: base.position(),
            production_count: Some(base.production_count().clone()),
        })
    }

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
        (status = 400, description = "Credit exchange rejected the request", body = String, content_type = "text/plain"),
        (status = 401, description = "Not authenticated", body = String, content_type = "text/html"),
        (status = 402, description = "Insufficient credit for booking", body = String, content_type = "text/plain"),
        (status = 403, description = "No write permission for the bloc", body = String, content_type = "text/html"),
        (status = 404, description = "Placement not found", body = String, content_type = "text/html"),
        (status = 409, description = "Placement is already occupied", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to create the base", body = String, content_type = "text/html"),
    ),
)]
#[post("/bases")]
pub(crate) async fn post(
    session: Session,
    body: web::Json<PostBaseBody>,
    tx: web::Data<mpsc::Sender<Command>>,
    mappings: web::Data<NameMappings>,
) -> Result<impl Responder> {
    let body = body.into_inner();
    let financing = resolve_financing(body.payment, &mappings)?;
    authenticated_user(&session)?.ok_or(UserError::Unauthorized)?;

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
        placement_id: body.placement_id,
        financing,
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
            UserError::CreditExchangeQueryFailed => {
                log::error!("credit exchange query failed while creating base")
            }
            UserError::NotFound(err) => log::info!("not found: {err}"),
            UserError::Conflict(err) => log::info!("conflict: {err}"),
            UserError::BadRequest(err) => log::info!("bad request: {err}"),
            UserError::InvalidBaseTarget => log::info!("invalid base target"),
            UserError::Unauthorized => log::info!("unauthorized while creating base"),
            UserError::Forbidden => log::info!("forbidden while creating base"),
            UserError::PaymentRequired(body) => log::info!("base payment required: {body}"),
            UserError::CreditExchange { status, body } => {
                log::info!("credit exchange rejected base creation with {status}: {body}")
            }
            UserError::AuthService { status, body } => {
                log::info!("auth service rejected base creation with {status}: {body}")
            }
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
        (status = 200, description = "All existing bases", body = [BaseResponse]),
        (status = 500, description = "Failed to retrieve bases", body = String, content_type = "text/html")
    )
)]
#[get("/bases")]
pub(crate) async fn list(
    session: Session,
    tx: web::Data<mpsc::Sender<Command>>,
    mappings: web::Data<NameMappings>,
) -> Result<impl Responder> {
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
            let mut response = BaseResponse::new(base, &mappings)?;
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
        (status = 200, description = "Base production published successfully"),
        (status = 401, description = "Missing or invalid coordination service credentials", body = String, content_type = "text/html"),
        (status = 403, description = "Coordination service lacks permission to publish base production", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to publish base production", body = String, content_type = "text/html")
    )
)]
#[post("/bases/publish-production")]
pub(crate) async fn publish_production(
    authorization: CoordinationAuthorization,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    authorization.require(CoordinationCapability::PublishBaseProduction)?;
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
        (status = 200, description = "Existing base", body = BaseResponse),
        (status = 404, description = "Base not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to retrieve the base", body = String, content_type = "text/html")
    )
)]
#[get("/bases/{id}")]
pub(crate) async fn get(
    session: Session,
    path: web::Path<u64>,
    tx: web::Data<mpsc::Sender<Command>>,
    mappings: web::Data<NameMappings>,
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

    let mut base = BaseResponse::new(base.as_ref().ok_or(UserError::NotFound("Base"))?, &mappings)?;
    if !can_read_bloc(&session, &base.bloc)? {
        base.redact_protected_fields();
    }
    Ok(HttpResponse::Ok().json(base))
}

/// Delete a base and rebase its units to the closest remaining base in the same bloc.
#[utoipa::path(
    operation_id = "deleteBase",
    tag = BASES,
    responses(
        (status = 204, description = "Base deleted successfully"),
        (status = 401, description = "Missing or invalid user or coordination service credentials", body = String, content_type = "text/html"),
        (status = 403, description = "Missing required bloc-write or coordination-service permission", body = String, content_type = "text/html"),
        (status = 404, description = "Base not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to delete the base or its credit subscriptions", body = String, content_type = "text/html")
    )
)]
#[delete("/bases/{id}")]
pub(crate) async fn delete(
    session: Session,
    coordination_authorization: OptionalCoordinationAuthorization,
    path: web::Path<u64>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let id = BaseId(path.into_inner());
    let source = if coordination_authorization.is_present() {
        coordination_authorization.require(CoordinationCapability::DeleteBase)?;
        DestructionSource::CoordinationService
    } else {
        authenticated_user(&session)?.ok_or(UserError::Unauthorized)?;
        let (base_tx, base_rx) = tokio::sync::oneshot::channel();
        tx.send(Command::GetBase(id, base_tx)).await.map_err(|error| {
            log::error!("Error sending base lookup command: {error}");
            UserError::InternalError
        })?;
        let base = base_rx.await.map_err(|error| {
            log::error!("Error receiving base: {error}");
            UserError::InternalError
        })?;
        let bloc = base
            .as_ref()
            .map(MilitaryBase::bloc_name)
            .ok_or(UserError::NotFound("Base"))?;
        require_bloc_write(&session, bloc)?;
        DestructionSource::AuthorizedUser
    };

    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::DeleteBase {
        id,
        source,
        response: sender,
    })
    .await
    .map_err(|error| {
        log::error!("Error sending base deletion command: {error}");
        UserError::InternalError
    })?;

    receiver.await.map_err(|error| {
        log::error!("Error receiving base deletion result: {error}");
        UserError::InternalError
    })??;
    Ok(HttpResponse::NoContent().finish())
}

/// Update a base.
#[utoipa::path(
    operation_id = "patchBase",
    tag = BASES,
    responses(
        (status = 200, description = "Base updated successfully"),
        (status = 400, description = "Target belongs to the same bloc as the base", body = String, content_type = "text/html"),
        (status = 401, description = "Not authenticated", body = String, content_type = "text/html"),
        (status = 403, description = "No write permission for the bloc", body = String, content_type = "text/html"),
        (status = 404, description = "Base or target not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to update the base", body = String, content_type = "text/html")
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
    use std::{collections::HashMap, sync::Arc};

    use actix_session::{Session, SessionMiddleware, storage::CookieSessionStore};
    use actix_web::{App, HttpResponse, cookie::Key, http::StatusCode, test as actix_test, web};
    use ordered_float::NotNan;
    use tokio::sync::{RwLock, mpsc};

    use super::{
        BaseResponse, BaseTargetResponse, Financing, FinancingRequest, delete, financing_response, resolve_financing,
    };
    use crate::{
        domain::{
            BaseId, Bloc, BlocKey, BlocName, Chance, CharacterKey, CharacterName, DestructionSource, Loot,
            MilitaryBase, NameMappings, Placement, PlacementId, Zone, ZoneKey, ZoneName,
        },
        geometry::Point,
        services::{
            auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthenticatedUser},
            credit_exchange_service::Share,
        },
        simulation::Command,
    };

    async fn seed_authenticated_session(session: Session, user: web::Data<AuthenticatedUser>) -> HttpResponse {
        session.insert(AUTHENTICATED_USER_SESSION_KEY, user.get_ref()).unwrap();
        HttpResponse::Ok().finish()
    }

    fn base(id: BaseId, bloc_name: BlocName) -> MilitaryBase {
        let bloc_key = BlocKey::from("bloc-key".to_string());
        let bloc = Arc::new(RwLock::new(Bloc::new(
            bloc_key.clone(),
            bloc_name.clone(),
            Chance::new(1),
            Share::default(),
        )));
        let zone = Arc::new(Zone::new_with_social_rules(
            ZoneKey::from("zone-key".to_string()),
            ZoneName::from("zone".to_string()),
            bloc_key,
            bloc_name,
            bloc,
            vec![],
        ));
        let placement = Arc::new(Placement::new(
            serde_json::from_value(serde_json::json!("placement")).unwrap(),
            zone,
            Point::new(NotNan::new(0.0).unwrap(), NotNan::new(0.0).unwrap()),
        ));
        MilitaryBase::from_persisted(id, placement, vec![], true, false, Loot::default(), Loot::default())
    }

    #[actix_web::test]
    async fn user_with_bloc_write_permission_can_delete_base() {
        let id = BaseId(7);
        let bloc_name = BlocName::from("west".to_string());
        let base = base(id, bloc_name.clone());
        let user = serde_json::from_value::<AuthenticatedUser>(serde_json::json!({
            "userId": "alice",
            "blocPermissions": { "west": "write" },
            "zonePermissions": {}
        }))
        .unwrap();
        let (tx, mut rx) = mpsc::channel(2);
        actix_web::rt::spawn(async move {
            match rx.recv().await.unwrap() {
                Command::GetBase(actual_id, response) => {
                    assert_eq!(actual_id, id);
                    response.send(Some(base)).unwrap();
                }
                command => panic!("unexpected command: {command:?}"),
            }
            match rx.recv().await.unwrap() {
                Command::DeleteBase {
                    id: actual_id,
                    source: DestructionSource::AuthorizedUser,
                    response,
                } => {
                    assert_eq!(actual_id, id);
                    response.send(Ok(())).unwrap();
                }
                command => panic!("unexpected command: {command:?}"),
            }
        });

        let app = actix_test::init_service(
            App::new()
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .app_data(web::Data::new(user))
                .app_data(web::Data::new(tx))
                .route("/test/session", web::post().to(seed_authenticated_session))
                .service(delete),
        )
        .await;
        let session_response =
            actix_test::call_service(&app, actix_test::TestRequest::post().uri("/test/session").to_request()).await;
        let session_cookie = session_response.response().cookies().next().unwrap();
        let request = actix_test::TestRequest::delete()
            .uri("/bases/7")
            .cookie(session_cookie)
            .to_request();

        let response = actix_test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

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

    #[test]
    fn frontend_financing_uses_character_names_while_internal_financing_uses_keys() {
        let character_key = CharacterKey::from("character-key".to_string());
        let character_name = CharacterName::from("Character Name".to_string());
        let mappings = NameMappings::new(
            HashMap::new(),
            HashMap::new(),
            HashMap::from([(character_key.clone(), character_name.clone())]),
        );
        let request = serde_json::from_value::<FinancingRequest>(serde_json::json!({
            "financierId": "Character Name",
            "share": 0.25
        }))
        .unwrap();
        let internal = resolve_financing(vec![request], &mappings).unwrap();
        assert_eq!(internal[0].financier, character_key);

        let response = financing_response(
            &[Financing {
                financier: internal[0].financier.clone(),
                share: internal[0].share,
            }],
            &mappings,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!([{
                "financierId": "Character Name",
                "share": 0.25
            }])
        );
    }
}
