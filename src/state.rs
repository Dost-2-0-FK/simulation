//! This module contains the simulation state that is queried or mutated by users or the simulation itself.

use std::sync::Arc;

use tokio::sync::{OwnedRwLockReadGuard, RwLock, mpsc::Receiver, oneshot::Sender};
use typed_builder::TypedBuilder;

use crate::{
    config::Config,
    domain::{BaseId, Bloc, Chance, MilitaryBase, MilitaryUnit, Placement, PlacementId, Target, Trust, TrustId, Zone},
    error::UserError,
    geometry::Point,
    handlers::{
        bases::{Financing, base_response_by_id},
        units::UnitResponse,
    },
    services::payment_service::PaymentService,
};

#[derive(TypedBuilder)]
pub(crate) struct State {
    config: Config,
    // units,
    // etc,
    receiver: Receiver<Command>,
}

/// Type alias for a one shot sender with an RwLockReadGuard, used to send the response for a read command.
type ReadCommand<T> = Sender<OwnedRwLockReadGuard<T>>;

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
    GetBases(ReadCommand<Vec<MilitaryBase>>),
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
    GetTrusts(ReadCommand<Vec<Trust>>),
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
        // Load data from database
        let units = Arc::new(RwLock::new(vec![]));
        let bases = Arc::new(RwLock::new(Vec::<MilitaryBase>::new()));
        let trusts = Arc::new(RwLock::new(Vec::<Trust>::new()));

        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                Command::GetUnits(resp) => {
                    let units_guard = units.read().await;
                    let bases_guard = bases.read().await;
                    let units = units_guard
                        .iter()
                        .map(|unit| UnitResponse::new(unit, base_response_by_id(&bases_guard, unit.base_id())))
                        .collect();
                    let _ = resp.send(units);
                }
                Command::CreateUnit { base_id, position } => {
                    let payment = self.payment_service().pay_for_military_unit();
                    let unit = MilitaryUnit::new(payment, base_id, position);
                    let mut units_guard = units.write().await;
                    units_guard.push(unit);
                }
                Command::CreateBase {
                    placement_id,
                    financing,
                    response,
                } => {
                    log::debug!("received command to create base on placement with id {placement_id:?}");
                    let payment = self.payment_service().pay_for_military_base(financing).await;
                    let Some(placement) = self.placements().find(|placement| placement.id() == &placement_id) else {
                        let _ = response.send(Err(UserError::NotFound("Placement")));
                        continue;
                    };

                    let base = MilitaryBase::new(payment, placement.clone());
                    let mut bases_guard = bases.write().await;
                    bases_guard.push(base);

                    let _ = response.send(Ok(()));
                }
                Command::GetBases(resp) => {
                    let baeses_guard = bases.clone().read_owned().await;
                    let _ = resp.send(baeses_guard);
                }
                Command::GetBase(id, resp) => {
                    let bases_guard = bases.read().await;
                    let base = bases_guard.iter().find(|base| base.id().0 == id.0).cloned();
                    let _ = resp.send(base);
                }
                Command::PatchBase {
                    id,
                    prioritized,
                    target,
                    response,
                } => {
                    let mut bases_guard = bases.write().await;
                    let Some(base) = bases_guard.iter_mut().find(|base| base.id().0 == id.0) else {
                        let _ = response.send(Err(UserError::NotFound("Base")));
                        continue;
                    };

                    if let Some(prioritized) = prioritized {
                        base.set_prioritized(prioritized);
                    }
                    if let Some(target) = target {
                        base.set_target(target);
                    }

                    let _ = response.send(Ok(()));
                }
                Command::CreateTrust {
                    placement_id,
                    financing,
                    response,
                } => {
                    log::debug!("received command to create trust on placement with id {placement_id:?}");
                    let payment = self.payment_service().pay_for_trust(financing).await;
                    let Some(placement) = self.placements().find(|placement| placement.id() == &placement_id) else {
                        let _ = response.send(Err(UserError::NotFound("Placement")));
                        continue;
                    };

                    let trust = Trust::new(payment, placement.clone());
                    let mut trusts_guard = trusts.write().await;
                    trusts_guard.push(trust);

                    let _ = response.send(Ok(()));
                }
                Command::GetTrusts(resp) => {
                    let trusts_guard = trusts.clone().read_owned().await;
                    let _ = resp.send(trusts_guard);
                }
                Command::GetTrust(id, resp) => {
                    let trusts_guard = trusts.read().await;
                    let trust = trusts_guard.iter().find(|trust| trust.id().0 == id.0).cloned();
                    let _ = resp.send(trust);
                }
                Command::GetPlacements(resp) => {
                    let _ = resp.send(self.placements().collect());
                }
                Command::GetZones(resp) => {
                    let _ = resp.send(self.zones().collect());
                }
                Command::GetBlocs(resp) => {
                    let _ = resp.send(self.blocs().collect());
                }
                Command::PatchBloc {
                    id,
                    chance,
                    military_expense,
                    response,
                } => {
                    let Some(bloc) = self.blocs().find(|bloc| bloc.name().to_string() == id) else {
                        let _ = response.send(Err(UserError::NotFound("Bloc")));
                        continue;
                    };

                    if let Some(chance) = chance {
                        bloc.set_chance(chance);
                    }
                    if let Some(military_expense) = military_expense {
                        bloc.set_military_expense(military_expense);
                    }

                    let _ = response.send(Ok(()));
                }
            }
        }
    }

    fn payment_service(&self) -> &PaymentService {
        self.config.payment_service()
    }

    fn placements(&self) -> impl Iterator<Item = Arc<Placement>> {
        self.config.placements()
    }

    fn zones(&self) -> impl Iterator<Item = Arc<Zone>> {
        self.config.zones()
    }

    fn blocs(&self) -> impl Iterator<Item = Arc<Bloc>> {
        self.config.blocs()
    }
}
