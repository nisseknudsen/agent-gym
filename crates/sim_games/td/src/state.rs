use sim_core::Tick;

#[derive(Clone, Debug)]
pub struct TdConfig {
    pub width: u16,
    pub height: u16,
    pub spawn: (u16, u16),
    pub goal: (u16, u16),
    pub tick_hz: u32,
    pub waves_total: u8,
    pub inter_wave_pause_ticks: u32,
    pub wave_base_size: u16,
    pub wave_size_growth: u16,
    pub spawn_interval_ticks: u32,
    pub max_leaks: u16,
    pub mob_move_interval_ticks: u32, // ticks between mob movements (e.g., 30 at 60Hz = 2 cells/sec)
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
            inter_wave_pause_ticks: 60 * 30,
            wave_base_size: 5,
            wave_size_growth: 3,
            spawn_interval_ticks: 10,
            max_leaks: 10,
            mob_move_interval_ticks: 30, // 2 cells/sec at 60Hz
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tower {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
}

#[derive(Clone, Debug)]
pub struct Mob {
    pub x: u16,
    pub y: u16,
    pub hp: i32,
    pub dmg: i32,
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
}

impl TdState {
    pub fn new(config: TdConfig) -> Self {
        let size = (config.width as usize) * (config.height as usize);
        Self {
            tick: 0,
            blocked: vec![false; size],
            towers: Vec::new(),
            mobs: Vec::new(),
            current_wave: 0,
            phase: WavePhase::Pause { until_tick: 1 },
            leaks: 0,
            dist: vec![u32::MAX; size],
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
