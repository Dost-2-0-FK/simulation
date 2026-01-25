use actix_web::{HttpResponse, Responder, get, post, web};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::{
    Command,
    error::{Result, UserError},
    placement::{Placement, PlacementId},
};

#[get("/units")]
async fn get_units(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    // This channel is one-shot: it is only used once and gets re-created on every request
    let (get_units_tx, get_units_rx) = tokio::sync::oneshot::channel();

    // Via the global channel, send a command to the state loop to query the count. The command includes the one-shot
    // channel sender from which we're going to get the response.
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

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Percentage(f32);

impl Percentage {
    pub(crate) fn as_factor(self) -> f32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Percentage {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let percent = f32::deserialize(deserializer)?;

        if !(0.0..=100.0).contains(&percent) {
            Err(serde::de::Error::custom("percentage must be between 0 and 100"))
        } else {
            Ok(Percentage(percent / 100.0))
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PaymentInfo {
    pub(crate) financier_id: String,
    pub(crate) percentage: Percentage,
}

/// Create a base on a placement
/// POST /api/base?placement=<placement_id> (JSON payload: {financier: <financier_id: str>, percentage: <value: int>})
#[post("/base/")]
async fn post_create_base(
    placement_id: web::Query<PlacementId>,
    payment_info: web::Json<PaymentInfo>,
    tx: web::Data<mpsc::Sender<Command>>,
) -> Result<impl Responder> {
    let (create_base_tx, result_rx) = tokio::sync::oneshot::channel();

    tx.send(Command::CreateBase {
        placement_id: placement_id.into_inner(),
        payment_info: payment_info.into_inner(),
        response: create_base_tx,
    })
    .await
    .map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    // Receive the response from the state.
    let result = result_rx.await.map_err(|e| {
        log::error!("Error receiving response: {e}");
        UserError::InternalError
    })?;

    result.map(|()| HttpResponse::Ok())
}
