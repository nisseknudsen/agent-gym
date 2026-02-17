//! Canonical serializable types for the Tower Defense game.
//!
//! Shared between `sim_td` (the game simulation + MCP server) and
//! `td-viewer-app` (the Bevy spectator client).

use serde::{Deserialize, Serialize};

/// Position on the map.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

/// Current wave status.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type")]
pub enum WaveStatus {
    /// Between waves, waiting for next wave to start.
    Pause {
        /// Tick when next wave starts.
        #[serde(default)]
        until_tick: u64,
        /// Size of the next wave (number of mobs).
        #[serde(default)]
        next_wave_size: u16,
    },
    /// Currently spawning mobs.
    InWave {
        /// Number of mobs spawned so far this wave.
        spawned: u16,
        /// Total mobs in this wave.
        wave_size: u16,
        /// Tick when next mob spawns.
        next_spawn_tick: u64,
    },
}

impl Default for WaveStatus {
    fn default() -> Self {
        Self::Pause {
            until_tick: 0,
            next_wave_size: 0,
        }
    }
}

/// Information about a tower.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TowerInfo {
    pub id: String,
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub tower_type: String,
    pub player_id: u8,
    pub upgrade_level: u8,
    pub damage: i32,
    pub upgrade_cost: u32,
}

/// Information about a mob.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MobInfo {
    pub x: f32,
    pub y: f32,
    pub hp: i32,
}

/// Information about a pending build.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PendingBuildInfo {
    pub x: u16,
    pub y: u16,
    pub tower_type: String,
    pub complete_tick: u64,
    pub player_id: u8,
}

/// Full game state observation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TdObservation {
    pub tick: u64,
    pub ticks_per_second: u32,

    pub map_width: u16,
    pub map_height: u16,
    pub spawn: Position,
    pub goal: Position,

    pub max_leaks: u16,
    pub tower_cost: u32,
    pub tower_range: f32,
    pub tower_damage: i32,
    pub build_time_ticks: u64,
    pub gold_per_mob_kill: u32,

    pub gold: u32,
    pub leaks: u16,

    pub current_wave: u8,
    pub waves_total: u8,
    pub wave_status: WaveStatus,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub walkable: Vec<bool>,

    pub towers: Vec<TowerInfo>,
    pub mobs: Vec<MobInfo>,
    pub build_queue: Vec<PendingBuildInfo>,
}

/// Result of observe_next (long-poll observation).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ObserveNextResult {
    /// Whether this was a timeout (returned current state rather than waiting for next decision tick).
    pub timed_out: bool,
    /// The full game state observation.
    #[serde(flatten)]
    pub observation: TdObservation,
}

/// Information about a match.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MatchInfoResult {
    pub match_id: u64,
    pub status: MatchStatusInfo,
    pub current_tick: u64,
    pub player_count: u8,
}

/// Match status.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type")]
pub enum MatchStatusInfo {
    WaitingForPlayers { current: u8, required: u8 },
    Running,
    Finished { outcome: String },
    Terminated,
}

/// Result of listing matches.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ListMatchesResult {
    pub matches: Vec<MatchInfoResult>,
}
