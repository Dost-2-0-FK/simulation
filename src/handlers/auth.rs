use actix_identity::Identity;
use actix_session::Session;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, Responder, post, web};
use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, UserError},
    services::auth_service::{AUTHENTICATED_USER_SESSION_KEY, AuthService, LoginCredentials},
};

const AUTH: &str = "auth";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginRequest {
    user_id: String,
    password: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginResponse {
    user_id: String,
}

/// Log in and persist the authenticated user in the identity session.
#[utoipa::path(
    operation_id = "login",
    tag = AUTH,
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login completed successfully", body = LoginResponse),
        (status = 401, description = "Invalid credentials")
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

    Identity::login(&request.extensions(), authenticated_user.user_id().to_string()).map_err(|error| {
        log::error!("Error storing identity: {error}");
        UserError::InternalError
    })?;

    Ok(HttpResponse::Ok().json(LoginResponse {
        user_id: authenticated_user.user_id().to_string(),
    }))
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
