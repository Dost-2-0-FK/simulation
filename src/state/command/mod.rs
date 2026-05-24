pub(crate) mod base;
pub(crate) mod bloc;
pub(crate) mod persist;
pub(crate) mod placement;
pub(crate) mod trust;
pub(crate) mod unit;
pub(crate) mod zone;

#[derive(Debug)]
pub(crate) enum CommandError {
    NotFound(&'static str),
}

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, mpsc::Receiver, oneshot::Sender};

use crate::{
    config::Config,
    domain::{
        BaseId, Bloc, BlocName, Chance, MilitaryBase, MilitaryUnit, Placement, PlacementId, Target, Trust, TrustId,
        UnitId, Zone,
    },
    error::UserError,
    handlers::{bases::Financing, units::UnitResponse},
    persistence::MongoPersistence,
    services::payment_service::Share,
};

/// Used to query or mutate the state of the [state_loop].
#[derive(Debug)]
pub(crate) enum Command {
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
        enabled: Option<bool>,
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
    GetBlocs(Sender<Vec<Bloc>>),
    PatchBloc {
        id: BlocName,
        chance: Option<Chance>,
        military_expense: Option<Share>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Persist the current in-memory state to the database. Sent periodically by a background task.
    Persist,
    /// Run one hourly military unit production cycle for all blocs. Sent periodically by a background task.
    ProduceMilitaryUnits,
    /// Move all military units one step toward their closest enemy target. Sent periodically by a background task.
    MoveMilitaryUnits,
}

/// The core of the state is this loop, where it accepts commands to be read or mutated.
pub(crate) async fn run(
    mut receiver: Receiver<Command>,
    config: &Config,
    persistence: &MongoPersistence,
    mut units: HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    mut bases: HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    mut trusts: HashMap<TrustId, Arc<RwLock<Trust>>>,
    blocs: HashMap<BlocName, Arc<RwLock<Bloc>>>,
) {
    while let Some(cmd) = receiver.recv().await {
        match cmd {
            Command::GetUnits(resp) => {
                unit::get(resp, &units).await;
            }
            Command::CreateBase {
                placement_id,
                financing,
                response,
            } => {
                let result = async {
                    let base = base::create(placement_id, financing, config.payment_service(), config.placements())
                        .await
                        .map_err(|CommandError::NotFound(n)| UserError::NotFound(n))?;
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
                    let lock = bases.get(&id).ok_or(UserError::NotFound("Base"))?;
                    let patched = base::patch(lock.read().await.clone(), enabled, prioritized, target);
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
                    let trust = trust::create(placement_id, financing, config.payment_service(), config.placements())
                        .await
                        .map_err(|CommandError::NotFound(n)| UserError::NotFound(n))?;
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
                persist::persist_all(persistence, &units, &bases, &trusts, &blocs).await;
            }
            Command::ProduceMilitaryUnits => {
                unit::produce_units(&blocs, &bases, &mut units, config.payment_service()).await;
            }
            Command::MoveMilitaryUnits => {
                unit::move_units(&mut units, &bases, &trusts, config.movement_step()).await;
            }
        }
    }
}
