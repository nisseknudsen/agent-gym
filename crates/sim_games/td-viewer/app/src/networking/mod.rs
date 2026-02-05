mod client;
mod state_sync;

pub use state_sync::*;

#[allow(unused_imports)]
pub use client::*;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};

/// Plugin for networking systems.
pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        // Create channels for async responses
        let (observe_tx, observe_rx) = unbounded::<Result<ehttp::Response, String>>();
        let (events_tx, events_rx) = unbounded::<Result<ehttp::Response, String>>();
        let (match_list_tx, match_list_rx) = unbounded::<Result<ehttp::Response, String>>();
        let (join_match_tx, join_match_rx) = unbounded::<(u64, Result<ehttp::Response, String>)>();

        app.init_resource::<crate::game::ConnectionState>()
            .init_resource::<crate::game::PollingTimers>()
            .init_resource::<crate::game::MatchList>()
            .init_resource::<crate::game::GameEvents>()
            .insert_resource(ResponseChannels {
                observe_tx,
                observe_rx,
                events_tx,
                events_rx,
                match_list_tx,
                match_list_rx,
                join_match_tx,
                join_match_rx,
            })
            .init_resource::<RequestState>()
            .add_systems(Update, (
                poll_observe,
                poll_events,
                poll_match_list,
                process_responses,
            ).chain());
    }
}

/// Channels for receiving async HTTP responses.
#[derive(Resource)]
pub struct ResponseChannels {
    pub observe_tx: Sender<Result<ehttp::Response, String>>,
    pub observe_rx: Receiver<Result<ehttp::Response, String>>,
    pub events_tx: Sender<Result<ehttp::Response, String>>,
    pub events_rx: Receiver<Result<ehttp::Response, String>>,
    pub match_list_tx: Sender<Result<ehttp::Response, String>>,
    pub match_list_rx: Receiver<Result<ehttp::Response, String>>,
    pub join_match_tx: Sender<(u64, Result<ehttp::Response, String>)>,
    pub join_match_rx: Receiver<(u64, Result<ehttp::Response, String>)>,
}

/// Track in-flight request state to prevent overlapping requests.
#[derive(Resource, Default)]
pub struct RequestState {
    pub observe_pending: bool,
    pub events_pending: bool,
    pub match_list_pending: bool,
    pub join_match_pending: bool,
}
