//! This module contains the core of this crate, the simulation.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::state::Command;

/// Run the simulation, allow it to send [Command]s via a channel.
pub(super) async fn simulation(_tx: mpsc::Sender<Command>) {
    loop {
        // Just create a military unit every second.
        tokio::time::sleep(Duration::from_secs(1)).await;
        // TODO: in the future, we'd get this id from the existing base that produces the unit. Taking a shortcut here
        // for now.
        // let base_id = BaseId(0);
        // let _ = tx.send(Command::CreateUnit { base_id, position }).await;
    }
}
