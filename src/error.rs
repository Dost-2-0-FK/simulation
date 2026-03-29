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
    #[display("Not found: {_0}")]
    NotFound(#[error(not(source))] &'static str),
}

impl error::ResponseError for UserError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::html())
            .body(self.to_string())
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            UserError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            UserError::NotFound(_) => StatusCode::NOT_FOUND,
        }
    }
}
