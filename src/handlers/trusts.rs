use actix_session::Session;
use actix_web::{HttpResponse, Responder, delete, get, post, web};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    domain::{DestructionSource, NameMappings, PlacementId, Trust, TrustId, ZoneName},
    error::{Result, UserError},
    geometry::{Distance, Point, Positioned},
    handlers::{
        authenticated_user,
        bases::{FinancingRequest, FinancingResponse, financing_response, resolve_financing},
        can_read_zone, require_zone_write,
    },
    services::{
        coordination_service::{CoordinationAuthorization, CoordinationCapability, OptionalCoordinationAuthorization},
        credit_exchange_service::{Money, ResourceName, Resources},
    },
    simulation::Command,
};

const TRUSTS: &str = "trusts";

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct PostTrustBody {
    placement_id: PlacementId,
    resource: ResourceName,
    payment: Vec<FinancingRequest>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustResponse {
    id: TrustId,
    placement_id: PlacementId,
    zone: ZoneName,
    payment: Vec<FinancingResponse>,
    position: Point,
    /// The resource produced by this trust. Visible regardless of zone permissions.
    resource: ResourceName,
    /// The inhibition radius applied after capping the configured radius by half the distance to the nearest
    /// placement.
    inhibition_radius: Distance,
    /// The current monetary income after applying the resource-supply discount. Omitted without read access to the
    /// zone.
    #[serde(skip_serializing_if = "Option::is_none")]
    income: Option<Money>,
    /// The configured resource production. Omitted without read access to the zone.
    #[serde(skip_serializing_if = "Option::is_none")]
    producing: Option<Resources>,
}

impl TrustResponse {
    pub(crate) fn new(
        trust: &Trust,
        income: Money,
        producing: Resources,
        inhibition_radius: Distance,
        mappings: &NameMappings,
    ) -> Result<Self> {
        let placement = trust.placement();
        Ok(Self {
            id: trust.id(),
            placement_id: placement.id().clone(),
            zone: placement.zone().name().clone(),
            payment: financing_response(trust.financing(), mappings)?,
            position: trust.position(),
            resource: trust.resource_name().clone(),
            inhibition_radius,
            income: Some(income),
            producing: Some(producing),
        })
    }

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
        (status = 400, description = "Credit exchange rejected the request", body = String, content_type = "text/plain"),
        (status = 401, description = "Not authenticated", body = String, content_type = "text/html"),
        (status = 402, description = "Insufficient credit for booking", body = String, content_type = "text/plain"),
        (status = 403, description = "No write permission for the zone", body = String, content_type = "text/html"),
        (status = 404, description = "Placement or resource not found", body = String, content_type = "text/html"),
        (status = 409, description = "Placement is already occupied", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to create the trust", body = String, content_type = "text/html"),
    ),
)]
#[post("/trusts")]
pub(crate) async fn post(
    session: Session,
    body: web::Json<PostTrustBody>,
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
    let zone = placements
        .iter()
        .find(|placement| placement.id() == &body.placement_id)
        .map(|placement| placement.zone().name())
        .ok_or(UserError::NotFound("Placement"))?;
    require_zone_write(&session, zone)?;

    let (sender, receiver) = tokio::sync::oneshot::channel();

    tx.send(Command::CreateTrust {
        placement_id: body.placement_id,
        financing,
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
        (status = 200, description = "Trust production published successfully"),
        (status = 401, description = "Missing or invalid coordination service credentials", body = String, content_type = "text/html"),
        (status = 403, description = "Coordination service lacks permission to publish trust production", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to publish trust production", body = String, content_type = "text/html")
    )
)]
#[post("/trusts/publish-production")]
pub(crate) async fn publish_production(
    authorization: CoordinationAuthorization,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    authorization.require(CoordinationCapability::PublishTrustProduction)?;
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
        (status = 200, description = "All existing trusts", body = [TrustResponse]),
        (status = 500, description = "Failed to retrieve trusts", body = String, content_type = "text/html")
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
    })??;

    let trusts = trusts
        .into_iter()
        .map(|mut response| {
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
        (status = 200, description = "Existing trust", body = TrustResponse),
        (status = 404, description = "Trust not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to retrieve the trust", body = String, content_type = "text/html")
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
    })??;

    let mut trust = trust.ok_or(UserError::NotFound("Trust"))?;
    if !can_read_zone(&session, &trust.zone)? {
        trust.redact_protected_fields();
    }
    Ok(HttpResponse::Ok().json(trust))
}

/// Delete a trust and its dependent simulation state.
#[utoipa::path(
    operation_id = "deleteTrust",
    tag = TRUSTS,
    responses(
        (status = 204, description = "Trust deleted successfully"),
        (status = 401, description = "Missing or invalid user or coordination service credentials", body = String, content_type = "text/html"),
        (status = 403, description = "Missing required zone-write or coordination-service permission", body = String, content_type = "text/html"),
        (status = 404, description = "Trust not found", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to delete the trust or its credit subscriptions", body = String, content_type = "text/html")
    )
)]
#[delete("/trusts/{id}")]
pub(crate) async fn delete(
    session: Session,
    coordination_authorization: OptionalCoordinationAuthorization,
    path: web::Path<u64>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let id = TrustId(path.into_inner());
    let source = if coordination_authorization.is_present() {
        coordination_authorization.require(CoordinationCapability::DeleteTrust)?;
        DestructionSource::CoordinationService
    } else {
        authenticated_user(&session)?.ok_or(UserError::Unauthorized)?;
        let (trust_tx, trust_rx) = tokio::sync::oneshot::channel();
        tx.send(Command::GetTrust(id, trust_tx)).await.map_err(|error| {
            log::error!("Error sending trust lookup command: {error}");
            UserError::InternalError
        })?;
        let trust = trust_rx.await.map_err(|error| {
            log::error!("Error receiving trust: {error}");
            UserError::InternalError
        })??;
        let zone = trust
            .as_ref()
            .map(|trust| &trust.zone)
            .ok_or(UserError::NotFound("Trust"))?;
        require_zone_write(&session, zone)?;
        DestructionSource::AuthorizedUser
    };

    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::DeleteTrust {
        id,
        source,
        response: sender,
    })
    .await
    .map_err(|error| {
        log::error!("Error sending trust deletion command: {error}");
        UserError::InternalError
    })?;

    receiver.await.map_err(|error| {
        log::error!("Error receiving trust deletion result: {error}");
        UserError::InternalError
    })??;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_session::{Session, SessionMiddleware, storage::CookieSessionStore};
    use actix_web::{App, HttpResponse, cookie::Key, http::StatusCode, test as actix_test, web};
    use ordered_float::NotNan;
    use tokio::sync::mpsc;

    use super::{TrustResponse, delete};
    use crate::{
        domain::{DestructionSource, PlacementId, TrustId, ZoneName},
        geometry::{Distance, Point},
        services::{
            auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthenticatedUser},
            credit_exchange_service::{Money, ResourceName, Resources},
        },
        simulation::Command,
    };

    async fn seed_authenticated_session(session: Session, user: web::Data<AuthenticatedUser>) -> HttpResponse {
        session.insert(AUTHENTICATED_USER_SESSION_KEY, user.get_ref()).unwrap();
        HttpResponse::Ok().finish()
    }

    fn trust_response(id: TrustId, zone: ZoneName) -> TrustResponse {
        TrustResponse {
            id,
            placement_id: serde_json::from_value(serde_json::json!("placement")).unwrap(),
            zone,
            payment: vec![],
            position: Point::new(NotNan::new(0.0).unwrap(), NotNan::new(0.0).unwrap()),
            resource: ResourceName::new("oil".to_string()),
            inhibition_radius: serde_json::from_value(serde_json::json!(1.0)).unwrap(),
            income: Some(Money::from(1.0)),
            producing: Some(Resources::new_single(ResourceName::new("oil".to_string()), 1.0)),
        }
    }

    #[actix_web::test]
    async fn user_with_zone_write_permission_can_delete_trust() {
        let id = TrustId(7);
        let trust = trust_response(id, ZoneName::from("zone-w".to_string()));
        let user = serde_json::from_value::<AuthenticatedUser>(serde_json::json!({
            "userId": "alice",
            "blocPermissions": {},
            "zonePermissions": { "zone-w": "write" }
        }))
        .unwrap();
        let (tx, mut rx) = mpsc::channel(2);
        actix_web::rt::spawn(async move {
            match rx.recv().await.unwrap() {
                Command::GetTrust(actual_id, response) => {
                    assert_eq!(actual_id, id);
                    response.send(Ok(Some(trust))).unwrap();
                }
                command => panic!("unexpected command: {command:?}"),
            }
            match rx.recv().await.unwrap() {
                Command::DeleteTrust {
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
            .uri("/trusts/7")
            .cookie(session_cookie)
            .to_request();

        let response = actix_test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn redacted_trust_omits_income_and_producing() {
        let mut response = TrustResponse {
            id: TrustId(1),
            placement_id: serde_json::from_str::<PlacementId>(r#""placement""#).unwrap(),
            zone: ZoneName::from("zone-w".to_string()),
            payment: vec![],
            position: Point::new(NotNan::new(1.0).unwrap(), NotNan::new(2.0).unwrap()),
            resource: ResourceName::new("oil".to_string()),
            inhibition_radius: serde_json::from_value::<Distance>(serde_json::json!(1.0)).unwrap(),
            income: Some(Money::from(10.0)),
            producing: Some(Resources::new_single(ResourceName::new("oil".to_string()), 2.0)),
        };

        let visible_response = serde_json::to_value(&response).unwrap();
        assert!(visible_response.get("income").is_some());
        assert!(visible_response.get("producing").is_some());
        assert_eq!(visible_response["inhibitionRadius"], 1.0);

        response.redact_protected_fields();
        let response = serde_json::to_value(response).unwrap();

        assert!(response.get("income").is_none());
        assert!(response.get("producing").is_none());
        assert_eq!(response["resource"], "oil");
    }
}
