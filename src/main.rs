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

use actix_identity::IdentityMiddleware;
use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_web::{App, HttpServer, middleware::Logger, web};
use anyhow::Context;
use utoipa_actix_web::{AppExt, scope};
use utoipa_swagger_ui::SwaggerUi;

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
    let server_address = config.server_address();

    let tx = app::start_simulation(config).await.map_err(|e| {
        log::error!("{:#}", e);
        std::io::Error::other(e)
    })?;

    HttpServer::new(move || {
        let logger = Logger::default();
        let session = SessionMiddleware::new(CookieSessionStore::default(), auth_cookie_key.clone());

        App::new()
            .into_utoipa_app()
            .openapi(app::openapi())
            .map(|app| app.wrap(IdentityMiddleware::default()).wrap(session).wrap(logger))
            .app_data(web::Data::new(tx.clone()))
            .app_data(web::Data::new(services::auth_service::AuthService))
            .service(scope::scope("/api").configure(routes::configure))
            .openapi_service(|api| SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", api))
            .into_app()
    })
    .bind(server_address)?
    .run()
    .await
}
