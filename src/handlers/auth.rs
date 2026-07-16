use actix_identity::Identity;
use actix_session::Session;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, Responder, get, post, web};
use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, UserError},
    handlers::bases::UserId,
    services::auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthService, AuthenticatedUser, LoginCredentials},
};

const AUTH: &str = "auth";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginRequest {
    user_id: UserId,
    password: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginResponse {
    user_id: UserId,
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
        .authenticate(LoginCredentials::new(
            login_request.user_id.clone(),
            login_request.password.clone(),
        ))
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

    Ok(HttpResponse::Ok().json(LoginResponse {
        user_id: authenticated_user.user_id().clone(),
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
pub(crate) async fn get_current_user(session: Session) -> Result<impl Responder> {
    let authenticated_user = session
        .get::<AuthenticatedUser>(AUTHENTICATED_USER_SESSION_KEY)
        .map_err(|error| {
            log::error!("Error reading authenticated user from session: {error}");
            UserError::InternalError
        })?
        .ok_or(UserError::Unauthorized)?;

    Ok(HttpResponse::Ok().json(authenticated_user))
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
    use actix_session::{SessionMiddleware, storage::CookieSessionStore};
    use actix_web::{
        App,
        cookie::Key,
        http::{StatusCode, header},
        test, web,
    };

    use super::{get_current_user, login};
    use crate::services::auth_service::{AuthService, AuthenticatedUser};

    fn auth_service() -> AuthService {
        AuthService::new(
            "http://127.0.0.1:18081".parse().unwrap(),
            [crate::domain::BlocName::from("west".to_string())],
            [crate::domain::ZoneName::from("zone_w".to_string())],
        )
    }

    #[actix_web::test]
    async fn current_user_returns_unauthorized_without_a_session() {
        let app = test::init_service(
            App::new()
                .wrap(IdentityMiddleware::default())
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .service(get_current_user),
        )
        .await;

        let request = test::TestRequest::get().uri("/me").to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn current_user_returns_the_authenticated_user() {
        let app = test::init_service(
            App::new()
                .wrap(IdentityMiddleware::default())
                .wrap(SessionMiddleware::new(CookieSessionStore::default(), Key::generate()))
                .app_data(web::Data::new(auth_service()))
                .service(login)
                .service(get_current_user),
        )
        .await;

        let login_request = test::TestRequest::post()
            .uri("/login")
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .set_payload(r#"{"userId":"alice","password":"secret"}"#)
            .to_request();
        let login_response = test::call_service(&app, login_request).await;
        assert_eq!(login_response.status(), StatusCode::OK);

        let session_cookie = login_response
            .response()
            .cookies()
            .next()
            .expect("login should set a session cookie");
        let session_cookie = format!("{}={}", session_cookie.name(), session_cookie.value());
        let request = test::TestRequest::get()
            .uri("/me")
            .insert_header((header::COOKIE, session_cookie))
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);

        let authenticated_user: AuthenticatedUser = test::read_body_json(response).await;
        assert_eq!(authenticated_user.user_id().as_str(), "alice");
    }
}
