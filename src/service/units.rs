use actix_web::{HttpResponse, Responder, get, web};
use tokio::sync::mpsc;

use crate::{
    Command,
    error::{Result, UserError},
};

const UNITS: &str = "units";

/// List all units.
#[utoipa::path(
    tag = UNITS,
    responses(
        (status = 200, description = "All existing units")
    )
)]
#[get("/units")]
async fn get(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    // This channel is one-shot: it is only used once and gets re-created on every request
    let (get_units_tx, get_units_rx) = tokio::sync::oneshot::channel();

    // Via the global channel, send a command to the state loop to query the count. The command includes the
    // one-shot channel sender from which we're going to get the response.
    tx.send(Command::GetUnits(get_units_tx)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    // Receive the response from the state.
    let units = get_units_rx.await.map_err(|e| {
        log::error!("Error receiving count: {e}");
        UserError::InternalError
    })?;

    let response = HttpResponse::Ok().json(units.as_slice());
    Ok(response)
}
