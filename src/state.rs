//! This module contains the simulation state that is queried or mutated by users or the simulation itself.

use std::sync::Arc;

use tokio::sync::{OwnedRwLockReadGuard, RwLock, oneshot::Sender};

use crate::{
    geometry::Point,
    military::{BaseId, MilitaryUnit, MilitaryUnitCost},
    money::Payment,
};

/// Type alias for a one shot sender with an RwLockReadGuard, used to send the response for a read command.
type ReadCommand<T> = Sender<OwnedRwLockReadGuard<T>>;

/// Used to query or mutate the state of the [state_loop].
#[derive(Debug)]
pub(super) enum Command {
    CreateUnit {
        payment: Payment<MilitaryUnitCost>,
        base_id: BaseId,
        position: Point,
    },
    GetUnits(ReadCommand<Vec<MilitaryUnit>>),
}

/// Spawn a state loop and wait for [Command]s to query the state or mutate it.
///
/// Returns when the channel is closed and when there are no more messages in the channels buffer, i.e., no more
/// messages are going to be received.
pub(super) async fn state_loop(mut rx: tokio::sync::mpsc::Receiver<Command>) {
    // Load units from database
    let units = Arc::new(RwLock::new(vec![]));

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::GetUnits(resp) => {
                let units_guard = units.clone().read_owned().await;
                let _ = resp.send(units_guard);
            }
            Command::CreateUnit {
                payment,
                base_id,
                position,
            } => {
                let cost = payment.cost();
                log::debug!("creating unit... paid {cost:?}");
                let unit = MilitaryUnit::new(payment, base_id, position);
                let mut units_guard = units.write().await;
                units_guard.push(unit);
            }
        }
    }
}
