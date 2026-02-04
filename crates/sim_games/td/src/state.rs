use sim_core::{Micros, Speed, Tick};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct TdConfig {
    pub width: u16,
    pub height: u16,
    pub spawn: (u16, u16),
    pub goal: (u16, u16),
    pub tick_hz: u32,
    pub waves_total: u8,
    pub inter_wave_pause: Micros,
    pub wave_base_size: u16,
    pub wave_size_growth: u16,
    pub spawn_interval: Micros,
    pub max_leaks: u16,

    // Economy
    pub gold_start: u32,
    pub gold_per_wave_base: u32,
    pub gold_per_wave_growth: u32,
    pub tower_cost: u32,
    pub gold_per_mob_kill: u32,

    // Build pacing
    pub build_time: Micros,

    // Tower combat
    pub tower_range: u16,
    pub tower_damage: i32,
    pub tower_fire_period: Micros,
}

impl Default for TdConfig {
    fn default() -> Self {
        Self {
            width: 32,
            height: 32,
            spawn: (0, 16),
            goal: (31, 16),
            tick_hz: 60,
            waves_total: 10,
            inter_wave_pause: Micros::from_secs(30),
            wave_base_size: 5,
            wave_size_growth: 3,
            spawn_interval: Micros::from_secs(1),
            max_leaks: 10,

            // Economy defaults
            gold_start: 50,
            gold_per_wave_base: 25,
            gold_per_wave_growth: 5,
            tower_cost: 15,
            gold_per_mob_kill: 1,

            // Build pacing
            build_time: Micros::from_secs(5),

            // Tower combat
            tower_range: 3,
            tower_damage: 5,
            tower_fire_period: Micros::from_secs(1),
        }
    }
}

impl TdConfig {
    /// Convert a duration to ticks.
    pub fn duration_to_ticks(&self, d: Micros) -> u64 {
        d.to_ticks(self.tick_hz)
    }

    /// Convert speed to ticks between moves.
    pub fn speed_to_move_interval(&self, s: Speed) -> u64 {
        s.to_tick_interval(self.tick_hz)
    }
}

#[derive(Clone, Debug)]
pub struct PendingBuild {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub complete_tick: Tick,
}

#[derive(Clone, Debug, Default)]
pub struct BuildQueue {
    pub queue: VecDeque<PendingBuild>,
}

#[derive(Clone, Debug)]
pub struct Tower {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub next_fire_tick: Tick,
}

#[derive(Clone, Debug)]
pub struct Mob {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub dmg: i32,
    pub speed: Speed,         // cells per second
    pub next_move_tick: Tick, // when this mob can move next
}

#[derive(Clone, Debug)]
pub enum WavePhase {
    InWave {
        spawned: u16,
        wave_size: u16,
        next_spawn_tick: Tick,
    },
    Pause {
        until_tick: Tick,
    },
}

#[derive(Clone, Debug)]
pub struct TdState {
    pub config: TdConfig,
    pub tick: Tick,
    pub blocked: Vec<bool>,
    pub towers: Vec<Tower>,
    pub mobs: Vec<Mob>,
    pub current_wave: u8,
    pub phase: WavePhase,
    pub leaks: u16,
    pub dist: Vec<u32>,
    pub gold: u32,
    pub build_queue: BuildQueue,
}

impl TdState {
    pub fn new(config: TdConfig) -> Self {
        let size = (config.width as usize) * (config.height as usize);
        let gold_start = config.gold_start;
        // Use inter_wave_pause for initial delay before first wave
        let initial_pause_ticks = config.duration_to_ticks(config.inter_wave_pause);
        Self {
            tick: 0,
            blocked: vec![false; size],
            towers: Vec::new(),
            mobs: Vec::new(),
            current_wave: 0,
            phase: WavePhase::Pause { until_tick: initial_pause_ticks },
            leaks: 0,
            dist: vec![u32::MAX; size],
            gold: gold_start,
            build_queue: BuildQueue::default(),
            config,
        }
    }

    #[inline]
    pub fn idx(&self, x: u16, y: u16) -> usize {
        (y as usize) * (self.config.width as usize) + (x as usize)
    }

    #[inline]
    pub fn in_bounds(&self, x: u16, y: u16) -> bool {
        x < self.config.width && y < self.config.height
    }
}
