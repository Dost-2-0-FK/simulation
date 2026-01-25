//! This module contains the simulation state that is queried or mutated by users or the simulation itself.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{OwnedRwLockReadGuard, RwLock, mpsc::Receiver, oneshot::Sender};
use typed_builder::TypedBuilder;

use crate::{
    config::Config,
    error::UserError,
    geometry::Point,
    military::{BaseId, MilitaryBase, MilitaryUnit},
    payment_service::PaymentService,
    placement::{Placement, PlacementId},
    service::PaymentInfo,
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
pub(super) enum Command {
    CreateUnit {
        base_id: BaseId,
        position: Point,
    },
    GetUnits(ReadCommand<Vec<MilitaryUnit>>),
    CreateBase {
        placement_id: PlacementId,
        payment_info: PaymentInfo,
        response: Sender<core::result::Result<(), UserError>>,
    },
}

impl State {
    /// Spawn a state loop and wait for [Command]s to query the state or mutate it.
    ///
    /// Returns when the channel is closed and when there are no more messages in the channels buffer, i.e., no more
    /// messages are going to be received.
    pub(super) async fn run(mut self) {
        // Load data from database
        let units = Arc::new(RwLock::new(vec![]));
        let bases = Arc::new(RwLock::new(Vec::<MilitaryBase>::new()));
        let placements = Arc::new(RwLock::new(HashMap::<PlacementId, Arc<Placement>>::new()));

        let payment_service = PaymentService {
            military_unit: self.config.costs().unit(),
            military_base: self.config.costs().base(),
        };

        while let Some(cmd) = self.receiver.recv().await {
            match cmd {
                Command::GetUnits(resp) => {
                    let units_guard = units.clone().read_owned().await;
                    let _ = resp.send(units_guard);
                }
                Command::CreateUnit { base_id, position } => {
                    let payment = payment_service.pay_for_military_unit();
                    let unit = MilitaryUnit::new(payment, base_id, position);
                    let mut units_guard = units.write().await;
                    units_guard.push(unit);
                }
                Command::CreateBase {
                    placement_id,
                    payment_info,
                    response,
                } => {
                    let payment = payment_service.pay_for_militray_base(&payment_info).await;
                    let placements = placements.read().await;
                    let Some(placement) = placements.get(&placement_id) else {
                        let _ = response.send(Err(UserError::NotFound));
                        continue;
                    };

                    let base = MilitaryBase::new(payment, placement.clone());
                    let mut bases_guard = bases.write().await;
                    bases_guard.push(base);

                    let _ = response.send(Ok(()));
                }
            }
        }
    }
}
