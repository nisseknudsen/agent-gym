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
        // Channels for async HTTP responses (match list only now)
        let (match_list_tx, match_list_rx) = unbounded::<Result<ehttp::Response, String>>();

        // Channel for SSE observe messages from EventSource
        let (sse_observe_tx, sse_observe_rx) = unbounded::<String>();

        app.init_resource::<crate::game::ConnectionState>()
            .init_resource::<crate::game::MatchList>()
            .insert_resource(ResponseChannels {
                match_list_tx,
                match_list_rx,
            })
            .insert_resource(SseChannel {
                observe_tx: sse_observe_tx,
                observe_rx: sse_observe_rx,
            })
            .init_resource::<SseConnectionState>()
            .init_resource::<RequestState>()
            .add_systems(Update, (
                manage_sse_connection,
                poll_match_list,
                process_responses,
            ).chain());
    }
}

/// Channels for receiving async HTTP responses (match list).
#[derive(Resource)]
pub struct ResponseChannels {
    pub match_list_tx: Sender<Result<ehttp::Response, String>>,
    pub match_list_rx: Receiver<Result<ehttp::Response, String>>,
}

/// Channel for SSE observe data from EventSource.
#[derive(Resource)]
pub struct SseChannel {
    pub observe_tx: Sender<String>,
    pub observe_rx: Receiver<String>,
}

/// Tracks the EventSource connection state.
#[derive(Resource, Default)]
pub struct SseConnectionState {
    /// The match_id we're currently streaming, if any.
    pub connected_match_id: Option<u64>,
    /// Whether we have an active EventSource.
    pub is_connected: bool,
}

/// Track in-flight request state to prevent overlapping requests.
#[derive(Resource, Default)]
pub struct RequestState {
    pub match_list_pending: bool,
}
