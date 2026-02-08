use crate::match_handle::MatchHandle;
use sim_core::Game;
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};

/// Run the tick loop for a match.
/// This function runs until the match finishes or shutdown is requested.
pub async fn run_tick_loop<G: Game + Send + 'static>(handle: MatchHandle<G>)
where
    G::Action: Send,
    G::Observation: Send,
    G::Event: Send,
    G::Config: Send,
{
    let tick_hz = handle.tick_hz();
    let tick_duration = Duration::from_secs_f64(1.0 / tick_hz as f64);

    let mut interval = interval(tick_duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        if handle.should_shutdown() {
            break;
        }

        let finished = handle.step_one_tick().await;

        if finished {
            break;
        }
    }
}

/// Spawn a tick loop as a tokio task.
/// Returns a JoinHandle that can be used to wait for the loop to finish.
pub fn spawn_tick_loop<G: Game + Send + 'static>(
    handle: MatchHandle<G>,
) -> tokio::task::JoinHandle<()>
where
    G::Action: Send,
    G::Observation: Send,
    G::Event: Send,
    G::Config: Send,
{
    tokio::spawn(run_tick_loop(handle))
}
