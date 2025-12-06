use actix_web::{HttpResponse, Responder, get, web};
use tokio::sync::mpsc;

use crate::{
    Command,
    error::{Result, UserError},
};

#[get("/")]
pub(super) async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

/// PoC: query simualted state
#[get("/count")]
async fn count(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    // This channel is one-shot: it is only used once and gets re-created on every request
    let (get_count_tx, get_count_rx) = tokio::sync::oneshot::channel();

    // Via the global channel, send a command to the state loop to query the count. The command includes the one-shot
    // channel sender from which we're going to get the response.
    tx.send(Command::GetCount(get_count_tx)).await.map_err(|e| {
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
