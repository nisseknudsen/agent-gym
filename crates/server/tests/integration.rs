use sim_core::{ActionEnvelope, Game, PlayerId, TerminalOutcome, Tick};
use sim_server::{EventCursor, GameServer, MatchStatus, ServerConfig};
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// A simple counter game for testing.
/// Each tick, the counter increments. Win when counter reaches target.
#[derive(Clone)]
struct CounterGame {
    counter: u64,
    target: u64,
    events: Vec<CounterEvent>,
}

#[derive(Clone, Debug)]
struct CounterConfig {
    target: u64,
}

#[derive(Clone, Debug)]
enum CounterAction {
    Increment(u64),
}

#[derive(Clone, Debug)]
struct CounterObservation {
    counter: u64,
    target: u64,
}

#[derive(Clone, Debug)]
enum CounterEvent {
    Incremented { amount: u64, new_value: u64 },
    TickAdvanced { tick: Tick },
}

impl Game for CounterGame {
    type Config = CounterConfig;
    type Action = CounterAction;
    type Observation = CounterObservation;
    type Event = CounterEvent;

    fn new(config: Self::Config, _seed: u64) -> Self {
        Self {
            counter: 0,
            target: config.target,
            events: Vec::new(),
        }
    }

    fn step(
        &mut self,
        tick: Tick,
        actions: &[ActionEnvelope<Self::Action>],
        out_events: &mut Vec<Self::Event>,
    ) {
        for action in actions {
            match &action.payload {
                CounterAction::Increment(amount) => {
                    self.counter += amount;
                    out_events.push(CounterEvent::Incremented {
                        amount: *amount,
                        new_value: self.counter,
                    });
                }
            }
        }
        out_events.push(CounterEvent::TickAdvanced { tick });
    }

    fn observe(&self, _tick: Tick, _player: PlayerId) -> Self::Observation {
        CounterObservation {
            counter: self.counter,
            target: self.target,
        }
    }

    fn is_terminal(&self) -> Option<TerminalOutcome> {
        if self.counter >= self.target {
            Some(TerminalOutcome::Win)
        } else {
            None
        }
    }
}

#[tokio::test]
async fn test_create_and_list_matches() {
    let config = ServerConfig {
        default_tick_hz: 100, // Fast for testing
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    // Create a match
    let match_id = server
        .create_match(CounterConfig { target: 1000 }, 42)
        .await
        .unwrap();

    // List matches
    let matches = server.list_matches().await;
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].match_id, match_id);

    server.shutdown().await;
}

#[tokio::test]
async fn test_join_and_observe() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    let match_id = server
        .create_match(CounterConfig { target: 1000 }, 42)
        .await
        .unwrap();

    // Join the match
    let (session, player_id) = server.join_match(match_id).await.unwrap();
    assert_eq!(player_id, 0);

    // Wait a bit for ticks to advance
    sleep(Duration::from_millis(50)).await;

    // Observe
    let obs = server.observe(match_id, session).await.unwrap();
    assert_eq!(obs.target, 1000);

    server.shutdown().await;
}

#[tokio::test]
async fn test_submit_action_and_poll_events() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    let match_id = server
        .create_match(CounterConfig { target: 1000 }, 42)
        .await
        .unwrap();

    let (session, _player_id) = server.join_match(match_id).await.unwrap();

    // Get current tick and submit action for future tick
    let current_tick = server.current_tick(match_id).await.unwrap();
    let intended_tick = current_tick + 5;

    let (action_id, scheduled_tick) = server
        .submit_action(match_id, session, CounterAction::Increment(10), intended_tick)
        .await
        .unwrap();
    assert_eq!(action_id, 1);
    assert_eq!(scheduled_tick, intended_tick);

    // Wait for the action to be processed
    sleep(Duration::from_millis(100)).await;

    // Poll events
    let (events, new_cursor) = server
        .poll_events(match_id, session, EventCursor(0))
        .await
        .unwrap();

    // Should have some events
    assert!(!events.is_empty());
    assert!(new_cursor.0 > 0);

    // Check that increment event is present
    let has_increment = events.iter().any(|e| {
        matches!(
            e.event,
            CounterEvent::Incremented {
                amount: 10,
                new_value: 10
            }
        )
    });
    assert!(has_increment, "Should have increment event");

    server.shutdown().await;
}

#[tokio::test]
async fn test_game_terminates() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    let match_id = server
        .create_match(CounterConfig { target: 10 }, 42)
        .await
        .unwrap();

    let (session, _player_id) = server.join_match(match_id).await.unwrap();

    // Submit action to reach target quickly
    let current_tick = server.current_tick(match_id).await.unwrap();
    server
        .submit_action(
            match_id,
            session,
            CounterAction::Increment(10),
            current_tick + 2,
        )
        .await
        .unwrap();

    // Wait for game to finish
    sleep(Duration::from_millis(100)).await;

    // Check status
    let matches = server.list_matches().await;
    let info = matches.iter().find(|m| m.match_id == match_id).unwrap();
    assert!(matches!(info.status, MatchStatus::Finished(TerminalOutcome::Win)));

    server.shutdown().await;
}

#[tokio::test]
async fn test_terminate_match() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    let match_id = server
        .create_match(CounterConfig { target: 1000 }, 42)
        .await
        .unwrap();

    // Terminate the match
    server.terminate_match(match_id).await.unwrap();

    // Match should be gone
    let matches = server.list_matches().await;
    assert!(matches.is_empty());

    server.shutdown().await;
}

#[tokio::test]
async fn test_multiple_players() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    // Create match requiring 2 players
    let match_id = server
        .create_match_with_players(CounterConfig { target: 1000 }, 42, 2)
        .await
        .unwrap();

    // First player joins - should be waiting
    let (session1, player1) = server.join_match(match_id).await.unwrap();
    assert_eq!(player1, 0);

    let matches = server.list_matches().await;
    let info = matches.iter().find(|m| m.match_id == match_id).unwrap();
    assert!(matches!(
        info.status,
        MatchStatus::WaitingForPlayers {
            current: 1,
            required: 2
        }
    ));

    // Second player joins - should start running
    let (session2, player2) = server.join_match(match_id).await.unwrap();
    assert_eq!(player2, 1);

    // Give tick loop a chance to check status
    sleep(Duration::from_millis(20)).await;

    let matches = server.list_matches().await;
    let info = matches.iter().find(|m| m.match_id == match_id).unwrap();
    assert!(matches!(info.status, MatchStatus::Running));

    // Both players can observe
    let obs1 = server.observe(match_id, session1).await.unwrap();
    let obs2 = server.observe(match_id, session2).await.unwrap();
    assert_eq!(obs1.target, obs2.target);

    server.shutdown().await;
}

#[tokio::test]
async fn test_determinism() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    // Run the same scenario twice with same seed
    let mut final_counters = Vec::new();

    for _ in 0..2 {
        let server: GameServer<CounterGame> = GameServer::new(config.clone());

        let match_id = server
            .create_match(CounterConfig { target: 100 }, 12345) // Same seed
            .await
            .unwrap();

        let (session, _) = server.join_match(match_id).await.unwrap();

        // Submit same actions at same ticks
        let current_tick = server.current_tick(match_id).await.unwrap();

        server
            .submit_action(
                match_id,
                session,
                CounterAction::Increment(5),
                current_tick + 3,
            )
            .await
            .unwrap();
        server
            .submit_action(
                match_id,
                session,
                CounterAction::Increment(7),
                current_tick + 5,
            )
            .await
            .unwrap();

        // Wait for actions to process
        sleep(Duration::from_millis(100)).await;

        let obs = server.observe(match_id, session).await.unwrap();
        final_counters.push(obs.counter);

        server.shutdown().await;
    }

    // Both runs should produce the same result
    assert_eq!(final_counters[0], final_counters[1]);
    assert_eq!(final_counters[0], 12); // 5 + 7
}

#[tokio::test]
async fn test_leave_match() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    let match_id = server
        .create_match(CounterConfig { target: 1000 }, 42)
        .await
        .unwrap();

    let (session, _) = server.join_match(match_id).await.unwrap();

    // Leave the match
    server.leave_match(match_id, session).await.unwrap();

    // Cannot observe with invalid session
    let result = server.observe(match_id, session).await;
    assert!(result.is_err());

    server.shutdown().await;
}

#[tokio::test]
async fn test_max_matches() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 4,
        max_matches: 2,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);

    // Create max matches
    server
        .create_match(CounterConfig { target: 1000 }, 1)
        .await
        .unwrap();
    server
        .create_match(CounterConfig { target: 1000 }, 2)
        .await
        .unwrap();

    // Third should fail
    let result = server
        .create_match(CounterConfig { target: 1000 }, 3)
        .await;
    assert!(result.is_err());

    server.shutdown().await;
}

#[tokio::test]
async fn test_observe_next_returns_immediately_for_cached_data() {
    let config = ServerConfig {
        default_tick_hz: 100,
        decision_hz: 10, // Decision every 10 ticks (100ms)
        max_matches: 10,
        event_buffer_capacity: 100,
    };

    let server: GameServer<CounterGame> = GameServer::new(config);
    let match_id = server
        .create_match(CounterConfig { target: 1000 }, 42)
        .await
        .unwrap();
    let (session, _) = server.join_match(match_id).await.unwrap();

    // Wait for first decision tick to occur
    sleep(Duration::from_millis(150)).await;

    // Get the current tick after decision tick has occurred
    let current_tick = server.current_tick(match_id).await.unwrap();

    // First call with after_tick=0 should return immediately (bootstrap case)
    let start = Instant::now();
    let (_obs1, _timed_out1) = server
        .observe_next(match_id, session, 0, 5000)
        .await
        .unwrap();
    let elapsed1 = start.elapsed();

    assert!(
        elapsed1 < Duration::from_millis(50),
        "First call should return immediately (got {:?})",
        elapsed1
    );

    // Second call with same tick should ALSO return immediately (NEW BEHAVIOR - key change)
    // This is the critical test: with after_tick = current_tick and last_decision >= current_tick,
    // it should return the cached observation immediately instead of waiting 100ms for next tick
    let start = Instant::now();
    let (_obs2, _timed_out2) = server
        .observe_next(match_id, session, current_tick, 5000)
        .await
        .unwrap();
    let elapsed2 = start.elapsed();

    assert!(
        elapsed2 < Duration::from_millis(50),
        "Calling with same cached tick should return immediately (got {:?})",
        elapsed2
    );

    server.shutdown().await;
}
