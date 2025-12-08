mod config;
mod error;
mod geometry;
mod military;
mod money;
mod placement;
mod politics;
mod service;
mod simulation;
mod state;
mod trust;

use actix_web::{App, HttpServer, middleware::Logger, web};
use anyhow::{Context, Result};
use tokio::sync::mpsc;

use crate::{
    config::Config,
    simulation::simulation,
    state::{Command, state_loop},
};

async fn setup() -> Result<mpsc::Sender<Command>> {
    env_logger::init();
    let _c = Config::parse().await.context("parsing config file".to_string())?;
    const MAX_MESSAGE_COUNT: usize = 100;
    // Create a channel to query or mutate state from the state loop
    let (tx, rx) = mpsc::channel(MAX_MESSAGE_COUNT);

    // Spawn state loop and simulation separately so they never block each other.

    // The state loop receives commands
    tokio::spawn(state_loop(rx));

    // The simulation sends commands
    tokio::spawn(simulation(tx.clone()));
    Ok(tx)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let tx = setup().await.map_err(|e| {
        log::error!("{:#}", e);
        std::io::Error::other(e)
    })?;

    HttpServer::new(move || {
        let logger = Logger::default();

        App::new()
            .wrap(logger)
            // The API also sends commands
            .app_data(web::Data::new(tx.clone()))
            .service(
                web::scope("/api")
                .service(service::get_units)
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
