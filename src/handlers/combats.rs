use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{BaseId, BlocName, Combat, CombatId, CombatState, CombatStructureSnapshot, TrustId, UnitId},
    error::{Result, UserError},
    geometry::Point,
    state::Command,
};

const COMBATS: &str = "combats";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CombatUnitsResponse {
    bloc: BlocName,
    unit_ids: Vec<UnitId>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum CombatStructureResponse {
    None,
    Trust { id: TrustId, destruction_threshold: u32 },
    Base { id: BaseId, destruction_threshold: u32 },
}

impl From<CombatStructureSnapshot> for CombatStructureResponse {
    fn from(snapshot: CombatStructureSnapshot) -> Self {
        match snapshot {
            CombatStructureSnapshot::None => Self::None,
            CombatStructureSnapshot::Trust {
                id,
                destruction_threshold,
            } => Self::Trust {
                id,
                destruction_threshold,
            },
            CombatStructureSnapshot::Base {
                id,
                destruction_threshold,
            } => Self::Base {
                id,
                destruction_threshold,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CombatResponse {
    id: CombatId,
    position: Point,
    units: Vec<CombatUnitsResponse>,
    structure: CombatStructureResponse,
    state: CombatState,
}

impl CombatResponse {
    pub(crate) async fn from_combat(combat: &Combat) -> Self {
        let units = combat
            .unit_ids_by_bloc()
            .await
            .into_iter()
            .map(|(bloc, unit_ids)| CombatUnitsResponse { bloc, unit_ids })
            .collect();

        Self {
            id: combat.id(),
            position: combat.position(),
            units,
            structure: combat.structure_snapshot().await.into(),
            state: combat.state(),
        }
    }
}

/// List all combats.
#[utoipa::path(
    operation_id = "listCombats",
    tag = COMBATS,
    responses(
        (status = 200, description = "All existing combats")
    )
)]
#[get("/combats")]
pub(crate) async fn list(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetCombats(sender)).await.map_err(|e| {
        log::error!("Error sending command: {e}");
        UserError::InternalError
    })?;

    let combats = receiver.await.map_err(|e| {
        log::error!("Error receiving combats: {e}");
        UserError::InternalError
    })?;

    Ok(HttpResponse::Ok().json(combats))
}
