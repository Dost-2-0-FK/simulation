//! This module contains the core of this crate, the simulation.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::{geometry::Point, military::BaseId, money::Costs, state::Command};

/// Run the simulation, allow it to send [Command]s via a channel.
pub(super) async fn simulation(tx: mpsc::Sender<Command>) {
    // This is most likely not supposed to live here.
    let payment_service = Costs {
        military_unit: Default::default(),
    };

    let position = Point::new(3.0, 4.0);
    loop {
        // Just create a military unit every second.
        tokio::time::sleep(Duration::from_secs(1)).await;
        // TODO: in the future, we'd get this id from the existing base that produces the unit. Taking a shortcut here
        // for now.
        let base_id = BaseId(0);
        let payment = payment_service.pay_for_military_unit();
        let _ = tx
            .send(Command::CreateUnit {
                payment,
                base_id,
                position,
            })
            .await;
    }
}
