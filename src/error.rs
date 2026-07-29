use actix_web::{
    HttpResponse, error,
    http::{StatusCode, header::ContentType},
};
use derive_more::derive::{Display, Error};

pub type Result<T> = core::result::Result<T, UserError>;

#[derive(Debug, Display, Error)]
pub enum UserError {
    #[display("An internal error occurred. Please try again later.")]
    InternalError,
    #[display("Failed to query credit service.")]
    CreditExchangeQueryFailed,
    #[display("Not found: {_0}")]
    NotFound(#[error(not(source))] &'static str),
    #[display("Conflict: {_0}")]
    Conflict(#[error(not(source))] &'static str),
    #[display("Bad request: {_0}")]
    BadRequest(#[error(not(source))] &'static str),
    #[display("Unauthorized.")]
    Unauthorized,
    #[display("Forbidden.")]
    Forbidden,
    #[display("{_0}")]
    PaymentRequired(#[error(not(source))] String),
    #[display("{body}")]
    CreditExchange { status: u16, body: String },
    #[display("{body}")]
    AuthService { status: u16, body: String },
}

impl error::ResponseError for UserError {
    fn error_response(&self) -> HttpResponse {
        if let Self::PaymentRequired(body)
        | Self::CreditExchange { body, .. }
        | Self::AuthService { body, .. } = self
        {
            return HttpResponse::build(self.status_code())
                .insert_header(ContentType::plaintext())
                .body(body.clone());
        }

        HttpResponse::build(self.status_code())
            .insert_header(ContentType::html())
            .body(self.to_string())
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            UserError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            UserError::CreditExchangeQueryFailed => StatusCode::INTERNAL_SERVER_ERROR,
            UserError::NotFound(_) => StatusCode::NOT_FOUND,
            UserError::Conflict(_) => StatusCode::CONFLICT,
            UserError::BadRequest(_) => StatusCode::BAD_REQUEST,
            UserError::Unauthorized => StatusCode::UNAUTHORIZED,
            UserError::Forbidden => StatusCode::FORBIDDEN,
            UserError::PaymentRequired(_) => StatusCode::PAYMENT_REQUIRED,
            UserError::CreditExchange { status, .. } => StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            UserError::AuthService { status, .. } => StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{ResponseError, body::to_bytes, http::StatusCode};

    use super::UserError;

    #[actix_web::test]
    async fn payment_required_error_returns_402_and_preserves_body() {
        let response = UserError::PaymentRequired("Insufficient credit for booking".to_string()).error_response();

        assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            to_bytes(response.into_body()).await.unwrap(),
            "Insufficient credit for booking"
        );
    }

    #[actix_web::test]
    async fn credit_exchange_query_failure_returns_500() {
        let response = UserError::CreditExchangeQueryFailed.error_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            to_bytes(response.into_body()).await.unwrap(),
            "Failed to query credit service."
        );
    }

    #[actix_web::test]
    async fn auth_service_response_is_forwarded() {
        let response = UserError::AuthService {
            status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            body: "Invalid social-rule update".to_string(),
        }
        .error_response();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            to_bytes(response.into_body()).await.unwrap(),
            "Invalid social-rule update"
        );
    }
}
