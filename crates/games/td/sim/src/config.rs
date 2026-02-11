use sim_core::{Micros, Speed};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TowerKind {
    Basic,
}

#[derive(Clone, Debug)]
pub struct TowerSpec {
    pub cost: u32,
    pub hp: i32,
    pub range: u16,
    pub damage: i32,
    pub fire_period: Micros,
}

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
    pub gold_per_mob_kill: u32,

    // Build pacing
    pub build_time: Micros,

    // Tower specs
    pub basic_spec: TowerSpec,
}

impl TdConfig {
    pub fn spec(&self, kind: TowerKind) -> &TowerSpec {
        match kind {
            TowerKind::Basic => &self.basic_spec,
        }
    }

    pub fn duration_to_ticks(&self, d: Micros) -> u64 {
        d.to_ticks(self.tick_hz)
    }

    pub fn speed_to_move_interval(&self, s: Speed) -> u64 {
        s.to_tick_interval(self.tick_hz)
    }
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

            gold_start: 50,
            gold_per_wave_base: 25,
            gold_per_wave_growth: 1, // 5,
            gold_per_mob_kill: 1,

            build_time: Micros::from_secs(5),

            basic_spec: TowerSpec {
                cost: 15,
                hp: 100,
                range: 3,
                damage: 3, // 5,
                fire_period: Micros::from_secs(1),
            },
        }
    }
}
