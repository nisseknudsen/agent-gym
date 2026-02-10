use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for creating a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateMatchParams {
    /// Random seed for deterministic gameplay.
    pub seed: u64,
    /// Number of players required to start the match (default: 1).
    #[serde(default = "default_required_players")]
    pub required_players: u8,
    /// Number of waves (default: 10).
    #[serde(default = "default_waves")]
    pub waves: u8,
    /// Starting gold (default: 50).
    #[serde(default = "default_gold")]
    pub starting_gold: u32,
}

fn default_required_players() -> u8 {
    1
}

fn default_waves() -> u8 {
    10
}

fn default_gold() -> u32 {
    50
}

/// Result of creating a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateMatchResult {
    pub match_id: u64,
}

/// Parameters for joining a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JoinMatchParams {
    pub match_id: u64,
}

/// Result of joining a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JoinMatchResult {
    pub session_token: u64,
    pub player_id: u8,
}

/// Parameters for spectating a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpectateMatchParams {
    pub match_id: u64,
}

/// Result of spectating a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpectateMatchResult {
    pub session_token: u64,
}

/// Parameters for leaving a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LeaveMatchParams {
    pub match_id: u64,
    pub session_token: u64,
}

/// Parameters for terminating a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TerminateMatchParams {
    pub match_id: u64,
}

/// Parameters for submitting an action.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitActionParams {
    pub match_id: u64,
    pub session_token: u64,
    /// The tick at which this action should be executed.
    /// If not provided or if the tick has passed, the action executes on the next tick.
    #[serde(default)]
    pub intended_tick: Option<u64>,
    /// The action to perform.
    pub action: ActionParams,
}

/// Available actions in the TD game.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum ActionParams {
    /// Place a tower at the given coordinates.
    PlaceTower {
        x: u16,
        y: u16,
        /// Tower type. Defaults to "Basic".
        #[serde(default = "default_tower_type")]
        tower_type: String,
    },
}

fn default_tower_type() -> String {
    "Basic".to_string()
}

/// Result of submitting an action.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitActionResult {
    pub action_id: u64,
    /// The tick at which the action was actually scheduled to execute.
    pub scheduled_tick: u64,
}

/// Parameters for observing game state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObserveParams {
    pub match_id: u64,
    pub session_token: u64,
}

/// Observation of the current game state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObserveResult {
    // Time
    pub tick: u64,
    pub ticks_per_second: u32,

    // Map info
    pub map_width: u16,
    pub map_height: u16,
    pub spawn: Position,
    pub goal: Position,

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
}

/// Position on the map.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

/// Current wave status.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum WaveStatus {
    /// Between waves, waiting for next wave to start.
    Pause {
        /// Tick when next wave starts.
        until_tick: u64,
        /// Size of the next wave (number of mobs).
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

/// Information about a tower.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TowerInfo {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub tower_type: String,
    pub player_id: u8,
}

/// Information about a mob.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MobInfo {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
}

/// Information about a pending build.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PendingBuildInfo {
    pub x: u16,
    pub y: u16,
    pub tower_type: String,
    pub complete_tick: u64,
    pub player_id: u8,
}

/// Parameters for polling events.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PollEventsParams {
    pub match_id: u64,
    pub session_token: u64,
    /// Cursor position to start polling from (0 for beginning).
    #[serde(default)]
    pub cursor: u64,
}

/// Result of polling events.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PollEventsResult {
    pub events: Vec<GameEvent>,
    pub next_cursor: u64,
}

/// A game event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GameEvent {
    pub sequence: u64,
    pub tick: u64,
    pub event: EventData,
}

/// Event data types.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum EventData {
    TowerPlaced { x: u16, y: u16, tower_type: String },
    TowerDestroyed { x: u16, y: u16 },
    MobLeaked,
    MobKilled { x: u16, y: u16 },
    WaveStarted { wave: u8 },
    WaveEnded { wave: u8 },
    BuildQueued { x: u16, y: u16, tower_type: String },
    InsufficientGold { cost: u32, have: u32 },
}

/// Parameters for getting current tick.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CurrentTickParams {
    pub match_id: u64,
}

/// Result of getting current tick.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CurrentTickResult {
    pub tick: u64,
}

/// Information about a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MatchInfoResult {
    pub match_id: u64,
    pub status: MatchStatusInfo,
    pub current_tick: u64,
    pub player_count: u8,
}

/// Match status.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum MatchStatusInfo {
    WaitingForPlayers { current: u8, required: u8 },
    Running,
    Finished { outcome: String },
    Terminated,
}

/// Result of listing matches.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListMatchesResult {
    pub matches: Vec<MatchInfoResult>,
}

/// Game rules and mechanics explanation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RulesResult {
    pub game: String,
    pub objective: String,
    pub win_condition: String,
    pub lose_condition: String,
    pub map: MapRules,
    pub towers: TowerRules,
    pub mobs: MobRules,
    pub waves: WaveRules,
    pub economy: EconomyRules,
    pub actions: Vec<ActionRule>,
    pub tips: Vec<String>,
}

/// Map layout rules.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MapRules {
    pub description: String,
    pub default_size: String,
    pub spawn_description: String,
    pub goal_description: String,
}

/// Tower mechanics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TowerRules {
    pub placement: String,
    pub attack: String,
    pub destruction: String,
    pub tower_types: Vec<TowerTypeInfo>,
}

/// Info about a tower type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TowerTypeInfo {
    pub name: String,
    pub cost: u32,
    pub hp: i32,
    pub range: u16,
    pub damage: i32,
    pub description: String,
}

/// Mob mechanics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MobRules {
    pub movement: String,
    pub leaking: String,
    pub combat: String,
}

/// Wave mechanics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WaveRules {
    pub progression: String,
    pub pause_between: String,
    pub scaling: String,
}

/// Economy rules.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EconomyRules {
    pub income: String,
    pub spending: String,
}

/// Description of an available action.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionRule {
    pub name: String,
    pub description: String,
    pub parameters: String,
}
