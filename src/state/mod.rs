//! This module contains the simulation state that is queried or mutated by users or the simulation itself.

mod command;
use std::{collections::HashMap, sync::Arc};

pub(crate) use command::Command;
use tokio::sync::{RwLock, mpsc::Receiver};
use typed_builder::TypedBuilder;

use crate::{
    config::Config,
    domain::{Bloc, BlocName},
    persistence::{LoadedState, MongoPersistence},
};

#[derive(TypedBuilder)]
pub(crate) struct State {
    config: Config,
    persistence: MongoPersistence,
    loaded_state: LoadedState,
    // units,
    // etc,
    receiver: Receiver<Command>,
}

impl State {
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
        } = self.loaded_state;

        let blocs: HashMap<BlocName, Arc<RwLock<Bloc>>> = if blocs.is_empty() {
            log::info!("No blocs persisted, instantiating from config.");
            self.config
                .blocs()
                .map(|bloc| (bloc.name().clone(), Arc::new(RwLock::new((*bloc).clone()))))
                .collect()
        } else {
            log::info!("Loaded blocs from database, ignoring blocs listed in config.");
            blocs
                .into_iter()
                .map(|bloc| (bloc.name().clone(), Arc::new(RwLock::new(bloc))))
                .collect()
        };

        let units = units
            .into_iter()
            .map(|unit| (unit.id().clone(), Arc::new(RwLock::new(unit))))
            .collect::<HashMap<_, _>>();
        // bases and trusts are already HashMaps from LoadedState

        command::run(
            self.receiver,
            &self.config,
            &self.persistence,
            units,
            bases,
            trusts,
            blocs,
        )
        .await;
    }
}
