use crate::config::TowerKind;
use crate::world::{TdState, WavePhase};
use sim_core::{PlayerId, Tick};

#[derive(Clone, Debug)]
pub struct ObsTower {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub kind: TowerKind,
    pub player_id: PlayerId,
}

#[derive(Clone, Debug)]
pub struct ObsMob {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
}

#[derive(Clone, Debug)]
pub struct ObsPendingBuild {
    pub x: u16,
    pub y: u16,
    pub kind: TowerKind,
    pub complete_tick: Tick,
    pub player_id: PlayerId,
}

#[derive(Clone, Debug)]
pub enum ObsWaveStatus {
    Pause {
        until_tick: Tick,
        next_wave_size: u16,
    },
    InWave {
        spawned: u16,
        wave_size: u16,
        next_spawn_tick: Tick,
    },
}

#[derive(Clone, Debug)]
pub struct TdObservation {
    pub tick: Tick,
    pub tick_hz: u32,

    pub map_width: u16,
    pub map_height: u16,
    pub spawn: (u16, u16),
    pub goal: (u16, u16),

    pub max_leaks: u16,
    pub tower_cost: u32,
    pub tower_range: u16,
    pub tower_damage: i32,
    pub build_time_ticks: u64,
    pub gold_per_mob_kill: u32,

    pub gold: u32,
    pub leaks: u16,

    pub current_wave: u8,
    pub waves_total: u8,
    pub wave_status: ObsWaveStatus,

    pub towers: Vec<ObsTower>,
    pub mobs: Vec<ObsMob>,
    pub build_queue: Vec<ObsPendingBuild>,
}

impl TdObservation {
    pub fn from_state(state: &TdState, tick: Tick) -> Self {
        let config = &state.config;
        let basic = config.spec(TowerKind::Basic);

        let wave_status = match &state.phase {
            WavePhase::Pause { until_tick } => {
                let next_wave = state.current_wave + 1;
                let next_wave_size = if next_wave <= config.waves_total {
                    config.wave_base_size + config.wave_size_growth * (next_wave as u16 - 1)
                } else {
                    0
                };
                ObsWaveStatus::Pause {
                    until_tick: *until_tick,
                    next_wave_size,
                }
            }
            WavePhase::InWave {
                spawned,
                wave_size,
                next_spawn_tick,
            } => ObsWaveStatus::InWave {
                spawned: *spawned,
                wave_size: *wave_size,
                next_spawn_tick: *next_spawn_tick,
            },
        };

        TdObservation {
            tick,
            tick_hz: config.tick_hz,

            map_width: config.width,
            map_height: config.height,
            spawn: config.spawn,
            goal: config.goal,

            max_leaks: config.max_leaks,
            tower_cost: basic.cost,
            tower_range: basic.range,
            tower_damage: basic.damage,
            build_time_ticks: config.duration_to_ticks(config.build_time),
            gold_per_mob_kill: config.gold_per_mob_kill,

            gold: state.gold,
            leaks: state.leaks,

            current_wave: state.current_wave,
            waves_total: config.waves_total,
            wave_status,

            towers: state
                .world
                .towers
                .values()
                .map(|t| ObsTower {
                    x: t.x,
                    y: t.y,
                    hp: t.hp,
                    kind: t.kind,
                    player_id: t.player_id,
                })
                .collect(),
            mobs: state
                .world
                .mobs
                .values()
                .map(|m| ObsMob {
                    x: m.x,
                    y: m.y,
                    hp: m.hp,
                })
                .collect(),
            build_queue: state
                .world
                .build_queue
                .iter()
                .map(|b| ObsPendingBuild {
                    x: b.x,
                    y: b.y,
                    kind: b.kind,
                    complete_tick: b.complete_tick,
                    player_id: b.player_id,
                })
                .collect(),
        }
    }
}
