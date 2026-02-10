use crate::config::TowerKind;
use crate::events::TdEvent;
use crate::pathing::{compute_distance_field, move_mob, MobMoveResult};
use crate::world::{CellState, Mob, MobId, PendingBuild, TdState, Tower, TowerId, WavePhase};
use sim_core::{PlayerId, Speed, Tick};

pub fn try_queue_build(
    state: &mut TdState,
    x: u16,
    y: u16,
    kind: TowerKind,
    tick: Tick,
    player_id: PlayerId,
    events: &mut Vec<TdEvent>,
) -> bool {
    if !state.world.grid.in_bounds(x, y) {
        return false;
    }

    let idx = state.world.grid.idx(x, y);
    if state.world.grid.is_blocked_idx(idx) {
        return false;
    }

    let cost = state.config.spec(kind).cost;
    if state.gold < cost {
        events.push(TdEvent::InsufficientGold {
            cost,
            have: state.gold,
        });
        return false;
    }

    state.gold -= cost;
    state.world.grid.set(x, y, CellState::Building);

    let build_ticks = state.config.duration_to_ticks(state.config.build_time);
    let complete_tick = tick + build_ticks;

    state.world.build_queue.push_back(PendingBuild {
        x,
        y,
        kind,
        complete_tick,
        player_id,
    });

    events.push(TdEvent::BuildQueued { x, y, kind });
    true
}

pub fn process_builds(state: &mut TdState, tick: Tick, events: &mut Vec<TdEvent>) -> bool {
    let mut towers_placed = false;

    while let Some(build) = state.world.build_queue.front() {
        if tick >= build.complete_tick {
            let build = state.world.build_queue.pop_front().unwrap();
            let spec = state.config.spec(build.kind);
            let tower = Tower {
                x: build.x,
                y: build.y,
                kind: build.kind,
                hp: spec.hp,
                max_hp: spec.hp,
                next_fire_tick: tick,
                player_id: build.player_id,
            };
            let id = state.world.towers.insert(tower);
            state.world.grid.set(build.x, build.y, CellState::Tower(id));
            events.push(TdEvent::TowerPlaced {
                id,
                x: build.x,
                y: build.y,
                kind: build.kind,
            });
            towers_placed = true;
        } else {
            break;
        }
    }

    towers_placed
}

pub fn update_wave(state: &mut TdState, tick: Tick, events: &mut Vec<TdEvent>) {
    match &mut state.phase {
        WavePhase::Pause { until_tick } => {
            if tick >= *until_tick {
                state.current_wave += 1;

                if state.current_wave > state.config.waves_total {
                    state.current_wave = state.config.waves_total;
                    return;
                }

                let wave_size = state.config.wave_base_size
                    + state.config.wave_size_growth * (state.current_wave as u16 - 1);

                state.phase = WavePhase::InWave {
                    spawned: 0,
                    wave_size,
                    next_spawn_tick: tick,
                };

                events.push(TdEvent::WaveStarted {
                    wave: state.current_wave,
                });
            }
        }
        WavePhase::InWave {
            spawned,
            wave_size,
            next_spawn_tick,
        } => {
            if tick >= *next_spawn_tick && *spawned < *wave_size {
                let spawn = state.config.spawn;
                state.world.mobs.insert(Mob {
                    x: spawn.0,
                    y: spawn.1,
                    hp: 10,
                    dmg: 1,
                    speed: Speed::from_cells_per_sec(2),
                    next_move_tick: tick,
                });
                *spawned += 1;
                *next_spawn_tick =
                    tick + state.config.duration_to_ticks(state.config.spawn_interval);
            }

            if *spawned >= *wave_size && state.world.mobs.is_empty() {
                let wave = state.current_wave;

                let gold_award = state.config.gold_per_wave_base
                    + state.config.gold_per_wave_growth * (wave as u32 - 1);
                state.gold += gold_award;

                events.push(TdEvent::WaveEnded { wave });
                state.phase = WavePhase::Pause {
                    until_tick: tick
                        + state
                            .config
                            .duration_to_ticks(state.config.inter_wave_pause),
                };
            }
        }
    }
}

pub fn move_mobs(state: &mut TdState, tick: Tick, events: &mut Vec<TdEvent>) {
    let mob_ids: Vec<MobId> = state.world.mobs.keys().collect();

    let mut leaked_ids = Vec::new();
    let mut attacks: Vec<(MobId, TowerId)> = Vec::new();

    for mob_id in mob_ids {
        let can_move = tick >= state.world.mobs[mob_id].next_move_tick;
        match move_mob(state, mob_id, can_move) {
            MobMoveResult::Moved => {
                let interval =
                    state.config.speed_to_move_interval(state.world.mobs[mob_id].speed);
                state.world.mobs[mob_id].next_move_tick = tick + interval;
            }
            MobMoveResult::Leaked => {
                leaked_ids.push(mob_id);
            }
            MobMoveResult::AttackTower(Some(tower_id)) => {
                attacks.push((mob_id, tower_id));
            }
            MobMoveResult::AttackTower(None) => {}
        }
    }

    // Process attacks
    let mut destroyed_towers: Vec<TowerId> = Vec::new();
    for (_mob_id, tower_id) in attacks {
        if let Some(tower) = state.world.towers.get_mut(tower_id) {
            tower.hp -= 1;
            if tower.hp <= 0 && !destroyed_towers.contains(&tower_id) {
                destroyed_towers.push(tower_id);
            }
        }
    }

    // Remove destroyed towers
    for &tower_id in &destroyed_towers {
        if let Some(tower) = state.world.towers.remove(tower_id) {
            state.world.grid.set(tower.x, tower.y, CellState::Empty);
            events.push(TdEvent::TowerDestroyed {
                id: tower_id,
                x: tower.x,
                y: tower.y,
            });
        }
    }

    // Recompute distance field if towers were destroyed
    if !destroyed_towers.is_empty() {
        compute_distance_field(&state.world.grid, state.config.goal, &mut state.dist);
    }

    // Handle leaked mobs
    for mob_id in leaked_ids {
        if state.world.mobs.remove(mob_id).is_some() {
            state.leaks += 1;
            events.push(TdEvent::MobLeaked { id: mob_id });
        }
    }
}

pub fn tower_attacks(state: &mut TdState, tick: Tick, _events: &mut Vec<TdEvent>) {
    // Collect tower firing info (can't iterate and mutate simultaneously)
    let tower_shots: Vec<(TowerId, u16, u16, u16, i32)> = state
        .world
        .towers
        .iter()
        .filter_map(|(id, tower)| {
            if tick < tower.next_fire_tick {
                return None;
            }
            let spec = state.config.spec(tower.kind);
            Some((id, tower.x, tower.y, spec.range, spec.damage))
        })
        .collect();

    for (tower_id, tx, ty, range, damage) in tower_shots {
        if let Some(target_id) = find_tower_target(tx, ty, range, &state.world.mobs) {
            state.world.mobs[target_id].hp -= damage;
            let fire_period = state.config.spec(state.world.towers[tower_id].kind).fire_period;
            state.world.towers[tower_id].next_fire_tick =
                tick + state.config.duration_to_ticks(fire_period);
        }
    }
}

fn find_tower_target(
    tx: u16,
    ty: u16,
    range: u16,
    mobs: &slotmap::SlotMap<MobId, Mob>,
) -> Option<MobId> {
    let range_sq = (range as i32) * (range as i32);
    let mut best: Option<(MobId, i32, i32)> = None;

    for (id, mob) in mobs.iter() {
        let dx = (mob.x as i32) - (tx as i32);
        let dy = (mob.y as i32) - (ty as i32);
        let dist_sq = dx * dx + dy * dy;

        if dist_sq <= range_sq {
            let dominated = match best {
                None => false,
                Some((_, best_dist, best_hp)) => {
                    dist_sq < best_dist || (dist_sq == best_dist && mob.hp < best_hp)
                }
            };
            if best.is_none() || dominated {
                best = Some((id, dist_sq, mob.hp));
            }
        }
    }

    best.map(|(id, _, _)| id)
}

pub fn remove_dead(state: &mut TdState, events: &mut Vec<TdEvent>) {
    let gold_per_kill = state.config.gold_per_mob_kill;
    let dead: Vec<MobId> = state
        .world
        .mobs
        .iter()
        .filter_map(|(id, mob)| if mob.hp <= 0 { Some(id) } else { None })
        .collect();

    for mob_id in dead {
        if let Some(mob) = state.world.mobs.remove(mob_id) {
            state.gold += gold_per_kill;
            events.push(TdEvent::MobKilled {
                id: mob_id,
                x: mob.x,
                y: mob.y,
            });
        }
    }
}
