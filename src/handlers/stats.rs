use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    domain::{BlocName, BlocStats, StructureStats, UnitStats},
    error::{Result, UserError},
    simulation::Command,
};

const STATS: &str = "stats";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StructureStatsResponse {
    destroyed_in_combat: u64,
    destroyed_via_coordination_service: u64,
    destroyed_by_authorized_users: u64,
    built: u64,
    remaining: u64,
}

impl StructureStatsResponse {
    pub(crate) fn new(stats: &StructureStats, remaining: u64) -> Self {
        Self {
            destroyed_in_combat: stats.destroyed_in_combat(),
            destroyed_via_coordination_service: stats.destroyed_via_coordination_service(),
            destroyed_by_authorized_users: stats.destroyed_by_authorized_users(),
            built: stats.built(),
            remaining,
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnitStatsResponse {
    destroyed_by_enemies: u64,
    produced: u64,
    remaining: u64,
}

impl UnitStatsResponse {
    pub(crate) fn new(stats: &UnitStats, remaining: u64) -> Self {
        Self {
            destroyed_by_enemies: stats.destroyed_by_enemies(),
            produced: stats.produced(),
            remaining,
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlocStatsResponse {
    bloc: BlocName,
    trusts: StructureStatsResponse,
    bases: StructureStatsResponse,
    units: UnitStatsResponse,
    combat_ready: bool,
}

impl BlocStatsResponse {
    pub(crate) fn new(
        bloc: BlocName,
        stats: &BlocStats,
        remaining_trusts: u64,
        remaining_bases: u64,
        remaining_units: u64,
        combat_ready: bool,
    ) -> Self {
        Self {
            bloc,
            trusts: StructureStatsResponse::new(stats.trusts(), remaining_trusts),
            bases: StructureStatsResponse::new(stats.bases(), remaining_bases),
            units: UnitStatsResponse::new(stats.units(), remaining_units),
            combat_ready,
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatsResponse {
    runtime_seconds: u64,
    blocs: Vec<BlocStatsResponse>,
}

impl StatsResponse {
    pub(crate) fn new(runtime_seconds: u64, blocs: Vec<BlocStatsResponse>) -> Self {
        Self { runtime_seconds, blocs }
    }
}

/// Return cumulative simulation statistics.
#[utoipa::path(
    operation_id = "getStats",
    tag = STATS,
    responses(
        (status = 200, description = "Cumulative simulation statistics", body = StatsResponse),
        (status = 500, description = "Failed to calculate statistics", body = String, content_type = "text/html")
    )
)]
#[get("/stats")]
pub(crate) async fn get(tx: web::Data<mpsc::Sender<Command>>) -> Result<impl Responder> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tx.send(Command::GetStats(sender)).await.map_err(|error| {
        log::error!("Error sending stats command: {error}");
        UserError::InternalError
    })?;

    let response = receiver.await.map_err(|error| {
        log::error!("Error receiving stats: {error}");
        UserError::InternalError
    })??;

    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use actix_web::{App, test, web};
    use tokio::sync::mpsc;

    use super::*;
    use crate::domain::BlocStats;

    #[actix_web::test]
    async fn returns_the_documented_json_shape() {
        let (tx, mut rx) = mpsc::channel(1);
        actix_web::rt::spawn(async move {
            let Command::GetStats(response) = rx.recv().await.unwrap() else {
                panic!("expected stats command");
            };
            response
                .send(Ok(StatsResponse::new(
                    123,
                    vec![BlocStatsResponse::new(
                        BlocName::from("Bloc A".to_string()),
                        &BlocStats::default(),
                        4,
                        2,
                        7,
                        true,
                    )],
                )))
                .unwrap();
        });
        let app = test::init_service(App::new().app_data(web::Data::new(tx)).service(get)).await;

        let response: serde_json::Value =
            test::call_and_read_body_json(&app, test::TestRequest::get().uri("/stats").to_request()).await;

        assert_eq!(
            response,
            serde_json::json!({
                "runtimeSeconds": 123,
                "blocs": [{
                    "bloc": "Bloc A",
                    "trusts": {
                        "destroyedInCombat": 0,
                        "destroyedViaCoordinationService": 0,
                        "destroyedByAuthorizedUsers": 0,
                        "built": 0,
                        "remaining": 4
                    },
                    "bases": {
                        "destroyedInCombat": 0,
                        "destroyedViaCoordinationService": 0,
                        "destroyedByAuthorizedUsers": 0,
                        "built": 0,
                        "remaining": 2
                    },
                    "units": {
                        "destroyedByEnemies": 0,
                        "produced": 0,
                        "remaining": 7
                    },
                    "combatReady": true
                }]
            })
        );
    }
}
