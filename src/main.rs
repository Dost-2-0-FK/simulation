mod app;
mod config;
mod domain;
mod error;
mod geometry;
mod handlers;
mod persistence;
mod routes;
mod services;
mod simulation;
mod tasks;

use std::sync::atomic::Ordering;

use actix_identity::IdentityMiddleware;
use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_web::{App, HttpResponse, HttpServer, middleware::Logger, web};
use anyhow::Context;
use utoipa_actix_web::{AppExt, scope};
use utoipa_swagger_ui::SwaggerUi;

use crate::services::coordination_service::CoordinationService;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let config = config::Config::parse()
        .await
        .context("parsing config file")
        .map_err(|e| {
            log::error!("{:#}", e);
            std::io::Error::other(e)
        })?;
    let auth_cookie_key = config.auth_cookie_key();
    let auth_service = config.auth_service().clone();
    let credit_exchange_service = config.credit_exchange_service().clone();
    let name_mappings = config.name_mappings();
    let coordination_api_key = std::env::var("COORDINATION_API_KEY")
        .context("COORDINATION_API_KEY must be set")
        .map_err(|e| {
            log::error!("{e:#}");
            std::io::Error::other(e)
        })?;
    if coordination_api_key.is_empty() {
        let error = std::io::Error::other("COORDINATION_API_KEY must not be empty");
        log::error!("{error}");
        return Err(error);
    }
    let coordination_service = CoordinationService::new(coordination_api_key);
    let resources = config.resources().cloned().collect::<Vec<_>>();
    let server_address = config.server_address();

    // Start the simulation in the background and bind the HTTP server
    // immediately, so liveness/readiness probes are served during the
    // potentially slow initialisation instead of the pod being killed before it
    // ever listens.
    let (tx, ready) = app::spawn_simulation(config);

    HttpServer::new(move || {
        let logger = Logger::default();
        let session = SessionMiddleware::new(CookieSessionStore::default(), auth_cookie_key.clone());

        App::new()
            .into_utoipa_app()
            .openapi(app::openapi())
            .map(|app| app.wrap(IdentityMiddleware::default()).wrap(session).wrap(logger))
            .app_data(web::Data::new(tx.clone()))
            .app_data(web::Data::new(auth_service.clone()))
            .app_data(web::Data::new(credit_exchange_service.clone()))
            .app_data(web::Data::from(name_mappings.clone()))
            .app_data(web::Data::new(coordination_service.clone()))
            .app_data(web::Data::new(resources.clone()))
            .service(scope::scope("/api").configure(routes::configure))
            .openapi_service(|api| SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", api))
            .into_app()
            // Liveness: the process is up and serving HTTP. Available
            // immediately, even while the simulation is still initialising.
            .route("/healthz", web::get().to(|| async { HttpResponse::Ok().finish() }))
            // Readiness: 200 only once the simulation has finished initialising
            // (Mongo connected, seeds registered, command loop running); 503
            // until then so no traffic is routed to a half-initialised instance.
            .route(
                "/readyz",
                web::get().to({
                    let ready = ready.clone();
                    move || {
                        let ready = ready.clone();
                        async move {
                            if ready.load(Ordering::SeqCst) {
                                HttpResponse::Ok().finish()
                            } else {
                                HttpResponse::ServiceUnavailable().finish()
                            }
                        }
                    }
                }),
            )
    })
    .bind(server_address)?
    .run()
    .await
}
