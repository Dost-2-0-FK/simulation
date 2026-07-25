use actix_identity::Identity;
use actix_session::Session;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, Responder, get, post, web};
use serde::{Deserialize, Serialize};

use crate::{
    config::{CharacterDirectory, PoliticsDirectory},
    error::{Result, UserError},
    handlers::bases::UserId,
    services::auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthService, AuthenticatedUser, LoginCredentials},
};

const AUTH: &str = "auth";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct LoginRequest {
    password: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginResponse {
    user_id: UserId,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct CurrentUserResponse {
    user_id: UserId,
    bloc_permissions: HashMap<String, crate::services::auth_service::AccessLevel>,
    zone_permissions: HashMap<String, crate::services::auth_service::AccessLevel>,
}

impl CurrentUserResponse {
    fn new(user: &AuthenticatedUser, characters: &CharacterDirectory, politics: &PoliticsDirectory) -> Result<Self> {
        let mut user_financing = vec![crate::handlers::bases::Financing {
            financier: user.user_id().clone(),
            share: crate::services::credit_exchange_service::Share::default(),
        }];
        characters
            .display_financing(&mut user_financing)
            .ok_or(UserError::InternalError)?;

        let bloc_permissions = user
            .bloc_permissions()
            .iter()
            .map(|(key, access)| {
                politics
                    .bloc_name(key)
                    .map(|name| (name.to_owned(), *access))
                    .ok_or(UserError::InternalError)
            })
            .collect::<Result<_>>()?;
        let zone_permissions = user
            .zone_permissions()
            .iter()
            .map(|(key, access)| {
                politics
                    .zone_name(key)
                    .map(|name| (name.to_owned(), *access))
                    .ok_or(UserError::InternalError)
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            user_id: user_financing.remove(0).financier,
            bloc_permissions,
            zone_permissions,
        })
    }
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
    characters: web::Data<CharacterDirectory>,
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

    Identity::login(&request.extensions(), authenticated_user.user_id().clone().into()).map_err(|error| {
        log::error!("Error storing identity: {error}");
        UserError::InternalError
    })?;

    let mut user_financing = vec![crate::handlers::bases::Financing {
        financier: authenticated_user.user_id().clone(),
        share: crate::services::credit_exchange_service::Share::default(),
    }];
    characters
        .display_financing(&mut user_financing)
        .ok_or(UserError::InternalError)?;

    Ok(HttpResponse::Ok().json(LoginResponse {
        user_id: user_financing.remove(0).financier,
    }))
}

/// Return the currently authenticated user including permissions.
#[utoipa::path(
    operation_id = "getCurrentUser",
    tag = AUTH,
    responses(
        (status = 200, description = "Current authenticated user", body = AuthenticatedUser),
        (status = 401, description = "Not authenticated", body = String, content_type = "text/html"),
        (status = 500, description = "Failed to read the authenticated user from the session", body = String, content_type = "text/html")
    )
)]
#[get("/me")]
pub(crate) async fn get_current_user(
    session: Session,
    characters: web::Data<CharacterDirectory>,
    politics: web::Data<PoliticsDirectory>,
) -> Result<impl Responder> {
    let authenticated_user = session
        .get::<AuthenticatedUser>(AUTHENTICATED_USER_SESSION_KEY)
        .map_err(|error| {
            log::error!("Error reading authenticated user from session: {error}");
            UserError::InternalError
        })?
        .ok_or(UserError::Unauthorized)?;

    Ok(HttpResponse::Ok().json(CurrentUserResponse::new(&authenticated_user, &characters, &politics)?))
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
    use actix_identity::IdentityMiddleware;
    use actix_session::{Session, SessionMiddleware, storage::CookieSessionStore};
    use actix_web::{
        App, HttpResponse,
        cookie::Key,
        http::{StatusCode, header},
        post, test, web,
    };

    use super::get_current_user;
    use crate::{
        config::{CharacterDirectory, PoliticsDirectory},
        services::auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthenticatedUser},
    };

    fn character_directory() -> web::Data<CharacterDirectory> {
        web::Data::new(CharacterDirectory::for_test("alice", "Alice"))
    }

    fn politics_directory() -> web::Data<PoliticsDirectory> {
        web::Data::new(PoliticsDirectory::for_test(("west", "WEST"), ("zone_w", "Zone West")))
    }

    #[post("/test/session")]
    async fn seed_authenticated_session(session: Session, user: web::Data<AuthenticatedUser>) -> HttpResponse {
        session.insert(AUTHENTICATED_USER_SESSION_KEY, user.get_ref()).unwrap();
        HttpResponse::Ok().finish()
    }

    #[actix_web::test]
    async fn current_user_returns_unauthorized_without_a_session() {
        let app = test::init_service(
            App::new()
                .wrap(IdentityMiddleware::default())
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .app_data(character_directory())
                .app_data(politics_directory())
                .service(get_current_user),
        )
        .await;

        let request = test::TestRequest::get().uri("/me").to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn current_user_returns_the_authenticated_user() {
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
                .app_data(character_directory())
                .app_data(politics_directory())
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

        let authenticated_user: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(authenticated_user["userId"], "Alice");
        assert_eq!(authenticated_user["blocPermissions"]["WEST"], "write");
        assert_eq!(authenticated_user["zonePermissions"]["Zone West"], "write");
    }
}
use std::collections::HashMap;
