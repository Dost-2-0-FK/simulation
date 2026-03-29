use actix_web::{HttpResponse, Responder, post, web};
use serde::Deserialize;
use tokio::sync::mpsc;

use super::Result;
use crate::{Command, error::UserError, placement::PlacementId};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Percentage(f32);

#[expect(dead_code)]
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostBaseBody {
    placement_id: PlacementId,
    payment: PaymentInfo,
}

/// Create a base on a placement
/// - `POST /api/bases` (payload:
/// ````
/// {
///    placementId: <placement id>,
///    payment: {
///       financierId: <financier_id (str)>,
///       percentage: <value (int)>
///    }
/// ```
///)
#[post("/bases")]
async fn post(body: web::Json<PostBaseBody>, tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (create_base_tx, result_rx) = tokio::sync::oneshot::channel();
    let body = body.into_inner();

    tx.send(Command::CreateBase {
        placement_id: body.placement_id,
        payment_info: body.payment,
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

    result
        .inspect_err(|err| match err {
            UserError::InternalError => todo!(),
            UserError::NotFound(err) => log::info!("not found: {err}"),
        })
        .map(|()| HttpResponse::Ok())
}
