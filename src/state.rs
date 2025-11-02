//! This module contains the simulation state that is queried or mutated by users or the simulation itself.

/// Used to query or mutate the state of the [state_loop].
#[derive(Debug)]
pub(super) enum Command {
    Increment,
    GetCount(tokio::sync::oneshot::Sender<u32>),
}

/// Spawn a state loop and wait for [Command]s to query the state or mutate it.
///
/// Returns when the channel is closed and when there are no more messages in the channels buffer, i.e., no more
/// messages are going to be received.
pub(super) async fn state_loop(mut rx: tokio::sync::mpsc::Receiver<Command>) {
    let mut count_value = 0;
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::GetCount(resp) => {
                let _ = resp.send(count_value);
            }
            Command::Increment => count_value += 1,
        }
    }
}
