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
    let mut loaded_state = persistence
        .load(config.placements())
        .await
        .context("loading simulation state from persistence".to_string())?;

    if loaded_state.is_empty() {
        let seeded = config.seeded_structures();
        log::info!(
            "No simulation state persisted, instantiating {} bases and {} trusts from config.",
            seeded.bases.len(),
            seeded.trusts.len(),
        );

        for base in seeded.bases.values() {
            let base = base.read().await;
            config
                .credit_exchange_service()
                .register_prepaid_military_base(&base)
                .await
                .context("registering configured base with credit-exchange service")?;
        }
        for trust in seeded.trusts.values() {
            let trust = trust.read().await;
            config
                .credit_exchange_service()
                .register_prepaid_trust(&trust)
                .await
                .context("registering configured trust with credit-exchange service")?;
        }

        loaded_state.bases = seeded.bases;
        loaded_state.trusts = seeded.trusts;
    } else {
        log::info!("Loaded simulation state from database, ignoring configured base and trust seeds.");
    }

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
            (name = "resources", description = "Endpoints related to configured resources."),
            (name = "trusts", description = "Endpoints related to trusts."),
            (name = "blocs", description = "Endpoints related to blocs."),
            (name = "combats", description = "Endpoints related to combats."),
            (name = "zones", description = "Endpoints related to zones."),
            (name = "auth", description = "Endpoints related to user authentication.")
        ),
    )]
struct ApiDoc;
