pub(crate) mod base;
pub(crate) mod bloc;
pub(crate) mod combat;
pub(crate) mod persist;
pub(crate) mod placement;
pub(crate) mod trust;
pub(crate) mod unit;
pub(crate) mod zone;

#[derive(Debug)]
pub(crate) enum CommandError {
    NotFound(&'static str),
    CreditExchange(String),
}

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex, RwLock, mpsc::Receiver, oneshot::Sender};

use crate::{
    config::Config,
    domain::{
        BaseId, Bloc, BlocName, Chance, Combat, MilitaryBase, MilitaryUnit, Placement, PlacementId, Target, Trust,
        TrustId, UnitId, Zone,
    },
    error::UserError,
    geometry::Point,
    handlers::{
        bases::{Financing, TargetBody},
        combats::CombatResponse,
        units::UnitResponse,
    },
    persistence::MongoPersistence,
    services::credit_exchange_service::Share,
};

/// Used to query or mutate the state of the [state_loop].
#[derive(Debug)]
pub(crate) enum Command {
    GetUnits(Sender<Vec<UnitResponse>>),
    GetCombats(Sender<Vec<CombatResponse>>),
    CreateBase {
        placement_id: PlacementId,
        financing: Vec<Financing>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    GetBases(Sender<Vec<MilitaryBase>>),
    GetBase(BaseId, Sender<Option<MilitaryBase>>),
    PatchBase {
        id: BaseId,
        enabled: Option<bool>,
        prioritized: Option<bool>,
        target: Option<TargetBody>,
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
    GetBlocs(Sender<Vec<Bloc>>),
    PatchBloc {
        id: BlocName,
        chance: Option<Chance>,
        military_expense: Option<Share>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Persist the current in-memory state to the database. Sent periodically by a background task.
    Persist,
    /// Publish accumulated base loot to the credit service.
    PublishBaseProduction {
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Run a military unit production cycle for all blocs.
    ProduceMilitaryUnits {
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Move all military units one step toward their closest enemy target. Sent periodically by a background task.
    MoveMilitaryUnits,
    /// On all combats, execute a [Combat::tick].
    CombatTick,
}

/// The core of the state is this loop, where it accepts commands to be read or mutated.
#[expect(clippy::too_many_arguments)]
pub(crate) async fn run(
    mut receiver: Receiver<Command>,
    config: &Config,
    persistence: &MongoPersistence,
    mut units: HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    mut bases: HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    mut trusts: HashMap<TrustId, Arc<RwLock<Trust>>>,
    blocs: HashMap<BlocName, Arc<RwLock<Bloc>>>,
    mut combats: HashMap<Point, Arc<RwLock<Combat>>>,
) {
    // We need this because combat tick and combat initiation should never happen concurrently.
    let combat_lock = Mutex::new(());

    while let Some(cmd) = receiver.recv().await {
        match cmd {
            Command::GetUnits(resp) => {
                unit::get(resp, &units, config.world_bounds()).await;
            }
            Command::GetCombats(resp) => {
                combat::get_all(resp, &combats).await;
            }
            Command::CreateBase {
                placement_id,
                financing,
                response,
            } => {
                let result = async {
                    let base = base::create(
                        placement_id,
                        financing,
                        config.credit_exchange_service(),
                        config.placements(),
                    )
                    .await
                    .map_err(|err| match err {
                        CommandError::NotFound(n) => UserError::NotFound(n),
                        CommandError::CreditExchange(err) => {
                            log::error!("credit exchange error while creating base: {err}");
                            UserError::InternalError
                        }
                    })?;
                    bases.insert(base.id(), Arc::new(RwLock::new(base)));
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::GetBases(resp) => {
                base::get_all(resp, &bases).await;
            }
            Command::GetBase(id, resp) => {
                base::get(id, resp, &bases).await;
            }
            Command::PatchBase {
                id,
                enabled,
                prioritized,
                target,
                response,
            } => {
                let result = async {
                    let resolved_target = match target {
                        None => None,
                        Some(TargetBody::None) => Some(Target::None),
                        Some(TargetBody::Base { id: base_id }) => {
                            let arc = bases.get(&base_id).ok_or(UserError::NotFound("Base"))?;
                            Some(Target::Base {
                                id: base_id,
                                base: arc.clone(),
                            })
                        }
                        Some(TargetBody::Trust { id: trust_id }) => {
                            let arc = trusts.get(&trust_id).ok_or(UserError::NotFound("Trust"))?;
                            Some(Target::Trust {
                                id: trust_id,
                                trust: arc.clone(),
                            })
                        }
                    };
                    let lock = bases.get(&id).ok_or(UserError::NotFound("Base"))?;
                    let patched = base::patch(lock.read().await.clone(), enabled, prioritized, resolved_target);
                    *lock.write().await = patched;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::CreateTrust {
                placement_id,
                financing,
                response,
            } => {
                let result = async {
                    let trust = trust::create(
                        placement_id,
                        financing,
                        config.credit_exchange_service(),
                        config.placements(),
                    )
                    .await
                    .map_err(|err| match err {
                        CommandError::NotFound(n) => UserError::NotFound(n),
                        CommandError::CreditExchange(err) => {
                            log::error!("credit exchange error while creating trust: {err}");
                            UserError::InternalError
                        }
                    })?;
                    trusts.insert(trust.id(), Arc::new(RwLock::new(trust)));
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::GetTrusts(resp) => {
                trust::get_all(resp, &trusts).await;
            }
            Command::GetTrust(id, resp) => {
                trust::get(id, resp, &trusts).await;
            }
            Command::GetPlacements(resp) => {
                placement::get(resp, config.placements());
            }
            Command::GetZones(resp) => {
                zone::get(resp, config.zones());
            }
            Command::GetBlocs(resp) => {
                bloc::get_all(resp, &blocs).await;
            }
            Command::PatchBloc {
                id,
                chance,
                military_expense,
                response,
            } => {
                let result = async {
                    let lock = blocs.get(&id).ok_or(UserError::NotFound("Bloc"))?;
                    let patched = bloc::patch(&lock.read().await.clone(), chance, military_expense);
                    *lock.write().await = patched;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::Persist => {
                persist::persist_all(persistence, &units, &bases, &trusts, &blocs, &combats).await;
            }
            Command::PublishBaseProduction { response } => {
                let result = async {
                    base::publish_production(&bases, config.credit_exchange_service())
                        .await
                        .map_err(|err| {
                            log::error!("failed to publish base production: {err:#}");
                            UserError::InternalError
                        })?;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::ProduceMilitaryUnits { response } => {
                let result = async {
                    unit::produce_units(&blocs, &bases, &mut units, config.credit_exchange_service())
                        .await
                        .map_err(|err| {
                            log::error!("failed to produce military units: {err:#}");
                            UserError::InternalError
                        })?;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::MoveMilitaryUnits => {
                let _combat_lock_guard = combat_lock.lock().await;
                combat::clear_dead_units(&mut units).await;
                unit::move_units(
                    &mut units,
                    &mut combats,
                    config.movement_step(),
                    config.world_bounds(),
                    config.base_destruction_threshold(),
                    config.trust_destruction_threshold(),
                )
                .await;
            }
            Command::CombatTick => {
                let _combat_lock_guard = combat_lock.lock().await;
                let events = combat::tick(&mut combats).await;
                combat::apply_events(&events, &mut bases, &mut trusts).await;
                combat::clear_dead_units(&mut units).await;
            }
        }
    }
}
