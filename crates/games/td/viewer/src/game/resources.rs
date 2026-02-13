//! Bevy resources for game state.

use bevy::prelude::*;

// Re-export canonical types from td-types.
pub use td_types::{
    MobInfo, PendingBuildInfo, TowerInfo, WaveStatus,
    MatchStatusInfo,
};
pub use td_types::MatchInfoResult as MatchInfo;

/// Mirrors the server's observation state.
#[derive(Resource, Default, Clone, Debug)]
pub struct GameStateCache {
    pub tick: u64,
    pub ticks_per_second: u32,

    pub map_width: u16,
    pub map_height: u16,
    pub spawn: (u16, u16),
    pub goal: (u16, u16),

    pub max_leaks: u16,
    pub tower_cost: u32,
    pub tower_range: u16,
    pub tower_damage: i32,
    pub build_time_ticks: u64,
    pub gold_per_mob_kill: u32,

    pub gold: u32,
    pub leaks: u16,

    pub current_wave: u8,
    pub waves_total: u8,
    pub wave_status: WaveStatus,

    pub walkable: Vec<bool>,

    pub towers: Vec<TowerInfo>,
    pub mobs: Vec<MobInfo>,
    pub build_queue: Vec<PendingBuildInfo>,

    /// Whether we've received at least one observation.
    pub initialized: bool,
}

/// Connection state to the server.
#[derive(Resource)]
pub struct ConnectionState {
    /// Base URL of the web server (e.g., "http://localhost:8080").
    pub server_url: String,
    /// Current match ID we're observing.
    pub match_id: Option<u64>,
    /// Session token (managed server-side for SSE streams).
    pub session_token: Option<u64>,
    /// Connection status.
    pub status: ConnectionStatus,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            match_id: None,
            session_token: None,
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

/// List of available matches from the server.
#[derive(Resource, Default)]
pub struct MatchList {
    pub matches: Vec<MatchInfo>,
}

/// Current UI state.
#[derive(Resource, Default, PartialEq, Clone, Copy)]
pub enum UiState {
    #[default]
    MatchSelection,
    Spectating,
}
