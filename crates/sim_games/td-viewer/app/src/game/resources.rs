//! Bevy resources for game state.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Mirrors the server's observation state.
#[derive(Resource, Default, Clone, Debug)]
pub struct GameStateCache {
    // Time
    pub tick: u64,
    pub tick_hz: u32,

    // Map info
    pub map_width: u16,
    pub map_height: u16,
    pub spawn: (u16, u16),
    pub goal: (u16, u16),

    // Game rules
    pub max_leaks: u16,
    pub tower_cost: u32,
    pub tower_range: u16,
    pub tower_damage: i32,
    pub build_time_ticks: u64,
    pub gold_per_mob_kill: u32,

    // Current resources
    pub gold: u32,
    pub leaks: u16,

    // Wave info
    pub current_wave: u8,
    pub waves_total: u8,
    pub wave_status: WaveStatus,

    // Entities
    pub towers: Vec<TowerInfo>,
    pub mobs: Vec<MobInfo>,
    pub build_queue: Vec<PendingBuildInfo>,

    /// Whether we've received at least one observation.
    pub initialized: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WaveStatus {
    #[default]
    Unknown,
    Pause {
        until_tick: u64,
        next_wave_size: u16,
    },
    InWave {
        spawned: u16,
        wave_size: u16,
        next_spawn_tick: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TowerInfo {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub player_id: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobInfo {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingBuildInfo {
    pub x: u16,
    pub y: u16,
    pub complete_tick: u64,
    pub player_id: u8,
}

/// Connection state to the MCP server.
#[derive(Resource)]
pub struct ConnectionState {
    /// Base URL of the web server (e.g., "http://localhost:8080").
    pub server_url: String,
    /// Current match ID we're observing.
    pub match_id: Option<u64>,
    /// Session token for the match.
    pub session_token: Option<u64>,
    /// Cursor for event polling.
    pub event_cursor: u64,
    /// Last time we polled for observations.
    #[allow(dead_code)]
    pub last_observe_time: f64,
    /// Last time we polled for events.
    #[allow(dead_code)]
    pub last_events_time: f64,
    /// Connection status.
    pub status: ConnectionStatus,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            match_id: None,
            session_token: None,
            event_cursor: 0,
            last_observe_time: 0.0,
            last_events_time: 0.0,
            status: ConnectionStatus::Disconnected,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Rendering configuration.
#[derive(Resource)]
pub struct RenderConfig {
    /// Size of each grid cell in pixels.
    pub cell_size: f32,
    /// Offset for centering the grid.
    pub grid_offset: Vec2,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            cell_size: 20.0,
            grid_offset: Vec2::ZERO,
        }
    }
}

impl RenderConfig {
    /// Convert grid coordinates to world position.
    pub fn grid_to_world(&self, x: u16, y: u16) -> Vec2 {
        Vec2::new(
            self.grid_offset.x + (x as f32 + 0.5) * self.cell_size,
            self.grid_offset.y + (y as f32 + 0.5) * self.cell_size,
        )
    }

    /// Calculate the grid offset to center the grid in the window.
    pub fn calculate_offset(cell_size: f32, map_width: u16, map_height: u16) -> Vec2 {
        Vec2::new(
            -(map_width as f32 * cell_size) / 2.0,
            -(map_height as f32 * cell_size) / 2.0,
        )
    }
}

/// Timers for polling the server.
#[derive(Resource)]
pub struct PollingTimers {
    pub observe_timer: Timer,
    pub events_timer: Timer,
}

impl Default for PollingTimers {
    fn default() -> Self {
        Self {
            observe_timer: Timer::from_seconds(0.1, TimerMode::Repeating), // 100ms
            events_timer: Timer::from_seconds(0.2, TimerMode::Repeating),  // 200ms
        }
    }
}

/// List of available matches from the server.
#[derive(Resource, Default)]
pub struct MatchList {
    pub matches: Vec<MatchInfo>,
    pub last_fetch_time: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchInfo {
    pub match_id: u64,
    pub status: MatchStatusInfo,
    pub current_tick: u64,
    pub player_count: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MatchStatusInfo {
    WaitingForPlayers { current: u8, required: u8 },
    Running,
    Finished { outcome: String },
    Terminated,
}

/// Current UI state.
#[derive(Resource, Default, PartialEq, Clone, Copy)]
pub enum UiState {
    #[default]
    MatchSelection,
    Spectating,
}

/// Events received from the server.
#[derive(Resource, Default)]
pub struct GameEvents {
    pub events: Vec<GameEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameEvent {
    pub sequence: u64,
    pub tick: u64,
    pub event: EventData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventData {
    TowerPlaced { x: u16, y: u16 },
    TowerDestroyed { x: u16, y: u16 },
    MobLeaked,
    MobKilled { x: u16, y: u16 },
    WaveStarted { wave: u8 },
    WaveEnded { wave: u8 },
    BuildQueued { x: u16, y: u16 },
    BuildStarted { x: u16, y: u16 },
    InsufficientGold { cost: u32, have: u32 },
}
