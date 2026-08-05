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
            production_units,
            zones: persisted_zones,
            stats,
        } = self.loaded_state;

        let live_blocs = self.config.blocs().collect::<Vec<_>>();
        let mut blocs_by_key = HashMap::with_capacity(live_blocs.len());
        for bloc in live_blocs {
            let key = bloc.read().await.key().clone();
            blocs_by_key.insert(key, bloc);
        }

        if blocs.is_empty() {
            log::info!("No blocs persisted, instantiating from config.");
        } else {
            log::info!("Loaded blocs from database, applying persisted bloc overrides.");
            for persisted_bloc in blocs {
                match blocs_by_key.get(persisted_bloc.key()) {
                    Some(live_bloc) => {
                        let mut live_bloc = live_bloc.write().await;
                        *live_bloc = crate::domain::Bloc::new(
                            live_bloc.key().clone(),
                            live_bloc.name().clone(),
                            persisted_bloc.chance(),
                            persisted_bloc.military_expense(),
                        );
                    }
                    None => log::warn!(
                        "Ignoring persisted bloc {} because it is not present in config.",
                        persisted_bloc.key()
                    ),
                }
            }
        }

        if persisted_zones.is_empty() {
            log::info!("No zone social-rule levels persisted; all zones use configured initial levels.");
        } else {
            log::info!("Loaded persisted zone social-rule levels; applying valid overrides to configured assignments.");
            let live_zones = self
                .config
                .zones()
                .map(|zone| (zone.key().clone(), zone))
                .collect::<HashMap<_, _>>();
            for persisted_zone in persisted_zones {
                let Some(live_zone) = live_zones.get(persisted_zone.id()) else {
                    log::warn!(
                        "Ignoring persisted social-rule levels for unknown zone {}; that removed zone has no runtime state.",
                        persisted_zone.id()
                    );
                    continue;
                };
                let persisted_rule_keys = persisted_zone
                    .social_rules()
                    .iter()
                    .map(|rule| rule.key())
                    .collect::<std::collections::HashSet<_>>();
                for persisted_rule in persisted_zone.social_rules() {
                    match live_zone
                        .apply_persisted_social_rule_level(persisted_rule.key(), persisted_rule.level())
                        .await
                    {
                        Ok(()) => {}
                        Err(crate::domain::PersistedSocialRuleError::UnassignedRule) => log::warn!(
                            "Ignoring persisted level {} for social rule {} in zone {} because the rule is no longer assigned; that removed rule has no runtime effect, while configured assignments keep their initial levels unless they have a valid persisted override.",
                            persisted_rule.level(),
                            persisted_rule.key(),
                            persisted_zone.id(),
                        ),
                        Err(crate::domain::PersistedSocialRuleError::LevelOutOfRange { min, max }) => log::warn!(
                            "Ignoring persisted level {} for social rule {} in zone {} because it is outside configured range {} through {}; that rule uses its configured initial level.",
                            persisted_rule.level(),
                            persisted_rule.key(),
                            persisted_zone.id(),
                            min,
                            max,
                        ),
                    }
                }
                for configured_rule in live_zone.social_rules().await {
                    if !persisted_rule_keys.contains(configured_rule.rule().key()) {
                        log::info!(
                            "No persisted level exists for newly configured social rule {} in zone {}; that rule keeps its configured initial level {}.",
                            configured_rule.rule().key(),
                            persisted_zone.id(),
                            configured_rule.level(),
                        );
                    }
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
            production_units,
            blocs_by_key,
            combats,
            stats,
        )
        .await;
    }
}
