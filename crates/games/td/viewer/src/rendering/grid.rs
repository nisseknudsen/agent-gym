//! Grid rendering system.

use bevy::prelude::*;
use crate::game::{GameStateCache, GoalMarker, GridCell, RenderConfig, SpawnMarker};

/// Colors for different cell types.
pub const COLOR_CELL_NORMAL: Color = Color::srgba(0.15, 0.15, 0.2, 1.0);
pub const COLOR_CELL_WALL: Color = Color::srgba(0.06, 0.06, 0.08, 1.0);
pub const COLOR_CELL_SPAWN: Color = Color::srgba(0.2, 0.6, 0.2, 1.0);
pub const COLOR_CELL_GOAL: Color = Color::srgba(0.6, 0.2, 0.2, 1.0);
#[allow(dead_code)]
pub const COLOR_GRID_LINE: Color = Color::srgba(0.3, 0.3, 0.35, 1.0);

/// Spawn the grid visualization.
pub fn spawn_grid(
    mut commands: Commands,
    game_state: Res<GameStateCache>,
    mut render_config: ResMut<RenderConfig>,
) {
    let cell_size = render_config.cell_size;
    let map_width = game_state.map_width;
    let map_height = game_state.map_height;
    let spawn = game_state.spawn;
    let goal = game_state.goal;

    // Calculate offset to center the grid in the area below the HUD (60px tall)
    let mut offset = RenderConfig::calculate_offset(cell_size, map_width, map_height);
    offset.y -= 30.0; // shift down by half HUD height so grid doesn't underlap the HUD
    render_config.grid_offset = offset;

    // Spawn grid cells
    for y in 0..map_height {
        for x in 0..map_width {
            let world_pos = render_config.grid_to_world(x, y);
            let is_spawn = (x, y) == spawn;
            let is_goal = (x, y) == goal;
            let idx = (y as usize) * (map_width as usize) + (x as usize);
            let is_walkable = game_state.walkable.get(idx).copied().unwrap_or(true);

            let color = if is_spawn {
                COLOR_CELL_SPAWN
            } else if is_goal {
                COLOR_CELL_GOAL
            } else if !is_walkable {
                COLOR_CELL_WALL
            } else {
                COLOR_CELL_NORMAL
            };

            let mut entity = commands.spawn((
                Sprite {
                    color,
                    custom_size: Some(Vec2::splat(cell_size - 1.0)),
                    ..default()
                },
                Transform::from_translation(world_pos.extend(0.0)),
                GridCell { x, y },
            ));

            if is_spawn {
                entity.insert(SpawnMarker);
            }
            if is_goal {
                entity.insert(GoalMarker);
            }
        }
    }

    tracing::info!(
        "Grid spawned: {}x{}, spawn={:?}, goal={:?}",
        map_width,
        map_height,
        spawn,
        goal
    );
}
