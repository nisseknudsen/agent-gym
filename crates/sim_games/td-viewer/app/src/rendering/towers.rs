//! Tower rendering system.

use bevy::prelude::*;
use crate::game::{GameStateCache, PendingBuild, RenderConfig, Tower};

pub const COLOR_TOWER: Color = Color::srgba(0.3, 0.5, 0.9, 1.0);
pub const COLOR_TOWER_HP_BG: Color = Color::srgba(0.2, 0.2, 0.2, 1.0);
pub const COLOR_TOWER_HP_FILL: Color = Color::srgba(0.2, 0.8, 0.3, 1.0);
pub const COLOR_PENDING_BUILD: Color = Color::srgba(0.3, 0.5, 0.9, 0.4);

const TOWER_HP_MAX: i32 = 10; // Default tower HP

/// Sync tower entities with game state.
pub fn sync_towers(
    mut commands: Commands,
    game_state: Res<GameStateCache>,
    render_config: Res<RenderConfig>,
    mut towers: Query<(Entity, &mut Tower, &mut Transform, &Children)>,
    mut hp_fills: Query<&mut Sprite, Without<Tower>>,
) {
    if !game_state.initialized {
        return;
    }

    let cell_size = render_config.cell_size;
    let tower_size = cell_size * 0.8;

    // Track which towers exist in game state
    let mut existing_positions: std::collections::HashSet<(u16, u16)> = std::collections::HashSet::new();
    for tower_info in &game_state.towers {
        existing_positions.insert((tower_info.x, tower_info.y));
    }

    // Update or despawn existing tower entities
    let mut found_positions: std::collections::HashSet<(u16, u16)> = std::collections::HashSet::new();
    for (entity, mut tower, mut transform, children) in towers.iter_mut() {
        let pos = (tower.grid_x, tower.grid_y);

        if let Some(tower_info) = game_state.towers.iter().find(|t| (t.x, t.y) == pos) {
            // Tower still exists, update HP
            tower.hp = tower_info.hp;

            // Update position in case render config changed
            let world_pos = render_config.grid_to_world(tower.grid_x, tower.grid_y);
            transform.translation = world_pos.extend(1.0);

            // Update HP bar fill width
            for child in children.iter() {
                if let Ok(mut sprite) = hp_fills.get_mut(child) {
                    let hp_ratio = (tower.hp as f32 / TOWER_HP_MAX as f32).clamp(0.0, 1.0);
                    let bar_width = tower_size * hp_ratio;
                    sprite.custom_size = Some(Vec2::new(bar_width, 3.0));
                }
            }

            found_positions.insert(pos);
        } else {
            // Tower no longer exists, despawn
            commands.entity(entity).despawn();
        }
    }

    // Spawn new towers
    for tower_info in &game_state.towers {
        let pos = (tower_info.x, tower_info.y);
        if !found_positions.contains(&pos) {
            let world_pos = render_config.grid_to_world(tower_info.x, tower_info.y);
            let hp_ratio = (tower_info.hp as f32 / TOWER_HP_MAX as f32).clamp(0.0, 1.0);

            commands.spawn((
                Sprite {
                    color: COLOR_TOWER,
                    custom_size: Some(Vec2::splat(tower_size)),
                    ..default()
                },
                Transform::from_translation(world_pos.extend(1.0)),
                Tower {
                    grid_x: tower_info.x,
                    grid_y: tower_info.y,
                    hp: tower_info.hp,
                },
            )).with_children(|parent: &mut ChildSpawnerCommands| {
                // HP bar background
                parent.spawn((
                    Sprite {
                        color: COLOR_TOWER_HP_BG,
                        custom_size: Some(Vec2::new(tower_size, 3.0)),
                        ..default()
                    },
                    Transform::from_translation(Vec3::new(0.0, tower_size / 2.0 + 4.0, 0.1)),
                ));
                // HP bar fill
                parent.spawn((
                    Sprite {
                        color: COLOR_TOWER_HP_FILL,
                        custom_size: Some(Vec2::new(tower_size * hp_ratio, 3.0)),
                        ..default()
                    },
                    bevy::sprite::Anchor::CENTER_LEFT,
                    Transform::from_translation(Vec3::new(-tower_size / 2.0, tower_size / 2.0 + 4.0, 0.2)),
                ));
            });
        }
    }
}

/// Sync pending build ghost entities with game state.
pub fn sync_pending_builds(
    mut commands: Commands,
    game_state: Res<GameStateCache>,
    render_config: Res<RenderConfig>,
    mut pending_builds: Query<(Entity, &mut PendingBuild, &mut Transform, &mut Sprite)>,
) {
    if !game_state.initialized {
        return;
    }

    let cell_size = render_config.cell_size;
    let tower_size = cell_size * 0.8;

    // Track which pending builds exist
    let mut existing_positions: std::collections::HashSet<(u16, u16)> = std::collections::HashSet::new();
    for build_info in &game_state.build_queue {
        existing_positions.insert((build_info.x, build_info.y));
    }

    // Update or despawn existing pending build entities
    let mut found_positions: std::collections::HashSet<(u16, u16)> = std::collections::HashSet::new();
    for (entity, mut pending, mut transform, mut sprite) in pending_builds.iter_mut() {
        let pos = (pending.grid_x, pending.grid_y);

        if let Some(build_info) = game_state.build_queue.iter().find(|b| (b.x, b.y) == pos) {
            pending.complete_tick = build_info.complete_tick;

            // Update position
            let world_pos = render_config.grid_to_world(pending.grid_x, pending.grid_y);
            transform.translation = world_pos.extend(0.5);

            // Update opacity based on progress
            let progress = if game_state.tick >= build_info.complete_tick {
                1.0
            } else {
                let remaining = (build_info.complete_tick - game_state.tick) as f32;
                let total = game_state.build_time_ticks as f32;
                1.0 - (remaining / total).clamp(0.0, 1.0)
            };
            sprite.color = COLOR_PENDING_BUILD.with_alpha(0.3 + progress * 0.4);

            found_positions.insert(pos);
        } else {
            commands.entity(entity).despawn();
        }
    }

    // Spawn new pending builds
    for build_info in &game_state.build_queue {
        let pos = (build_info.x, build_info.y);
        if !found_positions.contains(&pos) {
            let world_pos = render_config.grid_to_world(build_info.x, build_info.y);

            commands.spawn((
                Sprite {
                    color: COLOR_PENDING_BUILD,
                    custom_size: Some(Vec2::splat(tower_size)),
                    ..default()
                },
                Transform::from_translation(world_pos.extend(0.5)),
                PendingBuild {
                    grid_x: build_info.x,
                    grid_y: build_info.y,
                    complete_tick: build_info.complete_tick,
                },
            ));
        }
    }
}
