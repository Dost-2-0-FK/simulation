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

use actix_web::{App, HttpServer, middleware::Logger, web};
use utoipa_actix_web::{AppExt, scope};
use utoipa_swagger_ui::SwaggerUi;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let tx = app::start_simulation().await.map_err(|e| {
        log::error!("{:#}", e);
        std::io::Error::other(e)
    })?;

    HttpServer::new(move || {
        let logger = Logger::default();

        App::new()
            .into_utoipa_app()
            .openapi(app::openapi())
            .map(|app| app.wrap(logger))
            .app_data(web::Data::new(tx.clone()))
            .service(scope::scope("/api").configure(routes::configure))
            .openapi_service(|api| SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", api))
            .into_app()
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
