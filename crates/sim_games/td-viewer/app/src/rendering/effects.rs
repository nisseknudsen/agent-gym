//! Visual effects: attack lines, death particles.

use bevy::prelude::*;
use crate::game::{AttackLine, DeathParticle, RenderConfig};

pub const COLOR_ATTACK_LINE: Color = Color::srgba(1.0, 1.0, 0.3, 0.8);
pub const COLOR_DEATH_PARTICLE: Color = Color::srgba(0.9, 0.3, 0.3, 1.0);

/// Spawn an attack line effect from tower to mob.
#[allow(dead_code)]
pub fn spawn_attack_line(
    commands: &mut Commands,
    render_config: &RenderConfig,
    tower_x: u16,
    tower_y: u16,
    mob_x: u16,
    mob_y: u16,
) {
    let start = render_config.grid_to_world(tower_x, tower_y);
    let end = render_config.grid_to_world(mob_x, mob_y);

    let midpoint = (start + end) / 2.0;
    let diff = end - start;
    let length = diff.length();
    let angle = diff.y.atan2(diff.x);

    commands.spawn((
        Sprite {
            color: COLOR_ATTACK_LINE,
            custom_size: Some(Vec2::new(length, 2.0)),
            ..default()
        },
        Transform::from_translation(midpoint.extend(3.0))
            .with_rotation(Quat::from_rotation_z(angle)),
        AttackLine { lifetime: 0.1 },
    ));
}

/// Spawn death particle effects at a location.
#[allow(dead_code)]
pub fn spawn_death_particles(
    commands: &mut Commands,
    render_config: &RenderConfig,
    x: u16,
    y: u16,
    count: usize,
) {
    let center = render_config.grid_to_world(x, y);

    for i in 0..count {
        let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
        let speed = 50.0 + (i as f32 % 3.0) * 20.0;
        let velocity = Vec2::new(angle.cos(), angle.sin()) * speed;

        commands.spawn((
            Sprite {
                color: COLOR_DEATH_PARTICLE,
                custom_size: Some(Vec2::splat(4.0)),
                ..default()
            },
            Transform::from_translation(center.extend(4.0)),
            DeathParticle {
                lifetime: 0.5,
                velocity,
            },
        ));
    }
}

/// Update and despawn attack lines.
pub fn update_attack_lines(
    mut commands: Commands,
    time: Res<Time>,
    mut lines: Query<(Entity, &mut AttackLine, &mut Sprite)>,
) {
    let dt = time.delta_secs();

    for (entity, mut line, mut sprite) in lines.iter_mut() {
        line.lifetime -= dt;

        if line.lifetime <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            // Fade out
            let alpha = (line.lifetime / 0.1).clamp(0.0, 1.0);
            sprite.color = COLOR_ATTACK_LINE.with_alpha(alpha * 0.8);
        }
    }
}

/// Update and despawn death particles.
pub fn update_death_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut DeathParticle, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();

    for (entity, mut particle, mut transform, mut sprite) in particles.iter_mut() {
        particle.lifetime -= dt;

        if particle.lifetime <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            // Move particle
            transform.translation.x += particle.velocity.x * dt;
            transform.translation.y += particle.velocity.y * dt;

            // Apply gravity
            particle.velocity.y -= 100.0 * dt;

            // Fade out
            let alpha = (particle.lifetime / 0.5).clamp(0.0, 1.0);
            sprite.color = COLOR_DEATH_PARTICLE.with_alpha(alpha);

            // Shrink
            let size = 4.0 * alpha;
            sprite.custom_size = Some(Vec2::splat(size));
        }
    }
}
