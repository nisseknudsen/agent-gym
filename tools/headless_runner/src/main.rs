use sim_core::ActionEnvelope;
use sim_host::MatchHost;
use sim_td::{TdAction, TdConfig, TdEvent, TdGame};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let realtime = args.iter().any(|a| a == "--realtime" || a == "-r");

    let config = TdConfig::default();

    let tick_hz = config.tick_hz;
    let tower_cost = config.tower_cost;
    let gold_start = config.gold_start;
    let gold_per_wave_base = config.gold_per_wave_base;
    let gold_per_wave_growth = config.gold_per_wave_growth;
    let build_time_ticks = config.duration_to_ticks(config.build_time);
    let inter_wave_pause_ticks = config.duration_to_ticks(config.inter_wave_pause);

    let mut host = MatchHost::<TdGame>::new(config, 12345, tick_hz);
    let player = host.join_player();

    // Towers to place: vertical wall at x=15 (from y=0 to y=31)
    // Order towers starting from y=16 (the mob path) outward for best coverage
    let mut towers: Vec<(u16, u16)> = Vec::new();
    for offset in 0..=16 {
        towers.push((15, 16 + offset)); // y=16, 17, 18, ... 31
        if offset > 0 && 16 >= offset {
            towers.push((15, 16 - offset)); // y=15, 14, 13, ... 0
        }
    }

    // Schedule tower placements over time based on gold constraints
    // Starting gold: 50, tower cost: 15 → can afford 3 towers initially
    // Each wave awards: 25 + 5*(wave-1) gold
    // Build time: 5 seconds (300 ticks at 60Hz)
    // Inter-wave pause: 30 seconds (1800 ticks)

    let mut tower_idx = 0;
    let mut tick: u64 = 1;
    let mut gold = gold_start;
    let mut wave: u32 = 0;

    // First, build towers with starting gold
    while tower_idx < towers.len() && gold >= tower_cost {
        let (x, y) = towers[tower_idx];
        host.submit(ActionEnvelope {
            player_id: player,
            action_id: tower_idx as u64,
            intended_tick: tick,
            payload: TdAction::PlaceTower { x, y, hp: 100 },
        });
        gold -= tower_cost;
        tower_idx += 1;
    }

    // Then schedule more towers as waves arrive and grant gold
    // Wave 1 starts at tick 1, then each wave after inter_wave_pause
    while tower_idx < towers.len() && wave < 10 {
        wave += 1;
        let wave_gold = gold_per_wave_base + gold_per_wave_growth * (wave - 1);
        gold += wave_gold;

        // Schedule builds at the start of each wave's pause period
        // Add build_time_ticks offset so builds happen sequentially
        let wave_start_tick = if wave == 1 {
            1
        } else {
            1 + (wave as u64 - 1) * inter_wave_pause_ticks
        };

        // Build as many towers as we can afford with current gold
        let mut builds_this_wave = 0;
        while tower_idx < towers.len() && gold >= tower_cost {
            let (x, y) = towers[tower_idx];
            // Stagger builds by build_time to allow sequential completion
            tick = wave_start_tick + builds_this_wave * build_time_ticks;
            host.submit(ActionEnvelope {
                player_id: player,
                action_id: tower_idx as u64,
                intended_tick: tick,
                payload: TdAction::PlaceTower { x, y, hp: 100 },
            });
            gold -= tower_cost;
            tower_idx += 1;
            builds_this_wave += 1;
        }
    }

    println!("Scheduled {} tower placements", tower_idx);

    if realtime {
        run_realtime(&mut host, tick_hz);
    } else {
        run_fast(&mut host);
    }
}

fn run_fast(host: &mut MatchHost<TdGame>) {
    let max_ticks = 60 * 60 * 10; // 10 minutes at 60Hz
    let result = host.run_for_ticks(max_ticks);

    println!("=== Tower Defense Simulation Complete ===");
    println!("Outcome: {:?}", result.outcome);
    println!("Final tick: {}", result.final_tick);

    let state = host.game().state();
    println!("Leaks: {}", state.leaks);
    println!("Towers remaining: {}", state.towers.len());
    println!("Mobs remaining: {}", state.mobs.len());
    println!("Current wave: {}", state.current_wave);

    print_event_summary(&result.events);
}

fn run_realtime(host: &mut MatchHost<TdGame>, tick_hz: u32) {
    let tick_duration = Duration::from_secs_f64(1.0 / tick_hz as f64);
    let mut last_status = Instant::now();
    let mut all_events = Vec::new();

    println!("=== Running in Real-Time Mode ({}Hz) ===", tick_hz);
    println!("Press Ctrl+C to stop\n");

    loop {
        let tick_start = Instant::now();

        // Step one tick
        let Some(events) = host.step_one_tick() else {
            break; // Game is terminal
        };

        // Print events as they happen
        for event in &events {
            print_event(host.current_tick(), event);
        }
        all_events.extend(events);

        // Print status every second
        if last_status.elapsed() >= Duration::from_secs(1) {
            print_status(host);
            last_status = Instant::now();
        }

        // Sleep to maintain tick rate
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            std::thread::sleep(tick_duration - elapsed);
        }
    }

    println!("\n=== Tower Defense Simulation Complete ===");
    println!("Outcome: {:?}", host.is_terminal());
    println!("Final tick: {}", host.current_tick());

    let state = host.game().state();
    println!("Leaks: {}", state.leaks);
    println!("Towers remaining: {}", state.towers.len());
    println!("Mobs remaining: {}", state.mobs.len());
    println!("Current wave: {}", state.current_wave);

    print_event_summary(&all_events);
}

fn print_event(tick: u64, event: &TdEvent) {
    match event {
        TdEvent::TowerPlaced { x, y } => println!("[{:>6}] Tower placed at ({}, {})", tick, x, y),
        TdEvent::TowerDestroyed { x, y } => {
            println!("[{:>6}] Tower DESTROYED at ({}, {})", tick, x, y)
        }
        TdEvent::MobLeaked => println!("[{:>6}] Mob leaked!", tick),
        TdEvent::MobKilled { x, y } => println!("[{:>6}] Mob killed at ({}, {})", tick, x, y),
        TdEvent::WaveStarted { wave } => println!("[{:>6}] === Wave {} started ===", tick, wave),
        TdEvent::WaveEnded { wave } => println!("[{:>6}] === Wave {} ended ===", tick, wave),
        TdEvent::BuildQueued { x, y } => println!("[{:>6}] Build queued at ({}, {})", tick, x, y),
        TdEvent::BuildStarted { x, y } => println!("[{:>6}] Build started at ({}, {})", tick, x, y),
        TdEvent::InsufficientGold { cost, have } => {
            println!("[{:>6}] Insufficient gold: need {}, have {}", tick, cost, have)
        }
    }
}

fn print_status(host: &MatchHost<TdGame>) {
    let state = host.game().state();
    let time_secs = host.current_tick() as f64 / host.tick_hz() as f64;
    println!(
        "  [{:>5.1}s] Wave {}, Mobs: {}, Towers: {}, Gold: {}, Queue: {}, Leaks: {}/{}",
        time_secs,
        state.current_wave,
        state.mobs.len(),
        state.towers.len(),
        state.gold,
        state.build_queue.queue.len(),
        state.leaks,
        state.config.max_leaks
    );
}

fn print_event_summary(events: &[TdEvent]) {
    let mut towers_placed = 0;
    let mut towers_destroyed = 0;
    let mut waves_started = 0;
    let mut waves_ended = 0;
    let mut mob_leaks = 0;
    let mut mobs_killed = 0;
    let mut builds_queued = 0;
    let mut insufficient_gold = 0;

    for event in events {
        match event {
            TdEvent::TowerPlaced { .. } => towers_placed += 1,
            TdEvent::TowerDestroyed { .. } => towers_destroyed += 1,
            TdEvent::WaveStarted { .. } => waves_started += 1,
            TdEvent::WaveEnded { .. } => waves_ended += 1,
            TdEvent::MobLeaked => mob_leaks += 1,
            TdEvent::MobKilled { .. } => mobs_killed += 1,
            TdEvent::BuildQueued { .. } => builds_queued += 1,
            TdEvent::BuildStarted { .. } => {}
            TdEvent::InsufficientGold { .. } => insufficient_gold += 1,
        }
    }

    println!("\n=== Event Summary ===");
    println!("Builds queued: {}", builds_queued);
    println!("Towers placed: {}", towers_placed);
    println!("Towers destroyed: {}", towers_destroyed);
    println!("Mobs killed: {}", mobs_killed);
    println!("Mob leak events: {}", mob_leaks);
    println!("Waves started: {}", waves_started);
    println!("Waves ended: {}", waves_ended);
    println!("Insufficient gold events: {}", insufficient_gold);
}
