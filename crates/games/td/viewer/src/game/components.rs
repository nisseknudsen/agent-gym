//! ECS components for game entities.

use bevy::prelude::*;

/// Marker component for tower entities.
#[derive(Component)]
pub struct Tower {
    pub grid_x: u16,
    pub grid_y: u16,
    pub hp: i32,
    pub player_id: u8,
}

/// Marker component for mob entities.
#[derive(Component)]
pub struct Mob {
    pub grid_x: f32,
    pub grid_y: f32,
    pub hp: i32,
    /// Previous position for interpolation.
    pub prev_x: f32,
    pub prev_y: f32,
}

/// Marker component for pending build ghost entities.
#[derive(Component)]
pub struct PendingBuild {
    pub grid_x: u16,
    pub grid_y: u16,
    pub complete_tick: u64,
    pub player_id: u8,
}

/// Marker for the spawn cell.
#[derive(Component)]
pub struct SpawnMarker;

/// Marker for the goal cell.
#[derive(Component)]
pub struct GoalMarker;

/// Marker for grid cell backgrounds.
#[derive(Component)]
#[allow(dead_code)]
pub struct GridCell {
    pub x: u16,
    pub y: u16,
}

/// Marker for HP bar background.
#[derive(Component)]
#[allow(dead_code)]
pub struct HpBarBackground;

/// Marker for HP bar fill.
#[derive(Component)]
#[allow(dead_code)]
pub struct HpBarFill;

/// Marker for attack line effects.
#[derive(Component)]
pub struct AttackLine {
    pub lifetime: f32,
}

/// Marker for death particle effects.
#[derive(Component)]
pub struct DeathParticle {
    pub lifetime: f32,
    pub velocity: Vec2,
}
