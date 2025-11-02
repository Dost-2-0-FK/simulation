mod error;
mod simulation;
mod state;

use actix_web::{App, HttpResponse, HttpServer, Responder, get, middleware::Logger, web};
use tokio::sync::mpsc;

use crate::{
    error::{Result, UserError},
    simulation::simulation,
    state::{Command, state_loop},
};

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

/// PoC: query simualted state
#[get("/count")]
async fn count(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    // This channel is one-shot: it is only used once and gets re-created on every request
    let (get_count_tx, get_count_rx) = tokio::sync::oneshot::channel();

    // Via the global channel, send a command to the state loop to query the count. The command includes the one-shot
    // channel sender from which we're going to get the response.
    tx.send(Command::GetCount(get_count_tx))
        .await
        .map_err(|e| {
            log::error!("Error sending command: {e}");
            UserError::InternalError
        })?;

    // Receive the response from the state.
    let count_value = get_count_rx.await.map_err(|e| {
        log::error!("Error receiving count: {e}");
        UserError::InternalError
    })?;

    let response = HttpResponse::Ok().body(format!("{count_value}"));
    Ok(response)
}

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
            .service(hello)
            .service(count)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
