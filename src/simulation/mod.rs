//! This module contains the simulation state that is queried or mutated by users or the simulation itself.

mod command;
use std::collections::HashMap;

pub(crate) use command::Command;
use tokio::sync::mpsc::Receiver;
use typed_builder::TypedBuilder;

use crate::{
    config::Config,
    persistence::{LoadedState, MongoPersistence},
};

#[derive(TypedBuilder)]
pub(crate) struct Simulation {
    config: Config,
    persistence: MongoPersistence,
    loaded_state: LoadedState,
    // units,
    // etc,
    receiver: Receiver<Command>,
}

impl Simulation {
    /// Spawn a state loop and wait for [Command]s to query the state or mutate it.
    ///
    /// Returns when the channel is closed and when there are no more messages in the channels buffer, i.e., no more
    /// messages are going to be received.
    pub(crate) async fn run(self) {
        let LoadedState {
            bases,
            trusts,
            units,
            blocs,
            combats,
        } = self.loaded_state;

        let live_blocs = self.config.blocs().collect::<Vec<_>>();
        let mut blocs_by_name = HashMap::with_capacity(live_blocs.len());
        for bloc in live_blocs {
            let name = bloc.read().await.name().clone();
            blocs_by_name.insert(name, bloc);
        }

        if blocs.is_empty() {
            log::info!("No blocs persisted, instantiating from config.");
        } else {
            log::info!("Loaded blocs from database, applying persisted bloc overrides.");
            for persisted_bloc in blocs {
                match blocs_by_name.get(persisted_bloc.name()) {
                    Some(live_bloc) => *live_bloc.write().await = persisted_bloc,
                    None => log::warn!(
                        "Ignoring persisted bloc {} because it is not present in config.",
                        persisted_bloc.name()
                    ),
                }
            }
        }

        // bases, trusts, units, and combats are already shared maps from LoadedState

        command::run(
            self.receiver,
            &self.config,
            &self.persistence,
            units,
            bases,
            trusts,
            blocs_by_name,
            combats,
        )
        .await;
    }
}
