use crate::config::TowerKind;
use crate::world::{TdState, TowerId, WavePhase};
use sim_core::Tick;
use slotmap::Key;
use td_types::{
    MobInfo, PendingBuildInfo, Position, TdObservation, TowerInfo, WaveStatus,
};

pub fn kind_to_string(kind: TowerKind) -> String {
    match kind {
        TowerKind::Basic => "Basic".to_string(),
    }
}

pub fn string_to_kind(s: &str) -> TowerKind {
    match s {
        _ => TowerKind::Basic,
    }
}

pub fn tower_id_to_string(id: TowerId) -> String {
    id.data().as_ffi().to_string()
}

pub fn string_to_tower_id(s: &str) -> Result<TowerId, String> {
    let ffi: u64 = s.parse().map_err(|_| format!("Invalid tower_id: {}", s))?;
    let key_data = slotmap::KeyData::from_ffi(ffi);
    Ok(TowerId::from(key_data))
}

pub fn build_observation(state: &TdState, tick: Tick) -> TdObservation {
    let config = &state.config;
    let player_count = config.player_count;

    let wave_status = match &state.phase {
        WavePhase::Pause { until_tick } => {
            let next_wave = state.current_wave + 1;
            let next_wave_size = if next_wave <= config.waves_total {
                config.wave_size(next_wave, player_count)
            } else {
                0
            };
            WaveStatus::Pause {
                until_tick: *until_tick,
                next_wave_size,
            }
        }
        WavePhase::InWave {
            spawned,
            wave_size,
            next_spawn_tick,
        } => WaveStatus::InWave {
            spawned: *spawned,
            wave_size: *wave_size,
            next_spawn_tick: *next_spawn_tick,
        },
    };

    let current_tower_cost = config.build_cost(state.current_wave, TowerKind::Basic);
    let current_tower_damage = config.spec(TowerKind::Basic).damage;
    let current_gold_per_kill = config.gold_per_kill(state.current_wave);

    TdObservation {
        tick,
        ticks_per_second: config.tick_hz,

        map_width: config.width,
        map_height: config.height,
        spawn: Position {
            x: config.spawn.0,
            y: config.spawn.1,
        },
        goal: Position {
            x: config.goal.0,
            y: config.goal.1,
        },

        max_leaks: config.max_leaks,
        tower_cost: current_tower_cost,
        tower_range: config.spec(TowerKind::Basic).range,
        tower_damage: current_tower_damage,
        build_time_ticks: config.duration_to_ticks(config.build_time),
        gold_per_mob_kill: current_gold_per_kill,

        gold: state.gold,
        leaks: state.leaks,

        current_wave: state.current_wave,
        waves_total: config.waves_total,
        wave_status,

        walkable: state.world.grid.walkable.clone(),

        towers: state
            .world
            .towers
            .iter()
            .map(|(id, t)| TowerInfo {
                id: tower_id_to_string(id),
                x: t.x,
                y: t.y,
                hp: t.hp,
                tower_type: kind_to_string(t.kind),
                player_id: t.player_id,
                upgrade_level: t.upgrade_level,
                damage: config.tower_damage(t.kind, t.upgrade_level),
                upgrade_cost: config.upgrade_cost(t.upgrade_level),
            })
            .collect(),
        mobs: state
            .world
            .mobs
            .values()
            .map(|m| MobInfo {
                x: m.x,
                y: m.y,
                hp: m.hp,
            })
            .collect(),
        build_queue: state
            .world
            .build_queue
            .iter()
            .map(|b| PendingBuildInfo {
                x: b.x,
                y: b.y,
                tower_type: kind_to_string(b.kind),
                complete_tick: b.complete_tick,
                player_id: b.player_id,
            })
            .collect(),
    }
}
