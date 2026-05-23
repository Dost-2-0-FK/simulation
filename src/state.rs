//! This module contains the simulation state that is queried or mutated by users or the simulation itself.

use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, mpsc::Receiver, oneshot::Sender};
use typed_builder::TypedBuilder;

use crate::{
    config::Config,
    domain::{BaseId, Bloc, Chance, MilitaryBase, MilitaryUnit, Placement, PlacementId, Target, Trust, TrustId, Zone},
    error::UserError,
    geometry::Point,
    handlers::{
        bases::Financing,
        units::UnitResponse,
    },
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

/// Used to query or mutate the state of the [state_loop].
#[derive(Debug)]
pub(crate) enum Command {
    #[expect(dead_code)]
    CreateUnit {
        base_id: BaseId,
        position: Point,
    },
    GetUnits(Sender<Vec<UnitResponse>>),
    CreateBase {
        placement_id: PlacementId,
        financing: Vec<Financing>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    GetBases(Sender<Vec<MilitaryBase>>),
    GetBase(BaseId, Sender<Option<MilitaryBase>>),
    PatchBase {
        id: BaseId,
        prioritized: Option<bool>,
        target: Option<Target>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    CreateTrust {
        placement_id: PlacementId,
        financing: Vec<Financing>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    GetTrusts(Sender<Vec<Trust>>),
    GetTrust(TrustId, Sender<Option<Trust>>),
    GetPlacements(Sender<Vec<Arc<Placement>>>),
    GetZones(Sender<Vec<Arc<Zone>>>),
    GetBlocs(Sender<Vec<Arc<Bloc>>>),
    PatchBloc {
        id: String,
        chance: Option<Chance>,
        military_expense: Option<u32>,
        response: Sender<core::result::Result<(), UserError>>,
    },
}

impl State {
    /// Spawn a state loop and wait for [Command]s to query the state or mutate it.
    ///
    /// Returns when the channel is closed and when there are no more messages in the channels buffer, i.e., no more
    /// messages are going to be received.
    pub(crate) async fn run(mut self) {
        let LoadedState {
            bases,
            trusts,
            units,
            blocs,
        } = self.loaded_state;

        let mut blocs = if blocs.is_empty() {
            log::info!("No blocs persisted, instantiating from config.");
            self.config
                .blocs()
                .map(|bloc| (*bloc).clone())
                .map(Arc::new)
                .collect()
        } else {
            log::info!("Loaded blocs from database, ignoring blocs listed in config.");
            blocs.into_iter().map(Arc::new).collect::<Vec<_>>()
        };

        let mut units = units
            .into_iter()
            .map(|unit| (unit.id().clone(), Arc::new(RwLock::new(unit))))
            .collect::<HashMap<_, _>>();
        let mut bases = bases
            .into_iter()
            .map(|base| (base.id(), Arc::new(RwLock::new(base))))
            .collect::<HashMap<_, _>>();
        let mut trusts = trusts
            .into_iter()
            .map(|trust| (trust.id(), Arc::new(RwLock::new(trust))))
            .collect::<HashMap<_, _>>();

        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                Command::GetUnits(resp) => {
                    let unit_responses = stream::iter(units.values())
                        .then(async |unit| {
                            let unit_guard = unit.read().await;
                            let base_guard = bases
                                .get(&unit_guard.base_id())
                                .expect("units always have a base")
                                .read()
                                .await;
                            let base_response = (&(*base_guard)).into();
                            UnitResponse::new(&unit_guard, Some(base_response))
                        })
                        .collect()
                        .await;
                    let _ = resp.send(unit_responses);
                }
                Command::CreateUnit { base_id, position } => {
                    let payment = self.config.payment_service().pay_for_military_unit();
                    let unit = MilitaryUnit::new(payment, base_id, position);
                    if let Err(error) = self.persistence.save_unit(&unit).await {
                        log::error!("Error persisting unit: {error:#}");
                        continue;
                    }
                    units.insert(unit.id().to_owned(), Arc::new(RwLock::new(unit)));
                }
                Command::CreateBase {
                    placement_id,
                    financing,
                    response,
                } => {
                    log::debug!("received command to create base on placement with id {placement_id:?}");
                    let payment = self.config.payment_service().pay_for_military_base(financing).await;
                    let Some(placement) = self
                        .config
                        .placements()
                        .find(|placement| placement.id() == &placement_id)
                    else {
                        let _ = response.send(Err(UserError::NotFound("Placement")));
                        continue;
                    };

                    let base = MilitaryBase::new(payment, placement.clone());
                    if let Err(error) = self.persistence.save_base(&base).await {
                        log::error!("Error persisting base: {error:#}");
                        let _ = response.send(Err(UserError::InternalError));
                        continue;
                    }
                    bases.insert(base.id(), Arc::new(RwLock::new(base)));
                    let _ = response.send(Ok(()));
                }
                Command::GetBases(resp) => {
                    let mut bases_out = Vec::with_capacity(bases.len());
                    for base in bases.values() {
                        bases_out.push(base.read().await.clone());
                    }
                    let _ = resp.send(bases_out);
                }
                Command::GetBase(id, resp) => {
                    let base = match bases.get(&id) {
                        Some(base) => Some(base.read().await.clone()),
                        None => None,
                    };
                    let _ = resp.send(base);
                }
                Command::PatchBase {
                    id,
                    prioritized,
                    target,
                    response,
                } => {
                    let updated_base = {
                        let Some(base) = bases.get(&id) else {
                            let _ = response.send(Err(UserError::NotFound("Base")));
                            continue;
                        };

                        let mut updated_base = base.write().await;
                        if let Some(prioritized) = prioritized {
                            updated_base.set_prioritized(prioritized);
                        }
                        if let Some(target) = target {
                            updated_base.set_target(target);
                        }

                        updated_base
                    };

                    if let Err(error) = self.persistence.save_base(&updated_base).await {
                        log::error!("Error persisting base: {error:#}");
                        let _ = response.send(Err(UserError::InternalError));
                        continue;
                    }

                    let _ = response.send(Ok(()));
                }
                Command::CreateTrust {
                    placement_id,
                    financing,
                    response,
                } => {
                    log::debug!("received command to create trust on placement with id {placement_id:?}");
                    let payment = self.config.payment_service().pay_for_trust(financing).await;
                    let Some(placement) = self
                        .config
                        .placements()
                        .find(|placement| placement.id() == &placement_id)
                    else {
                        let _ = response.send(Err(UserError::NotFound("Placement")));
                        continue;
                    };

                    let trust = Trust::new(payment, placement.clone());
                    if let Err(error) = self.persistence.save_trust(&trust).await {
                        log::error!("Error persisting trust: {error:#}");
                        let _ = response.send(Err(UserError::InternalError));
                        continue;
                    }

                    trusts.insert(trust.id(), Arc::new(RwLock::new(trust)));

                    let _ = response.send(Ok(()));
                }
                Command::GetTrusts(resp) => {
                    let mut result = Vec::with_capacity(trusts.len());
                    for trust in trusts.values() {
                        result.push(trust.read().await.clone());
                    }
                    let _ = resp.send(result);
                }
                Command::GetTrust(id, resp) => {
                    let trust = match trusts.get(&id) {
                        Some(trust) => Some(trust.read().await.clone()),
                        None => None,
                    };
                    let _ = resp.send(trust);
                }
                Command::GetPlacements(resp) => {
                    let _ = resp.send(self.config.placements().collect());
                }
                Command::GetZones(resp) => {
                    let _ = resp.send(self.config.zones().collect());
                }
                Command::GetBlocs(resp) => {
                    let _ = resp.send(blocs.clone());
                }
                Command::PatchBloc {
                    id,
                    chance,
                    military_expense,
                    response,
                } => {
                    let Some(idx) = blocs.iter().position(|bloc| bloc.name().to_string() == id) else {
                        let _ = response.send(Err(UserError::NotFound("Bloc")));
                        continue;
                    };

                    let current = &blocs[idx];
                    let new_chance = chance.unwrap_or_else(|| current.chance());
                    let new_military_expense = military_expense.unwrap_or_else(|| current.military_expense());
                    let new_bloc = Arc::new(Bloc::new(current.name().clone(), new_chance, new_military_expense));

                    if let Err(error) = self.persistence.save_bloc(&new_bloc).await {
                        log::error!("Error persisting bloc: {error:#}");
                        let _ = response.send(Err(UserError::InternalError));
                        continue;
                    }

                    blocs[idx] = new_bloc;
                    let _ = response.send(Ok(()));
                }
            }
        }
    }
}
