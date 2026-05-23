use anyhow::{Context, Result};
use tokio::sync::mpsc;
use utoipa::OpenApi;

use crate::{
    config::Config,
    persistence::MongoPersistence,
    simulation::{periodic_persist, simulation},
    state::{Command, State},
};

pub(crate) async fn setup_state() -> Result<mpsc::Sender<Command>> {
    env_logger::init();
    let config = Config::parse().await.context("parsing config file".to_string())?;
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

    let state = State::builder()
        .config(config)
        .persistence(persistence)
        .loaded_state(loaded_state)
        .receiver(rx)
        .build();

    tokio::spawn(state.run());
    tokio::spawn(simulation(tx.clone()));
    tokio::spawn(periodic_persist(tx.clone(), persist_interval));

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
            (name = "zones", description = "Endpoints related to zones.")
        ),
    )]
struct ApiDoc;
