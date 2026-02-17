use sim_core::Micros;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TowerKind {
    Basic,
}

#[derive(Clone, Debug)]
pub struct TowerSpec {
    pub cost: u32,
    pub hp: i32,
    pub range: f32,
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
    pub spawn_interval: Micros,
    pub max_leaks: u16,

    // Build pacing
    pub build_time: Micros,

    // Tower specs
    pub basic_spec: TowerSpec,

    // Player count (set at match creation)
    pub player_count: u8,

    // Map generation
    pub maze_size: i32,
    pub dilation_base_radius: f64,
    pub dilation_amplitude: f64,
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

    /// Mob HP: `floor(10 * 1.15^w * p)`
    pub fn mob_hp(&self, wave: u8, player_count: u8) -> i32 {
        let w = wave as f64;
        let p = player_count as f64;
        (10.0 * 1.15_f64.powf(w) * p).floor() as i32
    }

    /// Wave size: `floor(8 * 1.08^w * p)`
    pub fn wave_size(&self, wave: u8, player_count: u8) -> u16 {
        let w = wave as f64;
        let p = player_count as f64;
        (8.0 * 1.08_f64.powf(w) * p).floor() as u16
    }

    /// Tower damage based on base spec damage and upgrade level: `floor(base * 1.15^level)`
    pub fn tower_damage(&self, kind: TowerKind, upgrade_level: u8) -> i32 {
        let base = self.spec(kind).damage as f64;
        let level = upgrade_level as f64;
        (base * 1.15_f64.powf(level)).floor() as i32
    }

    /// Build cost scaling with wave: `floor(base_cost * 1.12^w)`
    pub fn build_cost(&self, wave: u8, kind: TowerKind) -> u32 {
        let w = wave as f64;
        let base = self.spec(kind).cost as f64;
        (base * 1.12_f64.powf(w)).floor() as u32
    }

    /// Upgrade cost: `floor(20 * 1.20^next_level)`
    pub fn upgrade_cost(&self, current_level: u8) -> u32 {
        let next = (current_level as f64) + 1.0;
        (20.0 * 1.20_f64.powf(next)).floor() as u32
    }

    /// Starting gold: `50 + 30*(p-1)`
    pub fn gold_start(&self, player_count: u8) -> u32 {
        50 + 30 * (player_count as u32 - 1)
    }

    /// Gold per wave completion: `floor(25 * 1.12^w * p)`
    pub fn gold_per_wave(&self, wave: u8, player_count: u8) -> u32 {
        let w = wave as f64;
        let p = player_count as f64;
        (25.0 * 1.12_f64.powf(w) * p).floor() as u32
    }

    /// Gold per mob kill: `floor(1 * 1.08^w)`
    pub fn gold_per_kill(&self, wave: u8) -> u32 {
        let w = wave as f64;
        (1.0 * 1.08_f64.powf(w)).floor() as u32
    }
}

impl Default for TdConfig {
    fn default() -> Self {
        let maze_size = 10;
        let grid_size = (maze_size * 3) as u16;
        Self {
            width: grid_size,
            height: grid_size,
            spawn: (0, 0),
            goal: (grid_size - 1, grid_size - 1),
            tick_hz: 60,
            waves_total: 10,
            inter_wave_pause: Micros::from_secs(10),
            spawn_interval: Micros::from_millis(500),
            max_leaks: 10,

            build_time: Micros::from_secs(2),

            basic_spec: TowerSpec {
                cost: 15,
                hp: 100,
                range: 4.0,
                damage: 5,
                fire_period: Micros::from_secs(1),
            },

            player_count: 1,

            maze_size,
            dilation_base_radius: 3.0,
            dilation_amplitude: 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mob_hp_wave_1() {
        let config = TdConfig::default();
        // 10 * 1.15^1 * 1 = 11
        assert_eq!(config.mob_hp(1, 1), 11);
        assert_eq!(config.mob_hp(1, 4), 46);
    }

    #[test]
    fn mob_hp_wave_10() {
        let config = TdConfig::default();
        let hp = config.mob_hp(10, 1);
        // 10 * 1.15^10 ≈ 40.4
        assert_eq!(hp, 40);
    }

    #[test]
    fn mob_hp_large_wave() {
        let config = TdConfig::default();
        let hp = config.mob_hp(100, 1);
        // Should be very large: 10 * 1.15^100 ≈ 1,174,313
        assert!(hp > 1_000_000);
    }

    #[test]
    fn tower_damage_scaling() {
        let config = TdConfig::default();
        assert_eq!(config.tower_damage(TowerKind::Basic, 0), 5);
        // 5 * 1.15^10 ≈ 20.2
        let dmg_10 = config.tower_damage(TowerKind::Basic, 10);
        assert_eq!(dmg_10, 20);
    }

    #[test]
    fn upgrade_cost_scaling() {
        let config = TdConfig::default();
        // Upgrade from 0→1: 20 * 1.2^1 = 24
        assert_eq!(config.upgrade_cost(0), 24);
        // Upgrade from 1→2: 20 * 1.2^2 = 28.8 → 28
        assert_eq!(config.upgrade_cost(1), 28);
    }

    #[test]
    fn wave_size_scaling() {
        let config = TdConfig::default();
        // 8 * 1.08^1 * 1 = 8.64 → 8
        assert_eq!(config.wave_size(1, 1), 8);
        // 8 * 1.08^10 * 1 ≈ 17.2 → 17
        assert_eq!(config.wave_size(10, 1), 17);
    }

    #[test]
    fn build_cost_scaling() {
        let config = TdConfig::default();
        // 15 * 1.12^1 = 16.8 → 16
        assert_eq!(config.build_cost(1, TowerKind::Basic), 16);
        // 15 * 1.12^10 ≈ 46.6 → 46
        assert_eq!(config.build_cost(10, TowerKind::Basic), 46);
    }

    #[test]
    fn gold_start_scaling() {
        let config = TdConfig::default();
        assert_eq!(config.gold_start(1), 50);
        assert_eq!(config.gold_start(4), 140);
    }

    #[test]
    fn gold_per_wave_scaling() {
        let config = TdConfig::default();
        // 25 * 1.12^10 * 1 ≈ 77.6 → 77
        assert_eq!(config.gold_per_wave(10, 1), 77);
    }

    #[test]
    fn multiplayer_linear_scaling() {
        let config = TdConfig::default();
        // Multiplayer scales roughly linearly (floor rounding causes small deviations)
        let hp_1p = config.mob_hp(10, 1) as f64;
        let hp_4p = config.mob_hp(10, 4) as f64;
        assert!((hp_4p / hp_1p - 4.0).abs() < 0.1, "HP ratio: {}", hp_4p / hp_1p);

        let gold_1p = config.gold_per_wave(10, 1) as f64;
        let gold_4p = config.gold_per_wave(10, 4) as f64;
        assert!((gold_4p / gold_1p - 4.0).abs() < 0.1, "Gold ratio: {}", gold_4p / gold_1p);

        let size_1p = config.wave_size(10, 1) as f64;
        let size_4p = config.wave_size(10, 4) as f64;
        assert!((size_4p / size_1p - 4.0).abs() < 0.1, "Size ratio: {}", size_4p / size_1p);
    }

    #[test]
    fn gold_per_kill_scaling() {
        let config = TdConfig::default();
        // 1 * 1.08^1 = 1.08 → 1
        assert_eq!(config.gold_per_kill(1), 1);
        // 1 * 1.08^10 ≈ 2.15 → 2
        assert_eq!(config.gold_per_kill(10), 2);
    }
}
