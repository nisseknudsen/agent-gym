use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Re-export canonical types from td-types so `use super::types::*` still works.
pub use td_types::{
    ListMatchesResult, MatchInfoResult, MatchStatusInfo, MobInfo, ObserveNextResult,
    PendingBuildInfo, Position, TdObservation, TowerInfo, WaveStatus,
};

/// Parameters for creating a match.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateMatchParams {
    /// Random seed for deterministic gameplay.
    pub seed: u64,
    /// Number of players required to start the match.
    pub required_players: u8,
    /// Number of waves.
    pub waves: u8,
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

/// Parameters for placing a tower.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlaceTowerParams {
    pub match_id: u64,
    pub session_token: u64,
    /// The tick at which this action should be executed. Use 0 to execute immediately.
    pub intended_tick: u64,
    /// X grid coordinate.
    pub x: u16,
    /// Y grid coordinate.
    pub y: u16,
    /// Tower type (e.g. "Basic").
    #[serde(default = "default_tower_type")]
    pub tower_type: String,
}

fn default_tower_type() -> String {
    "Basic".to_string()
}

/// Parameters for upgrading a tower.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpgradeTowerParams {
    pub match_id: u64,
    pub session_token: u64,
    /// The tick at which this action should be executed. Use 0 to execute immediately.
    pub intended_tick: u64,
    /// ID of the tower to upgrade (from observe response).
    pub tower_id: String,
}

/// Result of submitting an action.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionResult {
    pub action_id: u64,
    /// The tick at which the action was actually scheduled to execute.
    pub scheduled_tick: u64,
}

/// Parameters for observe_next (long-poll observation).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObserveNextParams {
    pub match_id: u64,
    pub session_token: u64,
    /// Last tick you observed. Returns when a tick > this is available. Use 0 for the first call.
    pub after_tick: u64,
    /// Max time to wait in milliseconds (default: 5000).
    #[serde(default = "default_max_wait")]
    pub max_wait_ms: u64,
}

fn default_max_wait() -> u64 {
    5000
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
