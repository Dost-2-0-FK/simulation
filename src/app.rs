use anyhow::{Context, Result};
use tokio::sync::mpsc;
use utoipa::OpenApi;

use crate::{
    config::Config,
    persistence::MongoPersistence,
    simulation::{Command, Simulation},
    tasks::{periodic_combat_tick, periodic_move, periodic_persist},
};

pub(crate) async fn start_simulation(config: Config) -> Result<mpsc::Sender<Command>> {
    const MAX_MESSAGE_COUNT: usize = 100;
    let (tx, rx) = mpsc::channel(MAX_MESSAGE_COUNT);
    let persistence = MongoPersistence::connect(config.persistence())
        .await
        .context("connecting persistence layer".to_string())?;
    let loaded_state = persistence
        .load(config.placements())
        .await
        .context("loading simulation state from persistence".to_string())?;

    let persist_interval = config.persistence().interval();
    let movement_interval = config.movement_interval();
    let combat_tick_interval = config.combat_tick_interval();

    let simulation = Simulation::builder()
        .config(config)
        .persistence(persistence)
        .loaded_state(loaded_state)
        .receiver(rx)
        .build();

    tokio::spawn(simulation.run());
    tokio::spawn(periodic_persist(tx.clone(), persist_interval));
    tokio::spawn(periodic_move(tx.clone(), movement_interval));
    tokio::spawn(periodic_combat_tick(tx.clone(), combat_tick_interval));

    Ok(tx)
}

pub(crate) fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[derive(OpenApi)]
#[openapi(
        tags(
            (name = "units", description = "Endpoints related to military units."),
            (name = "bases", description = "Endpoints related to military bases."),
            (name = "placements", description = "Endpoints related to placements."),
            (name = "trusts", description = "Endpoints related to trusts."),
            (name = "blocs", description = "Endpoints related to blocs."),
            (name = "combats", description = "Endpoints related to combats."),
            (name = "zones", description = "Endpoints related to zones."),
            (name = "auth", description = "Endpoints related to user authentication.")
        ),
    )]
struct ApiDoc;
