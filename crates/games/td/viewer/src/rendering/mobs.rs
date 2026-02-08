//! Mob rendering system.

use bevy::prelude::*;
use crate::game::{GameStateCache, Mob, RenderConfig};

pub const COLOR_MOB: Color = Color::srgba(0.9, 0.3, 0.3, 1.0);
pub const COLOR_MOB_HP_BG: Color = Color::srgba(0.2, 0.2, 0.2, 1.0);
pub const COLOR_MOB_HP_FILL: Color = Color::srgba(0.8, 0.8, 0.2, 1.0);

const MOB_HP_MAX: i32 = 10; // Default mob HP

/// Sync mob entities with game state.
pub fn sync_mobs(
    mut commands: Commands,
    game_state: Res<GameStateCache>,
    render_config: Res<RenderConfig>,
    mut mobs: Query<(Entity, &mut Mob, &mut Transform, &Children)>,
    mut hp_fills: Query<&mut Sprite, Without<Mob>>,
) {
    if !game_state.initialized {
        return;
    }

    let cell_size = render_config.cell_size;
    let mob_size = cell_size * 0.6;

    // We need to match mobs by position since there's no ID
    // This is imperfect but works for visualization
    let server_mobs: Vec<_> = game_state.mobs.iter().collect();

    // Count mobs at each position in the server state
    let mut server_mob_counts: std::collections::HashMap<(u16, u16), Vec<&crate::game::MobInfo>> = std::collections::HashMap::new();
    for mob_info in &server_mobs {
        server_mob_counts
            .entry((mob_info.x, mob_info.y))
            .or_default()
            .push(mob_info);
    }

    // Track which server mobs we've matched
    let mut matched_server_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut entities_to_despawn: Vec<Entity> = Vec::new();

    // Try to match existing mob entities to server mobs
    for (entity, mut mob, mut transform, children) in mobs.iter_mut() {
        // Find the best matching server mob (closest to current position)
        let mut best_match: Option<(usize, f32)> = None;

        for (i, mob_info) in server_mobs.iter().enumerate() {
            if matched_server_indices.contains(&i) {
                continue;
            }

            let dx = mob_info.x as f32 - mob.grid_x as f32;
            let dy = mob_info.y as f32 - mob.grid_y as f32;
            let dist = dx * dx + dy * dy;

            // Allow matching if close (within 2 cells)
            if dist <= 4.0 {
                if best_match.is_none() || dist < best_match.unwrap().1 {
                    best_match = Some((i, dist));
                }
            }
        }

        if let Some((idx, _)) = best_match {
            let mob_info = server_mobs[idx];
            matched_server_indices.insert(idx);

            // Store previous position for interpolation
            mob.prev_x = transform.translation.x;
            mob.prev_y = transform.translation.y;
            mob.grid_x = mob_info.x;
            mob.grid_y = mob_info.y;
            mob.hp = mob_info.hp;

            // Target position (will be interpolated)
            let target_pos = render_config.grid_to_world(mob_info.x, mob_info.y);
            transform.translation = target_pos.extend(2.0);

            // Update HP bar
            for child in children.iter() {
                if let Ok(mut sprite) = hp_fills.get_mut(child) {
                    let hp_ratio = (mob.hp as f32 / MOB_HP_MAX as f32).clamp(0.0, 1.0);
                    sprite.custom_size = Some(Vec2::new(mob_size * hp_ratio, 2.0));
                }
            }
        } else {
            // No match found, despawn
            entities_to_despawn.push(entity);
        }
    }

    // Despawn unmatched entities
    for entity in entities_to_despawn {
        commands.entity(entity).despawn();
    }

    // Spawn new mobs
    for (i, mob_info) in server_mobs.iter().enumerate() {
        if matched_server_indices.contains(&i) {
            continue;
        }

        let world_pos = render_config.grid_to_world(mob_info.x, mob_info.y);
        let hp_ratio = (mob_info.hp as f32 / MOB_HP_MAX as f32).clamp(0.0, 1.0);

        commands.spawn((
            Sprite {
                color: COLOR_MOB,
                custom_size: Some(Vec2::splat(mob_size)),
                ..default()
            },
            Transform::from_translation(world_pos.extend(2.0)),
            Mob {
                grid_x: mob_info.x,
                grid_y: mob_info.y,
                hp: mob_info.hp,
                prev_x: world_pos.x,
                prev_y: world_pos.y,
            },
        )).with_children(|parent: &mut ChildSpawnerCommands| {
            // HP bar background
            parent.spawn((
                Sprite {
                    color: COLOR_MOB_HP_BG,
                    custom_size: Some(Vec2::new(mob_size, 2.0)),
                    ..default()
                },
                Transform::from_translation(Vec3::new(0.0, mob_size / 2.0 + 3.0, 0.1)),
            ));
            // HP bar fill
            parent.spawn((
                Sprite {
                    color: COLOR_MOB_HP_FILL,
                    custom_size: Some(Vec2::new(mob_size * hp_ratio, 2.0)),
                    ..default()
                },
                bevy::sprite::Anchor::CENTER_LEFT,
                Transform::from_translation(Vec3::new(-mob_size / 2.0, mob_size / 2.0 + 3.0, 0.2)),
            ));
        });
    }
}

/// Interpolate mob positions for smooth animation.
pub fn interpolate_mob_positions(
    time: Res<Time>,
    mut mobs: Query<(&Mob, &mut Transform)>,
    render_config: Res<RenderConfig>,
) {
    let dt = time.delta_secs();
    let lerp_speed = 10.0; // How quickly to interpolate

    for (mob, mut transform) in mobs.iter_mut() {
        let target_pos = render_config.grid_to_world(mob.grid_x, mob.grid_y);
        let current_pos = Vec2::new(transform.translation.x, transform.translation.y);

        // Lerp towards target position
        let new_pos = current_pos.lerp(target_pos, (lerp_speed * dt).min(1.0));
        transform.translation.x = new_pos.x;
        transform.translation.y = new_pos.y;
    }
}
