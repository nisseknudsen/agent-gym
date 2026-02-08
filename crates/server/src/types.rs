use sim_core::{TerminalOutcome, Tick};

/// Identifies a player session within a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionToken(pub u64);

/// Tracks position in an event stream for cursor-based retrieval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EventCursor(pub u64);

/// Status of a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchStatus {
    WaitingForPlayers { current: u8, required: u8 },
    Running,
    Finished(TerminalOutcome),
    Terminated,
}

/// Information about a match.
#[derive(Clone, Debug)]
pub struct MatchInfo {
    pub match_id: sim_core::MatchId,
    pub status: MatchStatus,
    pub current_tick: Tick,
    pub player_count: u8,
}

/// An event from the server with sequence number for cursor tracking.
#[derive(Clone, Debug)]
pub struct ServerEvent<E> {
    pub sequence: u64,
    pub tick: Tick,
    pub event: E,
}

/// Configuration for the game server.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Default tick rate for matches (ticks per second).
    pub default_tick_hz: u32,
    /// Maximum number of concurrent matches.
    pub max_matches: usize,
    /// Capacity of the event buffer per match.
    pub event_buffer_capacity: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            default_tick_hz: 20,
            max_matches: 100,
            event_buffer_capacity: 1024,
        }
    }
}
