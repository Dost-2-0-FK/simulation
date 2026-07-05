//! This module contains the core of this crate, the simulation.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::state::Command;

/// Run the simulation, allow it to send [Command]s via a channel.
pub(crate) async fn simulation(_tx: mpsc::Sender<Command>) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
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

/// Periodically send [Command::ProduceMilitaryUnits] to run the hourly military production cycle.
///
/// The task exits when the command channel is closed, which means the state loop has ended.
pub(crate) async fn periodic_produce(tx: mpsc::Sender<Command>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // skip the immediate first tick
    loop {
        ticker.tick().await;
        if tx.send(Command::ProduceMilitaryUnits).await.is_err() {
            log::warn!("Periodic production task stopping: state loop channel closed.");
            break;
        }
    }
}

/// Periodically send [Command::MoveMilitaryUnits] to move all units one step toward their closest enemy.
///
/// The task exits when the command channel is closed, which means the state loop has ended.
pub(crate) async fn periodic_move(tx: mpsc::Sender<Command>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // skip the immediate first tick
    loop {
        ticker.tick().await;
        if tx.send(Command::MoveMilitaryUnits).await.is_err() {
            log::warn!("Periodic movement task stopping: state loop channel closed.");
            break;
        }
    }
}

/// Periodically send [Command::CombatTick] to execute combat actions.
///
/// The task exits when the command channel is closed, which means the state loop has ended.
pub(crate) async fn periodic_combat_tick(tx: mpsc::Sender<Command>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // skip the immediate first tick
    loop {
        ticker.tick().await;
        if tx.send(Command::CombatTick).await.is_err() {
            log::warn!("Periodic movement task stopping: state loop channel closed.");
            break;
        }
    }
}
