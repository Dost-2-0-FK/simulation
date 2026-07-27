use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{
        BaseId, BlocName, Combat, CombatEvent, CombatId, CombatState, CombatStructureSnapshot, NameMappings, TrustId,
        UnitId, UnitKilled,
    },
    error::{Result, UserError},
    geometry::Point,
    simulation::Command,
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
pub(crate) struct UnitKilledResponse {
    killer: UnitId,
    killed: UnitId,
}

impl From<&UnitKilled> for UnitKilledResponse {
    fn from(event: &UnitKilled) -> Self {
        Self {
            killer: event.killer(),
            killed: event.killed(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum CombatEventResponse {
    UnitsKilled { units: Vec<UnitKilledResponse> },
    TrustDestroyed { id: TrustId },
    BaseDestroyed { id: BaseId },
}

impl CombatEventResponse {
    fn from_event(event: &CombatEvent) -> Option<Self> {
        match event {
            CombatEvent::None => None,
            CombatEvent::UnitsKilled { units } => Some(Self::UnitsKilled {
                units: units.iter().map(Into::into).collect(),
            }),
            CombatEvent::TrustDestroyed { id, .. } => Some(Self::TrustDestroyed { id: *id }),
            CombatEvent::BaseDestroyed { id, .. } => Some(Self::BaseDestroyed { id: *id }),
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
    events: Vec<CombatEventResponse>,
}

impl CombatResponse {
    pub(crate) async fn from_combat(combat: &Combat, mappings: &NameMappings) -> Result<Self> {
        let units = combat
            .unit_ids_by_bloc()
            .await
            .into_iter()
            .map(|(bloc, unit_ids)| {
                let bloc = mappings.bloc_name(&bloc).cloned().ok_or_else(|| {
                    log::error!("Combat references unknown configured bloc key {bloc}");
                    UserError::InternalError
                })?;
                Ok(CombatUnitsResponse { bloc, unit_ids })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            id: combat.id(),
            position: combat.position(),
            units,
            structure: combat.structure_snapshot().await.into(),
            state: combat.state(),
            events: combat
                .events()
                .iter()
                .filter_map(CombatEventResponse::from_event)
                .collect(),
        })
    }
}

/// List all combats.
#[utoipa::path(
    operation_id = "listCombats",
    tag = COMBATS,
    responses(
        (status = 200, description = "All existing combats", body = [CombatResponse]),
        (status = 500, description = "Failed to retrieve combats", body = String, content_type = "text/html")
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
    })??;

    Ok(HttpResponse::Ok().json(combats))
}
