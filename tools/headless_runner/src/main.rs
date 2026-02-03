use sim_core::ActionEnvelope;
use sim_host::MatchHost;
use sim_td::{TdAction, TdConfig, TdEvent, TdGame};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let realtime = args.iter().any(|a| a == "--realtime" || a == "-r");

    let config = TdConfig {
        width: 32,
        height: 32,
        spawn: (0, 16),
        goal: (31, 16),
        tick_hz: 60,
        waves_total: 10,
        inter_wave_pause_ticks: 60 * 30, // 30 seconds
        wave_base_size: 5,
        wave_size_growth: 3,
        spawn_interval_ticks: 60,        // spawn one mob per second
        max_leaks: 10,
        mob_move_interval_ticks: 30,     // 2 cells/sec at 60Hz
    };

    let tick_hz = config.tick_hz;
    let mut host = MatchHost::<TdGame>::new(config, 12345, tick_hz);
    let player = host.join_player();

    // Place towers at tick 1 (before wave starts)
    // Full vertical wall blocking path at x=15 (from y=0 to y=31)
    let towers: Vec<(u16, u16)> = (0..32).map(|y| (15, y)).collect();

    for (i, &(x, y)) in towers.iter().enumerate() {
        host.submit(ActionEnvelope {
            player_id: player,
            action_id: i as u64,
            intended_tick: 1,
            payload: TdAction::PlaceTower { x, y, hp: 100 },
        })
        .unwrap();
    }

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
        TdEvent::WaveStarted { wave } => println!("[{:>6}] === Wave {} started ===", tick, wave),
        TdEvent::WaveEnded { wave } => println!("[{:>6}] === Wave {} ended ===", tick, wave),
    }
}

fn print_status(host: &MatchHost<TdGame>) {
    let state = host.game().state();
    let time_secs = host.current_tick() as f64 / host.tick_hz() as f64;
    println!(
        "  [{:>5.1}s] Wave {}, Mobs: {}, Towers: {}, Leaks: {}/{}",
        time_secs,
        state.current_wave,
        state.mobs.len(),
        state.towers.len(),
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

    for event in events {
        match event {
            TdEvent::TowerPlaced { .. } => towers_placed += 1,
            TdEvent::TowerDestroyed { .. } => towers_destroyed += 1,
            TdEvent::WaveStarted { .. } => waves_started += 1,
            TdEvent::WaveEnded { .. } => waves_ended += 1,
            TdEvent::MobLeaked => mob_leaks += 1,
        }
    }

    println!("\n=== Event Summary ===");
    println!("Towers placed: {}", towers_placed);
    println!("Towers destroyed: {}", towers_destroyed);
    println!("Waves started: {}", waves_started);
    println!("Waves ended: {}", waves_ended);
    println!("Mob leak events: {}", mob_leaks);
}