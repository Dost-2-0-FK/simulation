//! This module contains the core of this crate, the simulation.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::state::Command;

/// Run the simulation, allow it to send [Command]s via a channel.
pub(crate) async fn simulation(_tx: mpsc::Sender<Command>) {
    loop {
        // Just create a military unit every second.
        tokio::time::sleep(Duration::from_secs(1)).await;
        // TODO: in the future, we'd get this id from the existing base that produces the unit. Taking a shortcut here
        // for now.
        // let base_id = BaseId(0);
        // let _ = tx.send(Command::CreateUnit { base_id, position }).await;
    }
}

/// Periodically send [Command::Persist] so the in-memory state is flushed to the database.
///
/// The task exits when the command channel is closed, which means the state loop has ended.
pub(crate) async fn periodic_persist(tx: mpsc::Sender<Command>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // skip the immediate first tick
    loop {
        ticker.tick().await;
        if tx.send(Command::Persist).await.is_err() {
            log::warn!("Periodic persistence task stopping: state loop channel closed.");
            break;
        }
    }
}
