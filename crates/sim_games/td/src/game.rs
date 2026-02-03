use crate::actions::TdAction;
use crate::pathing::{compute_distance_field, move_mob, MobMoveResult};
use crate::state::{Mob, PendingBuild, TdConfig, TdState, Tower, WavePhase};
use sim_core::{ActionEnvelope, Game, PlayerId, Speed, TerminalOutcome, Tick};

#[derive(Clone, Debug)]
pub enum TdEvent {
    TowerPlaced { x: u16, y: u16 },
    TowerDestroyed { x: u16, y: u16 },
    MobLeaked,
    MobKilled { x: u16, y: u16 },
    WaveStarted { wave: u8 },
    WaveEnded { wave: u8 },
    BuildQueued { x: u16, y: u16 },
    BuildStarted { x: u16, y: u16 },
    InsufficientGold { cost: u32, have: u32 },
}

#[derive(Clone, Debug)]
pub struct TdObservation {
    pub tick: Tick,
    pub current_wave: u8,
    pub mobs_count: usize,
    pub towers_count: usize,
    pub leaks: u16,
    pub gold: u32,
    pub build_queue_size: usize,
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

        // 1. Process build actions → queue builds, deduct gold
        for action in actions {
            match &action.payload {
                TdAction::PlaceTower { x, y, hp } => {
                    self.try_queue_build(*x, *y, *hp, tick, out_events);
                }
            }
        }

        // 2. Process completed builds → place towers
        let towers_placed = self.process_build_queue(tick, out_events);

        // 3. Recompute distance field if towers were placed
        if towers_placed {
            compute_distance_field(&mut self.state);
        }

        // 4. Update wave phase (may spawn mobs, award gold on wave start)
        self.update_wave_phase(tick, out_events);

        // 5. Move mobs (mobs attack towers)
        self.move_all_mobs(tick, out_events);

        // 6. Tower attacks → towers shoot nearest mob in range
        self.tower_attacks(tick, out_events);

        // 7. Remove dead mobs
        self.remove_dead_mobs(out_events);
    }

    fn observe(&self, tick: Tick, _player: PlayerId) -> Self::Observation {
        TdObservation {
            tick,
            current_wave: self.state.current_wave,
            mobs_count: self.state.mobs.len(),
            towers_count: self.state.towers.len(),
            leaks: self.state.leaks,
            gold: self.state.gold,
            build_queue_size: self.state.build_queue.queue.len(),
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
    fn try_queue_build(
        &mut self,
        x: u16,
        y: u16,
        hp: i32,
        tick: Tick,
        out_events: &mut Vec<TdEvent>,
    ) -> bool {
        // Reject if out of bounds
        if !self.state.in_bounds(x, y) {
            return false;
        }

        let idx = self.state.idx(x, y);

        // Reject if already blocked
        if self.state.blocked[idx] {
            return false;
        }

        // Check gold
        let cost = self.state.config.tower_cost;
        if self.state.gold < cost {
            out_events.push(TdEvent::InsufficientGold {
                cost,
                have: self.state.gold,
            });
            return false;
        }

        // Deduct gold
        self.state.gold -= cost;

        // Block the cell immediately (prevents overlapping builds)
        self.state.blocked[idx] = true;

        // Calculate completion tick
        let build_ticks = self
            .state
            .config
            .duration_to_ticks(self.state.config.build_time);
        let complete_tick = tick + build_ticks;

        // Add to build queue
        self.state.build_queue.queue.push_back(PendingBuild {
            x,
            y,
            hp,
            complete_tick,
        });

        out_events.push(TdEvent::BuildQueued { x, y });
        true
    }

    fn process_build_queue(&mut self, tick: Tick, out_events: &mut Vec<TdEvent>) -> bool {
        let mut towers_placed = false;

        while let Some(build) = self.state.build_queue.queue.front() {
            if tick >= build.complete_tick {
                let build = self.state.build_queue.queue.pop_front().unwrap();
                self.state.towers.push(Tower {
                    x: build.x,
                    y: build.y,
                    hp: build.hp,
                    next_fire_tick: tick,
                });
                out_events.push(TdEvent::TowerPlaced { x: build.x, y: build.y });
                towers_placed = true;
            } else {
                break;
            }
        }

        towers_placed
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
                        speed: Speed::from_cells_per_sec(2),
                        next_move_tick: tick, // can move immediately
                    });
                    *spawned += 1;
                    *next_spawn_tick =
                        tick + self.state.config.duration_to_ticks(self.state.config.spawn_interval);
                }

                // Check if wave is complete
                if *spawned >= *wave_size && self.state.mobs.is_empty() {
                    let wave = self.state.current_wave;

                    // Award gold for completing the wave
                    let gold_award = self.state.config.gold_per_wave_base
                        + self.state.config.gold_per_wave_growth * (wave as u32 - 1);
                    self.state.gold += gold_award;

                    out_events.push(TdEvent::WaveEnded { wave });
                    self.state.phase = WavePhase::Pause {
                        until_tick: tick
                            + self
                                .state
                                .config
                                .duration_to_ticks(self.state.config.inter_wave_pause),
                    };
                }
            }
        }
    }

    fn move_all_mobs(&mut self, tick: Tick, out_events: &mut Vec<TdEvent>) {
        let mut leaked_indices = Vec::new();
        let mut attacks: Vec<(usize, usize)> = Vec::new(); // (mob_idx, tower_idx)

        // First pass: determine moves and attacks
        for i in 0..self.state.mobs.len() {
            let can_move = tick >= self.state.mobs[i].next_move_tick;
            match move_mob(&mut self.state, i, can_move) {
                MobMoveResult::Moved => {
                    // Update next move tick based on speed
                    let interval = self
                        .state
                        .config
                        .speed_to_move_interval(self.state.mobs[i].speed);
                    self.state.mobs[i].next_move_tick = tick + interval;
                }
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

    fn tower_attacks(&mut self, tick: Tick, _out_events: &mut Vec<TdEvent>) {
        let range = self.state.config.tower_range;
        let damage = self.state.config.tower_damage;
        let fire_period_ticks = self
            .state
            .config
            .duration_to_ticks(self.state.config.tower_fire_period);

        for tower in &mut self.state.towers {
            // Check if tower can fire this tick
            if tick < tower.next_fire_tick {
                continue;
            }

            // Find target: nearest mob within range, tie-break by lowest HP
            if let Some(target_idx) = Self::find_tower_target(
                tower.x,
                tower.y,
                range,
                &self.state.mobs,
            ) {
                // Deal damage
                self.state.mobs[target_idx].hp -= damage;

                // Set next fire tick
                tower.next_fire_tick = tick + fire_period_ticks;
            }
        }
    }

    fn find_tower_target(tx: u16, ty: u16, range: u16, mobs: &[Mob]) -> Option<usize> {
        let range_sq = (range as i32) * (range as i32);
        let mut best: Option<(usize, i32, i32)> = None; // (index, dist_sq, hp)

        for (i, mob) in mobs.iter().enumerate() {
            let dx = (mob.x as i32) - (tx as i32);
            let dy = (mob.y as i32) - (ty as i32);
            let dist_sq = dx * dx + dy * dy;

            if dist_sq <= range_sq {
                let dominated = match best {
                    None => false,
                    Some((_, best_dist, best_hp)) => {
                        // Prefer closer mobs, tie-break by lower HP
                        dist_sq < best_dist || (dist_sq == best_dist && mob.hp < best_hp)
                    }
                };
                if best.is_none() || dominated {
                    best = Some((i, dist_sq, mob.hp));
                }
            }
        }

        best.map(|(idx, _, _)| idx)
    }

    fn remove_dead_mobs(&mut self, out_events: &mut Vec<TdEvent>) {
        let gold_per_kill = self.state.config.gold_per_mob_kill;
        let mut i = 0;
        while i < self.state.mobs.len() {
            if self.state.mobs[i].hp <= 0 {
                let mob = self.state.mobs.remove(i);
                self.state.gold += gold_per_kill;
                out_events.push(TdEvent::MobKilled { x: mob.x, y: mob.y });
            } else {
                i += 1;
            }
        }
    }
}
