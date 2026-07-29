use std::collections::HashMap;

use actix_identity::Identity;
use actix_session::Session;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, Responder, get, post, web};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{BlocName, CharacterKey, CharacterName, NameMappings, ZoneName},
    error::{Result, UserError},
    services::{
        auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthService, AuthenticatedUser, LoginCredentials},
        credit_exchange_service::{
            AccountSubscriptions, CreditAccount, CreditExchangeResponseError, CreditExchangeService, CreditUserId,
            MoneyBalance,
        },
    },
};

const AUTH: &str = "auth";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct LoginRequest {
    password: CharacterKey,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginResponse {
    user_id: CharacterName,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentUserResponse {
    #[serde(flatten)]
    user: AuthenticatedUser,
    #[serde(skip_serializing_if = "Option::is_none")]
    credit: Option<MoneyBalance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subscriptions: Option<AccountSubscriptions>,
    blocs: HashMap<BlocName, CreditAccount>,
    zones: HashMap<ZoneName, CreditAccount>,
}

/// Log in and persist the authenticated user in the identity session.
#[utoipa::path(
    operation_id = "login",
    tag = AUTH,
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login completed successfully", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to persist the authenticated session", body = String, content_type = "text/html")
    )
)]
#[post("/login")]
pub(crate) async fn login(
    request: HttpRequest,
    session: Session,
    auth_service: web::Data<AuthService>,
    login_request: web::Json<LoginRequest>,
) -> Result<impl Responder> {
    let authenticated_user = auth_service
        .authenticate(LoginCredentials::new(login_request.password.clone()))
        .await
        .map_err(|error| {
            log::error!("Auth service failed to authenticate user: {error:#}");
            UserError::InternalError
        })?
        .ok_or(UserError::Unauthorized)?;

    session
        .insert(AUTHENTICATED_USER_SESSION_KEY, &authenticated_user)
        .map_err(|error| {
            log::error!("Error storing authenticated user permissions in session: {error}");
            UserError::InternalError
        })?;

    Identity::login(
        &request.extensions(),
        authenticated_user.character_name().clone().into(),
    )
    .map_err(|error| {
        log::error!("Error storing identity: {error}");
        UserError::InternalError
    })?;

    Ok(HttpResponse::Ok().json(LoginResponse {
        user_id: authenticated_user.character_name().clone(),
    }))
}

/// Return the currently authenticated user including permissions.
#[utoipa::path(
    operation_id = "getCurrentUser",
    tag = AUTH,
    responses(
        (status = 200, description = "Current authenticated user", body = CurrentUserResponse),
        (status = 401, description = "Not authenticated", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to read the authenticated user or query the credit service", body = String, content_type = "text/html")
    )
)]
#[get("/me")]
pub(crate) async fn get_current_user(
    session: Session,
    credit_exchange_service: web::Data<CreditExchangeService>,
    name_mappings: web::Data<NameMappings>,
) -> Result<impl Responder> {
    let authenticated_user = session
        .get::<AuthenticatedUser>(AUTHENTICATED_USER_SESSION_KEY)
        .map_err(|error| {
            log::error!("Error reading authenticated user from session: {error}");
            UserError::InternalError
        })?
        .ok_or(UserError::Unauthorized)?;

    let character_key = name_mappings
        .character_key(authenticated_user.character_name())
        .ok_or_else(|| {
            log::error!(
                "Authenticated character {} has no configured credit-service key",
                authenticated_user.character_name()
            );
            UserError::InternalError
        })?;
    let character_credit_user_id = CreditUserId::from(character_key);
    let character_account = async {
        let credit = credit_exchange_service.money_balance(&character_credit_user_id).await?;
        let subscriptions = credit_exchange_service
            .subscriptions(&character_credit_user_id, &name_mappings)
            .await?;
        Ok::<_, anyhow::Error>((credit, subscriptions))
    }
    .await;
    let (credit, subscriptions) = match character_account {
        Ok((credit, subscriptions)) => (Some(credit), Some(subscriptions)),
        Err(error)
            if error
                .downcast_ref::<CreditExchangeResponseError>()
                .is_some_and(|response| response.status() == reqwest::StatusCode::NOT_FOUND) =>
        {
            log::warn!(
                "Authenticated character {} was not found in the credit service",
                authenticated_user.character_name()
            );
            (None, None)
        }
        Err(error) => return Err(credit_query_error(error, "querying authenticated user account")),
    };

    let mut blocs = HashMap::new();
    for bloc_name in authenticated_user.writable_blocs() {
        let bloc_key = name_mappings.bloc_key(bloc_name).ok_or_else(|| {
            log::error!("Write-permitted bloc {bloc_name} has no configured credit-service key");
            UserError::InternalError
        })?;
        match credit_exchange_service
            .account(&CreditUserId::from(bloc_key), &name_mappings)
            .await
        {
            Ok(account) => {
                blocs.insert(bloc_name.clone(), account);
            }
            Err(error)
                if error
                    .downcast_ref::<CreditExchangeResponseError>()
                    .is_some_and(|response| response.status() == reqwest::StatusCode::NOT_FOUND) =>
            {
                log::warn!("Write-permitted bloc {bloc_name} was not found in the credit service");
            }
            Err(error) => {
                return Err(credit_query_error(error, &format!("querying bloc account {bloc_name}")));
            }
        }
    }

    let mut zones = HashMap::new();
    for zone_name in authenticated_user.writable_zones() {
        let zone_key = name_mappings.zone_key(zone_name).ok_or_else(|| {
            log::error!("Write-permitted zone {zone_name} has no configured credit-service key");
            UserError::InternalError
        })?;
        match credit_exchange_service
            .account(&CreditUserId::from(zone_key), &name_mappings)
            .await
        {
            Ok(account) => {
                zones.insert(zone_name.clone(), account);
            }
            Err(error)
                if error
                    .downcast_ref::<CreditExchangeResponseError>()
                    .is_some_and(|response| response.status() == reqwest::StatusCode::NOT_FOUND) =>
            {
                log::warn!("Write-permitted zone {zone_name} was not found in the credit service");
            }
            Err(error) => {
                return Err(credit_query_error(error, &format!("querying zone account {zone_name}")));
            }
        }
    }

    Ok(HttpResponse::Ok().json(CurrentUserResponse {
        user: authenticated_user,
        credit,
        subscriptions,
        blocs,
        zones,
    }))
}

fn credit_query_error(error: anyhow::Error, action: &str) -> UserError {
    if let Some(response) = error.downcast_ref::<CreditExchangeResponseError>() {
        return UserError::CreditExchange {
            status: response.status().as_u16(),
            body: response.body().to_string(),
        };
    }

    log::error!("Credit service error while {action}: {error:#}");
    UserError::InternalError
}

/// Log out by clearing the identity session.
#[utoipa::path(
    operation_id = "logout",
    tag = AUTH,
    responses(
        (status = 200, description = "Logout completed successfully")
    )
)]
#[post("/logout")]
pub(crate) async fn logout(identity: Option<Identity>, session: Session) -> Result<impl Responder> {
    session.remove(AUTHENTICATED_USER_SESSION_KEY);

    if let Some(identity) = identity {
        identity.logout();
    }

    Ok(HttpResponse::Ok().finish())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use actix_identity::IdentityMiddleware;
    use actix_session::{Session, SessionMiddleware, storage::CookieSessionStore};
    use actix_web::{
        App, HttpResponse, HttpServer,
        cookie::Key,
        http::{StatusCode, header},
        post, test, web,
    };

    use super::get_current_user;
    use crate::{
        domain::{BlocKey, BlocName, CharacterKey, CharacterName, LootFactors, NameMappings, ZoneKey, ZoneName},
        services::{
            auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthenticatedUser},
            credit_exchange_service::{CreditExchangeService, VecResourceName},
        },
    };

    #[post("/test/session")]
    async fn seed_authenticated_session(session: Session, user: web::Data<AuthenticatedUser>) -> HttpResponse {
        session.insert(AUTHENTICATED_USER_SESSION_KEY, user.get_ref()).unwrap();
        HttpResponse::Ok().finish()
    }

    fn test_credit_service(url: url::Url) -> CreditExchangeService {
        CreditExchangeService::new(
            url,
            "bank".to_string(),
            serde_json::from_value(serde_json::json!({ "money": 0.0, "resources": {} })).unwrap(),
            serde_json::from_value(serde_json::json!({ "money": 0.0, "resources": {} })).unwrap(),
            serde_json::from_value(serde_json::json!({ "money": 0.0, "resources": {} })).unwrap(),
            serde_json::from_value::<VecResourceName>(serde_json::json!([])).unwrap(),
            LootFactors::default(),
        )
    }

    fn test_name_mappings() -> NameMappings {
        NameMappings::new(
            HashMap::from([(
                BlocKey::from("west-key".to_string()),
                BlocName::from("west".to_string()),
            )]),
            HashMap::from([(
                ZoneKey::from("zone-key".to_string()),
                ZoneName::from("zone_w".to_string()),
            )]),
            HashMap::from([(
                CharacterKey::from("alice-key".to_string()),
                CharacterName::from("alice".to_string()),
            )]),
        )
    }

    #[actix_web::test]
    async fn current_user_returns_unauthorized_without_a_session() {
        let app = test::init_service(
            App::new()
                .wrap(IdentityMiddleware::default())
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .app_data(web::Data::new(test_credit_service(
                    "http://127.0.0.1/".parse().unwrap(),
                )))
                .app_data(web::Data::new(test_name_mappings()))
                .service(get_current_user),
        )
        .await;

        let request = test::TestRequest::get().uri("/me").to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn current_user_returns_the_authenticated_user() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = HttpServer::new(|| {
            App::new()
                .route(
                    "/api/users/{user_id}/credits",
                    web::get().to(|path: web::Path<String>| async move {
                        let user_id = path.into_inner();
                        let credits = match user_id.as_str() {
                            "alice-key" => serde_json::json!([
                                { "creditType": "money", "balance": 8.0, "hourly": 0.0 }
                            ]),
                            "west-key" => serde_json::json!([
                                { "creditType": "money", "balance": 12000.0, "hourly": 1.0 },
                                { "creditType": "iron", "balance": 2500.0, "hourly": 2.0 }
                            ]),
                            "zone-key" => serde_json::json!([
                                { "creditType": "money", "balance": 1200.0, "hourly": 3.0 },
                                { "creditType": "water", "balance": 200.0, "hourly": 4.0 }
                            ]),
                            _ => return HttpResponse::NotFound().finish(),
                        };
                        HttpResponse::Ok().json(serde_json::json!({ "credits": credits }))
                    }),
                )
                .route(
                    "/api/users/{user_id}/subscriptions",
                    web::get().to(|path: web::Path<String>| async move {
                        let user_id = path.into_inner();
                        HttpResponse::Ok().json(serde_json::json!({
                            "subscriptions": [
                                {
                                    "id": format!("{user_id}-outgoing"),
                                    "sender": user_id.as_str(),
                                    "receiver": "receiver",
                                    "value": 5.0,
                                    "subscriptionType": "contract",
                                    "priority": 1,
                                    "creditType": "money"
                                },
                                {
                                    "id": format!("{user_id}-incoming"),
                                    "sender": "sender",
                                    "receiver": user_id.as_str(),
                                    "value": 6.0,
                                    "subscriptionType": "sr",
                                    "priority": 2,
                                    "creditType": "iron"
                                },
                                {
                                    "id": format!("{user_id}-self"),
                                    "sender": user_id.as_str(),
                                    "receiver": user_id.as_str(),
                                    "value": 7.0,
                                    "subscriptionType": "contract",
                                    "priority": 3,
                                    "creditType": "money"
                                }
                            ]
                        }))
                    }),
                )
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);

        let authenticated_user = serde_json::from_value::<AuthenticatedUser>(serde_json::json!({
            "userId": "alice",
            "blocPermissions": { "west": "write", "east": "read" },
            "zonePermissions": { "zone_w": "write", "zone_r": "read" }
        }))
        .unwrap();
        let app = test::init_service(
            App::new()
                .wrap(IdentityMiddleware::default())
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .app_data(web::Data::new(authenticated_user))
                .app_data(web::Data::new(test_credit_service(
                    format!("http://{address}/").parse().unwrap(),
                )))
                .app_data(web::Data::new(test_name_mappings()))
                .service(seed_authenticated_session)
                .service(get_current_user),
        )
        .await;

        let session_request = test::TestRequest::post().uri("/test/session").to_request();
        let session_response = test::call_service(&app, session_request).await;
        assert_eq!(session_response.status(), StatusCode::OK);

        let session_cookie = session_response
            .response()
            .cookies()
            .next()
            .expect("seeding the authenticated session should set a cookie");
        let session_cookie = format!("{}={}", session_cookie.name(), session_cookie.value());
        let request = test::TestRequest::get()
            .uri("/me")
            .insert_header((header::COOKIE, session_cookie))
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);

        let response: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(
            response,
            serde_json::json!({
                "userId": "alice",
                "blocPermissions": { "west": "write", "east": "read" },
                "zonePermissions": { "zone_w": "write", "zone_r": "read" },
                "credit": {
                    "balance": 8.0,
                    "hourly": 0.0
                },
                "subscriptions": {
                    "outgoing": [
                        {
                            "id": "alice-key-outgoing",
                            "sender": { "type": "character", "name": "alice" },
                            "receiver": { "type": "other", "id": "receiver" },
                            "value": 5.0,
                            "subscriptionType": "contract",
                            "priority": 1,
                            "creditType": "money"
                        },
                        {
                            "id": "alice-key-self",
                            "sender": { "type": "character", "name": "alice" },
                            "receiver": { "type": "character", "name": "alice" },
                            "value": 7.0,
                            "subscriptionType": "contract",
                            "priority": 3,
                            "creditType": "money"
                        }
                    ],
                    "incoming": [
                        {
                            "id": "alice-key-incoming",
                            "sender": { "type": "other", "id": "sender" },
                            "receiver": { "type": "character", "name": "alice" },
                            "value": 6.0,
                            "subscriptionType": "sr",
                            "priority": 2,
                            "creditType": "iron"
                        },
                        {
                            "id": "alice-key-self",
                            "sender": { "type": "character", "name": "alice" },
                            "receiver": { "type": "character", "name": "alice" },
                            "value": 7.0,
                            "subscriptionType": "contract",
                            "priority": 3,
                            "creditType": "money"
                        }
                    ]
                },
                "blocs": {
                    "west": {
                        "money": {
                            "balance": 12000.0,
                            "hourly": 1.0
                        },
                        "resources": {
                            "iron": {
                                "balance": 2500.0,
                                "hourly": 2.0
                            }
                        },
                        "subscriptions": {
                            "outgoing": [
                                {
                                    "id": "west-key-outgoing",
                                    "sender": { "type": "bloc", "name": "west" },
                                    "receiver": { "type": "other", "id": "receiver" },
                                    "value": 5.0,
                                    "subscriptionType": "contract",
                                    "priority": 1,
                                    "creditType": "money"
                                },
                                {
                                    "id": "west-key-self",
                                    "sender": { "type": "bloc", "name": "west" },
                                    "receiver": { "type": "bloc", "name": "west" },
                                    "value": 7.0,
                                    "subscriptionType": "contract",
                                    "priority": 3,
                                    "creditType": "money"
                                }
                            ],
                            "incoming": [
                                {
                                    "id": "west-key-incoming",
                                    "sender": { "type": "other", "id": "sender" },
                                    "receiver": { "type": "bloc", "name": "west" },
                                    "value": 6.0,
                                    "subscriptionType": "sr",
                                    "priority": 2,
                                    "creditType": "iron"
                                },
                                {
                                    "id": "west-key-self",
                                    "sender": { "type": "bloc", "name": "west" },
                                    "receiver": { "type": "bloc", "name": "west" },
                                    "value": 7.0,
                                    "subscriptionType": "contract",
                                    "priority": 3,
                                    "creditType": "money"
                                }
                            ]
                        }
                    }
                },
                "zones": {
                    "zone_w": {
                        "money": {
                            "balance": 1200.0,
                            "hourly": 3.0
                        },
                        "resources": {
                            "water": {
                                "balance": 200.0,
                                "hourly": 4.0
                            }
                        },
                        "subscriptions": {
                            "outgoing": [
                                {
                                    "id": "zone-key-outgoing",
                                    "sender": { "type": "zone", "name": "zone_w" },
                                    "receiver": { "type": "other", "id": "receiver" },
                                    "value": 5.0,
                                    "subscriptionType": "contract",
                                    "priority": 1,
                                    "creditType": "money"
                                },
                                {
                                    "id": "zone-key-self",
                                    "sender": { "type": "zone", "name": "zone_w" },
                                    "receiver": { "type": "zone", "name": "zone_w" },
                                    "value": 7.0,
                                    "subscriptionType": "contract",
                                    "priority": 3,
                                    "creditType": "money"
                                }
                            ],
                            "incoming": [
                                {
                                    "id": "zone-key-incoming",
                                    "sender": { "type": "other", "id": "sender" },
                                    "receiver": { "type": "zone", "name": "zone_w" },
                                    "value": 6.0,
                                    "subscriptionType": "sr",
                                    "priority": 2,
                                    "creditType": "iron"
                                },
                                {
                                    "id": "zone-key-self",
                                    "sender": { "type": "zone", "name": "zone_w" },
                                    "receiver": { "type": "zone", "name": "zone_w" },
                                    "value": 7.0,
                                    "subscriptionType": "contract",
                                    "priority": 3,
                                    "creditType": "money"
                                }
                            ]
                        }
                    }
                }
            })
        );
        handle.stop(true).await;
    }

    #[actix_web::test]
    async fn current_user_omits_credit_accounts_missing_from_the_credit_service() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = HttpServer::new(|| {
            App::new()
                .route(
                    "/api/users/{user_id}/credits",
                    web::get().to(|| async { HttpResponse::NotFound().body("Credit user not found") }),
                )
                .route(
                    "/api/users/{user_id}/subscriptions",
                    web::get().to(|| async { HttpResponse::NotFound().body("Credit user not found") }),
                )
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);

        let authenticated_user = serde_json::from_value::<AuthenticatedUser>(serde_json::json!({
            "userId": "alice",
            "blocPermissions": { "west": "write" },
            "zonePermissions": { "zone_w": "write" }
        }))
        .unwrap();
        let app = test::init_service(
            App::new()
                .wrap(IdentityMiddleware::default())
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .app_data(web::Data::new(authenticated_user))
                .app_data(web::Data::new(test_credit_service(
                    format!("http://{address}/").parse().unwrap(),
                )))
                .app_data(web::Data::new(test_name_mappings()))
                .service(seed_authenticated_session)
                .service(get_current_user),
        )
        .await;

        let session_response =
            test::call_service(&app, test::TestRequest::post().uri("/test/session").to_request()).await;
        let session_cookie = session_response
            .response()
            .cookies()
            .next()
            .expect("seeding the authenticated session should set a cookie");
        let session_cookie = format!("{}={}", session_cookie.name(), session_cookie.value());
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/me")
                .insert_header((header::COOKIE, session_cookie))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            test::read_body_json::<serde_json::Value, _>(response).await,
            serde_json::json!({
                "userId": "alice",
                "blocPermissions": { "west": "write" },
                "zonePermissions": { "zone_w": "write" },
                "blocs": {},
                "zones": {}
            })
        );
        handle.stop(true).await;
    }

    #[actix_web::test]
    async fn current_user_forwards_credit_service_error_responses() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = HttpServer::new(|| {
            App::new().route(
                "/api/users/alice-key/credits",
                web::get().to(|| async { HttpResponse::UnprocessableEntity().body("Invalid credit user") }),
            )
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);

        let authenticated_user = serde_json::from_value::<AuthenticatedUser>(serde_json::json!({
            "userId": "alice",
            "blocPermissions": {},
            "zonePermissions": {}
        }))
        .unwrap();
        let app = test::init_service(
            App::new()
                .wrap(IdentityMiddleware::default())
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .app_data(web::Data::new(authenticated_user))
                .app_data(web::Data::new(test_credit_service(
                    format!("http://{address}/").parse().unwrap(),
                )))
                .app_data(web::Data::new(test_name_mappings()))
                .service(seed_authenticated_session)
                .service(get_current_user),
        )
        .await;

        let session_response =
            test::call_service(&app, test::TestRequest::post().uri("/test/session").to_request()).await;
        let session_cookie = session_response
            .response()
            .cookies()
            .next()
            .expect("seeding the authenticated session should set a cookie");
        let session_cookie = format!("{}={}", session_cookie.name(), session_cookie.value());
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/me")
                .insert_header((header::COOKIE, session_cookie))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(test::read_body(response).await, "Invalid credit user");
        handle.stop(true).await;
    }
}
