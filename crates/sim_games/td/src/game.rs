use crate::actions::TdAction;
use crate::pathing::{compute_distance_field, move_mob, MobMoveResult};
use crate::state::{Mob, TdConfig, TdState, Tower, WavePhase};
use sim_core::{ActionEnvelope, Game, PlayerId, TerminalOutcome, Tick};

#[derive(Clone, Debug)]
pub enum TdEvent {
    TowerPlaced { x: u16, y: u16 },
    TowerDestroyed { x: u16, y: u16 },
    MobLeaked,
    WaveStarted { wave: u8 },
    WaveEnded { wave: u8 },
}

#[derive(Clone, Debug)]
pub struct TdObservation {
    pub tick: Tick,
    pub current_wave: u8,
    pub mobs_count: usize,
    pub towers_count: usize,
    pub leaks: u16,
}

pub struct TdGame {
    state: TdState,
    #[allow(dead_code)]
    seed: u64,
}

impl TdGame {
    pub fn state(&self) -> &TdState {
        &self.state
    }
}

impl Game for TdGame {
    type Config = TdConfig;
    type Action = TdAction;
    type Observation = TdObservation;
    type Event = TdEvent;

    fn new(config: Self::Config, seed: u64) -> Self {
        let mut state = TdState::new(config);
        compute_distance_field(&mut state);
        Self { state, seed }
    }

    fn step(
        &mut self,
        tick: Tick,
        actions: &[ActionEnvelope<Self::Action>],
        out_events: &mut Vec<Self::Event>,
    ) {
        self.state.tick = tick;

        // 1. Apply tower placements
        let mut towers_placed = false;
        for action in actions {
            match &action.payload {
                TdAction::PlaceTower { x, y, hp } => {
                    if self.try_place_tower(*x, *y, *hp) {
                        out_events.push(TdEvent::TowerPlaced { x: *x, y: *y });
                        towers_placed = true;
                    }
                }
            }
        }

        // 2. Recompute distance field if towers were placed
        if towers_placed {
            compute_distance_field(&mut self.state);
        }

        // 3. Update wave phase (may spawn mobs)
        self.update_wave_phase(tick, out_events);

        // 4. Move mobs (only on movement ticks, but always process attacks)
        let move_interval = self.state.config.mob_move_interval_ticks as u64;
        let is_move_tick = move_interval == 0 || tick % move_interval == 0;
        self.move_all_mobs(is_move_tick, out_events);
    }

    fn observe(&self, tick: Tick, _player: PlayerId) -> Self::Observation {
        TdObservation {
            tick,
            current_wave: self.state.current_wave,
            mobs_count: self.state.mobs.len(),
            towers_count: self.state.towers.len(),
            leaks: self.state.leaks,
        }
    }

    fn is_terminal(&self) -> Option<TerminalOutcome> {
        // Lose: leaks > max_leaks (strictly greater)
        if self.state.leaks > self.state.config.max_leaks {
            return Some(TerminalOutcome::Lose);
        }

        // Win: completed all waves and no mobs remaining
        if self.state.current_wave == self.state.config.waves_total {
            if let WavePhase::Pause { .. } = self.state.phase {
                if self.state.mobs.is_empty() {
                    return Some(TerminalOutcome::Win);
                }
            }
        }

        None
    }
}

impl TdGame {
    fn try_place_tower(&mut self, x: u16, y: u16, hp: i32) -> bool {
        // Reject if out of bounds
        if !self.state.in_bounds(x, y) {
            return false;
        }

        let idx = self.state.idx(x, y);

        // Reject if already blocked
        if self.state.blocked[idx] {
            return false;
        }

        // Place the tower
        self.state.blocked[idx] = true;
        self.state.towers.push(Tower { x, y, hp });
        true
    }

    fn update_wave_phase(&mut self, tick: Tick, out_events: &mut Vec<TdEvent>) {
        match &mut self.state.phase {
            WavePhase::Pause { until_tick } => {
                if tick >= *until_tick {
                    self.state.current_wave += 1;

                    // Check if all waves completed
                    if self.state.current_wave > self.state.config.waves_total {
                        // Stay in pause, will be detected as win when mobs clear
                        self.state.current_wave = self.state.config.waves_total;
                        return;
                    }

                    let wave_size = self.state.config.wave_base_size
                        + self.state.config.wave_size_growth * (self.state.current_wave as u16 - 1);

                    self.state.phase = WavePhase::InWave {
                        spawned: 0,
                        wave_size,
                        next_spawn_tick: tick,
                    };

                    out_events.push(TdEvent::WaveStarted {
                        wave: self.state.current_wave,
                    });
                }
            }
            WavePhase::InWave {
                spawned,
                wave_size,
                next_spawn_tick,
            } => {
                // Spawn mobs
                if tick >= *next_spawn_tick && *spawned < *wave_size {
                    let spawn = self.state.config.spawn;
                    self.state.mobs.push(Mob {
                        x: spawn.0,
                        y: spawn.1,
                        hp: 10,
                        dmg: 1,
                    });
                    *spawned += 1;
                    *next_spawn_tick = tick + self.state.config.spawn_interval_ticks as u64;
                }

                // Check if wave is complete
                if *spawned >= *wave_size && self.state.mobs.is_empty() {
                    let wave = self.state.current_wave;
                    out_events.push(TdEvent::WaveEnded { wave });
                    self.state.phase = WavePhase::Pause {
                        until_tick: tick + self.state.config.inter_wave_pause_ticks as u64,
                    };
                }
            }
        }
    }

    fn move_all_mobs(&mut self, is_move_tick: bool, out_events: &mut Vec<TdEvent>) {
        let mut leaked_indices = Vec::new();
        let mut attacks: Vec<(usize, usize)> = Vec::new(); // (mob_idx, tower_idx)

        // First pass: determine moves and attacks
        for i in 0..self.state.mobs.len() {
            match move_mob(&mut self.state, i, is_move_tick) {
                MobMoveResult::Moved => {}
                MobMoveResult::Leaked => {
                    leaked_indices.push(i);
                }
                MobMoveResult::AttackTower(Some(tower_idx)) => {
                    attacks.push((i, tower_idx));
                }
                MobMoveResult::AttackTower(None) => {}
            }
        }

        // Process attacks
        let mut destroyed_towers = Vec::new();
        for (_mob_idx, tower_idx) in attacks {
            if tower_idx < self.state.towers.len() {
                self.state.towers[tower_idx].hp -= 1;
                if self.state.towers[tower_idx].hp <= 0 {
                    destroyed_towers.push(tower_idx);
                }
            }
        }

        // Remove destroyed towers (in reverse order to maintain indices)
        destroyed_towers.sort_unstable();
        destroyed_towers.dedup();
        for &tower_idx in destroyed_towers.iter().rev() {
            let tower = &self.state.towers[tower_idx];
            let idx = self.state.idx(tower.x, tower.y);
            let (tx, ty) = (tower.x, tower.y);
            self.state.blocked[idx] = false;
            self.state.towers.remove(tower_idx);
            out_events.push(TdEvent::TowerDestroyed { x: tx, y: ty });
        }

        // Recompute distance field if towers were destroyed
        if !destroyed_towers.is_empty() {
            compute_distance_field(&mut self.state);
        }

        // Handle leaked mobs
        for &i in leaked_indices.iter().rev() {
            self.state.leaks += 1;
            self.state.mobs.remove(i);
            out_events.push(TdEvent::MobLeaked);
        }
    }
}
