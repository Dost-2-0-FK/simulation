//! This module contains the core of this crate, the simulation.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::state::Command;

/// Run the simulation, allow it to send [Command]s via a channel.
pub(super) async fn simulation(tx: mpsc::Sender<Command>) {
    loop {
        // Just send an increment command every second.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let _ = tx.send(Command::Increment).await;
    }
}
