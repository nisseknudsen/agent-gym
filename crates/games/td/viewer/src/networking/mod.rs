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
        // Channel for SSE observe messages from EventSource
        let (sse_observe_tx, sse_observe_rx) = unbounded::<String>();

        // Channel for SSE match list messages from EventSource
        let (match_list_tx, match_list_rx) = unbounded::<String>();

        app.init_resource::<crate::game::ConnectionState>()
            .init_resource::<crate::game::MatchList>()
            .insert_resource(SseChannel {
                observe_tx: sse_observe_tx,
                observe_rx: sse_observe_rx,
                match_list_tx,
                match_list_rx,
            })
            .init_resource::<SseConnectionState>()
            .add_systems(Update, (
                manage_sse_connection,
                manage_match_list_sse,
                process_responses,
            ).chain());
    }
}

/// Channels for SSE data from EventSource connections.
#[derive(Resource)]
pub struct SseChannel {
    pub observe_tx: Sender<String>,
    pub observe_rx: Receiver<String>,
    pub match_list_tx: Sender<String>,
    pub match_list_rx: Receiver<String>,
}

/// Tracks the EventSource connection state.
#[derive(Resource, Default)]
pub struct SseConnectionState {
    /// The match_id we're currently streaming, if any.
    pub connected_match_id: Option<u64>,
    /// Whether we have an active EventSource for game observation.
    pub is_connected: bool,
    /// Whether we have an active EventSource for match list.
    pub match_list_connected: bool,
}
