use sim_server::{EventCursor, GameServer, MatchStatus, ServerConfig};
use sim_td::{TdAction, TdConfig, TdEvent, TdGame, TdObservation};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    println!("=== Multiplayer Tower Defense Demo ===\n");

    let config = ServerConfig {
        default_tick_hz: 20, // 20 ticks per second for visible progression
        max_matches: 10,
        event_buffer_capacity: 1024,
    };

    let server = Arc::new(GameServer::<TdGame>::new(config));

    // Use a shorter game config for demo
    let game_config = TdConfig {
        tick_hz: 20,
        waves_total: 3,
        inter_wave_pause: sim_core::Micros::from_secs(10),
        wave_base_size: 3,
        wave_size_growth: 1,
        gold_start: 100, // More gold for demo
        ..TdConfig::default()
    };

    // Create a match requiring 2 players
    println!("Creating match requiring 2 players...");
    let match_id = server
        .create_match_with_players(game_config, 42, 2)
        .await
        .expect("Failed to create match");
    println!("Match {} created!\n", match_id);

    // Spawn two player tasks
    let server1 = Arc::clone(&server);
    let server2 = Arc::clone(&server);

    let player1 = tokio::spawn(run_player(
        server1,
        match_id,
        "Alice",
        vec![(14, 16), (14, 15), (14, 17), (14, 14), (14, 18)],
    ));

    let player2 = tokio::spawn(run_player(
        server2,
        match_id,
        "Bob",
        vec![(15, 16), (15, 15), (15, 17), (15, 14), (15, 18)],
    ));

    // Wait for both players to finish
    let _ = tokio::join!(player1, player2);

    // Print final match status
    println!("\n=== Final Match Status ===");
    let matches = server.list_matches().await;
    if let Some(info) = matches.iter().find(|m| m.match_id == match_id) {
        println!("Status: {:?}", info.status);
        println!("Final tick: {}", info.current_tick);
        println!("Players: {}", info.player_count);
    }

    server.shutdown().await;
    println!("\nServer shutdown complete.");
}

async fn run_player(
    server: Arc<GameServer<TdGame>>,
    match_id: u64,
    name: &'static str,
    towers: Vec<(u16, u16)>,
) {
    println!("[{}] Attempting to join match {}...", name, match_id);

    // Join the match
    let (session, player_id) = server
        .join_match(match_id)
        .await
        .expect("Failed to join match");

    println!(
        "[{}] Joined as player {} (session {})",
        name, player_id, session.0
    );

    // Wait for match to start (both players joined)
    loop {
        let matches = server.list_matches().await;
        if let Some(info) = matches.iter().find(|m| m.match_id == match_id) {
            if matches!(info.status, MatchStatus::Running) {
                println!("[{}] Match started!", name);
                break;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }

    // Play the game
    let mut cursor = EventCursor(0);
    let mut towers_placed = 0;
    let mut last_obs: Option<TdObservation> = None;

    loop {
        // Check match status
        let matches = server.list_matches().await;
        let info = matches.iter().find(|m| m.match_id == match_id);

        if let Some(info) = info {
            if matches!(
                info.status,
                MatchStatus::Finished(_) | MatchStatus::Terminated
            ) {
                println!("[{}] Match ended: {:?}", name, info.status);
                break;
            }
        } else {
            println!("[{}] Match no longer exists", name);
            break;
        }

        // Poll events
        if let Ok((events, new_cursor)) = server.poll_events(match_id, session, cursor).await {
            for event in &events {
                print_event(name, event.tick, &event.event);
            }
            cursor = new_cursor;
        }

        // Observe state
        if let Ok(obs) = server.observe(match_id, session).await {
            // Print status on wave change or significant events
            let should_print = match &last_obs {
                None => true,
                Some(prev) => {
                    prev.current_wave != obs.current_wave
                        || prev.towers.len() != obs.towers.len()
                        || prev.leaks != obs.leaks
                }
            };

            if should_print {
                println!(
                    "[{}] Tick {}: Wave {}, Towers: {}, Mobs: {}, Gold: {}, Leaks: {}",
                    name,
                    obs.tick,
                    obs.current_wave,
                    obs.towers.len(),
                    obs.mobs.len(),
                    obs.gold,
                    obs.leaks
                );
            }

            // Try to place towers when we have gold
            if towers_placed < towers.len() && obs.gold >= 15 {
                let (x, y) = towers[towers_placed];
                let intended_tick = obs.tick + 2;

                match server
                    .submit_action(
                        match_id,
                        session,
                        TdAction::PlaceTower { x, y, hp: 100 },
                        intended_tick,
                    )
                    .await
                {
                    Ok((action_id, scheduled_tick)) => {
                        println!(
                            "[{}] Queued tower at ({}, {}) - action {} @ tick {}",
                            name, x, y, action_id, scheduled_tick
                        );
                        towers_placed += 1;
                    }
                    Err(e) => {
                        println!("[{}] Failed to place tower: {:?}", name, e);
                    }
                }
            }

            last_obs = Some(obs);
        }

        sleep(Duration::from_millis(100)).await;
    }

    println!("[{}] Finished playing", name);
}

fn print_event(player: &str, tick: u64, event: &TdEvent) {
    match event {
        TdEvent::WaveStarted { wave } => {
            println!("[{}] [{:>4}] === WAVE {} STARTED ===", player, tick, wave);
        }
        TdEvent::WaveEnded { wave } => {
            println!("[{}] [{:>4}] === WAVE {} ENDED ===", player, tick, wave);
        }
        TdEvent::TowerPlaced { x, y } => {
            println!("[{}] [{:>4}] Tower placed at ({}, {})", player, tick, x, y);
        }
        TdEvent::TowerDestroyed { x, y } => {
            println!(
                "[{}] [{:>4}] TOWER DESTROYED at ({}, {})",
                player, tick, x, y
            );
        }
        TdEvent::MobLeaked => {
            println!("[{}] [{:>4}] MOB LEAKED!", player, tick);
        }
        TdEvent::MobKilled { x, y } => {
            println!("[{}] [{:>4}] Mob killed at ({}, {})", player, tick, x, y);
        }
        TdEvent::BuildQueued { x, y } => {
            println!("[{}] [{:>4}] Build queued at ({}, {})", player, tick, x, y);
        }
        TdEvent::BuildStarted { .. } => {}
        TdEvent::InsufficientGold { cost, have } => {
            println!(
                "[{}] [{:>4}] Insufficient gold: need {}, have {}",
                player, tick, cost, have
            );
        }
    }
}
