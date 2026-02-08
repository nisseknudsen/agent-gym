mod grid;
mod towers;
mod mobs;
mod effects;

pub use grid::*;
pub use towers::*;
pub use mobs::*;
pub use effects::*;

use bevy::prelude::*;

/// Plugin for all rendering systems.
pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                (
                    // Grid is spawned once when game state is initialized
                    spawn_grid.run_if(should_spawn_grid),
                    // Entity sync
                    sync_towers,
                    sync_mobs,
                    sync_pending_builds,
                    // Animation
                    interpolate_mob_positions,
                    // Effects
                    update_attack_lines,
                    update_death_particles,
                )
                    .chain(),
            );
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Condition: spawn grid only once when we have game state.
fn should_spawn_grid(
    game_state: Res<crate::game::GameStateCache>,
    query: Query<&crate::game::GridCell>,
) -> bool {
    game_state.initialized && query.is_empty()
}
