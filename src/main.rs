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
use tokio::sync::mpsc;

use crate::{
    simulation::simulation,
    state::{Command, state_loop},
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    const MAX_MESSAGE_COUNT: usize = 100;
    // Create a channel to query or mutate state from the state loop
    let (tx, rx) = mpsc::channel(MAX_MESSAGE_COUNT);

    // Spawn state loop and simulation separately so they never block each other.

    // The state loop receives commands
    tokio::spawn(state_loop(rx));

    // The simulation sends commands
    tokio::spawn(simulation(tx.clone()));

    HttpServer::new(move || {
        let logger = Logger::default();

        App::new()
            .wrap(logger)
            // The API also sends commands
            .app_data(web::Data::new(tx.clone()))
            .service(service::hello)
            .service(service::count)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
