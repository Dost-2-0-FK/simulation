use std::{
    collections::HashSet,
    fmt,
    future::{Ready, ready},
};

use actix_web::{FromRequest, HttpRequest, dev::Payload, http::header, web};
use subtle::ConstantTimeEq;

use crate::error::UserError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CoordinationCapability {
    TriggerUnitProduction,
    PublishBaseProduction,
    PublishTrustProduction,
}

#[derive(Clone)]
pub(crate) struct CoordinationService {
    api_key: String,
    capabilities: HashSet<CoordinationCapability>,
}

impl fmt::Debug for CoordinationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CoordinationService")
            .field("api_key", &"<redacted>")
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl CoordinationService {
    pub(crate) fn new(api_key: String) -> Self {
        Self {
            api_key,
            capabilities: HashSet::from([
                CoordinationCapability::TriggerUnitProduction,
                CoordinationCapability::PublishBaseProduction,
                CoordinationCapability::PublishTrustProduction,
            ]),
        }
    }

    fn authenticate(&self, api_key: &str) -> Option<CoordinationAuthorization> {
        let authenticated = bool::from(self.api_key.as_bytes().ct_eq(api_key.as_bytes()));
        authenticated.then(|| CoordinationAuthorization {
            capabilities: self.capabilities.clone(),
        })
    }
}

pub(crate) struct CoordinationAuthorization {
    capabilities: HashSet<CoordinationCapability>,
}

impl CoordinationAuthorization {
    pub(crate) fn require(&self, capability: CoordinationCapability) -> Result<(), UserError> {
        self.capabilities
            .contains(&capability)
            .then_some(())
            .ok_or(UserError::Forbidden)
    }
}

impl FromRequest for CoordinationAuthorization {
    type Error = UserError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(request: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let authorization = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|api_key| !api_key.is_empty())
            .and_then(|api_key| {
                request
                    .app_data::<web::Data<CoordinationService>>()
                    .and_then(|service| service.authenticate(api_key))
            })
            .ok_or(UserError::Unauthorized);

        ready(authorization)
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        App, HttpResponse,
        http::{StatusCode, header},
        test, web,
    };

    use super::{CoordinationAuthorization, CoordinationCapability, CoordinationService};

    async fn protected(authorization: CoordinationAuthorization) -> Result<HttpResponse, crate::error::UserError> {
        authorization.require(CoordinationCapability::TriggerUnitProduction)?;
        Ok(HttpResponse::Ok().finish())
    }

    #[actix_web::test]
    async fn valid_bearer_token_authorizes_request() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(CoordinationService::new("secret".to_string())))
                .route("/protected", web::get().to(protected)),
        )
        .await;
        let request = test::TestRequest::get()
            .uri("/protected")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .to_request();

        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn missing_or_invalid_bearer_token_is_unauthorized() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(CoordinationService::new("secret".to_string())))
                .route("/protected", web::get().to(protected)),
        )
        .await;

        for request in [
            test::TestRequest::get().uri("/protected").to_request(),
            test::TestRequest::get()
                .uri("/protected")
                .insert_header((header::AUTHORIZATION, "Bearer wrong"))
                .to_request(),
        ] {
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }
}
