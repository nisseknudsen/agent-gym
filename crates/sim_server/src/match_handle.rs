use crate::events::EventBuffer;
use crate::types::{EventCursor, MatchStatus, ServerEvent, SessionToken};
use sim_core::{ActionEnvelope, ActionId, Game, PlayerId, Tick};
use sim_host::MatchHost;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Internal state of a match.
pub struct MatchInner<G: Game> {
    pub host: MatchHost<G>,
    pub events: EventBuffer<G::Event>,
    pub sessions: HashMap<SessionToken, PlayerId>,
    pub players: HashMap<PlayerId, SessionToken>,
    pub next_session_id: u64,
    pub next_action_id: ActionId,
    pub required_players: u8,
    pub status: MatchStatus,
}

impl<G: Game> MatchInner<G> {
    pub fn new(host: MatchHost<G>, event_buffer_capacity: usize, required_players: u8) -> Self {
        Self {
            host,
            events: EventBuffer::new(event_buffer_capacity),
            sessions: HashMap::new(),
            players: HashMap::new(),
            next_session_id: 1,
            next_action_id: 1,
            required_players,
            status: MatchStatus::WaitingForPlayers {
                current: 0,
                required: required_players,
            },
        }
    }

    pub fn player_count(&self) -> u8 {
        self.sessions.len() as u8
    }
}

/// Thread-safe handle to a match.
pub struct MatchHandle<G: Game> {
    pub inner: Arc<Mutex<MatchInner<G>>>,
    shutdown: Arc<AtomicBool>,
    tick_hz: u32,
}

impl<G: Game> Clone for MatchHandle<G> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            shutdown: Arc::clone(&self.shutdown),
            tick_hz: self.tick_hz,
        }
    }
}

impl<G: Game> MatchHandle<G> {
    pub fn new(host: MatchHost<G>, event_buffer_capacity: usize, required_players: u8) -> Self {
        let tick_hz = host.tick_hz();
        Self {
            inner: Arc::new(Mutex::new(MatchInner::new(
                host,
                event_buffer_capacity,
                required_players,
            ))),
            shutdown: Arc::new(AtomicBool::new(false)),
            tick_hz,
        }
    }

    pub fn tick_hz(&self) -> u32 {
        self.tick_hz
    }

    pub fn should_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Join a new player to the match.
    /// Returns the session token and player ID.
    pub async fn join_player(&self) -> Option<(SessionToken, PlayerId)> {
        let mut inner = self.inner.lock().await;

        // Can only join in WaitingForPlayers status
        match inner.status {
            MatchStatus::WaitingForPlayers { current, required } => {
                if current >= required {
                    return None;
                }

                let player_id = inner.host.join_player();
                let session = SessionToken(inner.next_session_id);
                inner.next_session_id += 1;

                inner.sessions.insert(session, player_id);
                inner.players.insert(player_id, session);

                let new_count = current + 1;
                if new_count >= required {
                    inner.status = MatchStatus::Running;
                } else {
                    inner.status = MatchStatus::WaitingForPlayers {
                        current: new_count,
                        required,
                    };
                }

                Some((session, player_id))
            }
            _ => None,
        }
    }

    /// Remove a player from the match.
    pub async fn leave_player(&self, session: SessionToken) -> bool {
        let mut inner = self.inner.lock().await;

        if let Some(player_id) = inner.sessions.remove(&session) {
            inner.players.remove(&player_id);
            true
        } else {
            false
        }
    }

    /// Submit an action for a player.
    /// Returns (action_id, scheduled_tick) - the tick when the action will actually execute.
    /// If intended_tick is in the past, the action is scheduled for the next tick.
    pub async fn submit_action(
        &self,
        session: SessionToken,
        action: G::Action,
        intended_tick: Tick,
    ) -> Result<(ActionId, Tick), crate::errors::SubmitError> {
        let mut inner = self.inner.lock().await;

        let player_id = inner
            .sessions
            .get(&session)
            .copied()
            .ok_or(crate::errors::SubmitError::InvalidSession)?;

        if matches!(
            inner.status,
            MatchStatus::Finished(_) | MatchStatus::Terminated
        ) {
            return Err(crate::errors::SubmitError::Terminated);
        }

        let action_id = inner.next_action_id;
        inner.next_action_id += 1;

        let envelope = ActionEnvelope {
            player_id,
            action_id,
            intended_tick,
            payload: action,
        };

        let scheduled_tick = inner.host.submit(envelope);

        Ok((action_id, scheduled_tick))
    }

    /// Get the current observation for a player.
    pub async fn observe(&self, session: SessionToken) -> Option<G::Observation> {
        let inner = self.inner.lock().await;

        let player_id = inner.sessions.get(&session).copied()?;
        let tick = inner.host.current_tick();
        Some(inner.host.game().observe(tick, player_id))
    }

    /// Poll events from the given cursor.
    pub async fn poll_events(
        &self,
        session: SessionToken,
        cursor: EventCursor,
    ) -> Option<(Vec<ServerEvent<G::Event>>, EventCursor)> {
        let inner = self.inner.lock().await;

        // Verify session is valid
        if !inner.sessions.contains_key(&session) {
            return None;
        }

        Some(inner.events.get_from_cursor(cursor))
    }

    /// Get the current tick.
    pub async fn current_tick(&self) -> Tick {
        let inner = self.inner.lock().await;
        inner.host.current_tick()
    }

    /// Get the current match status.
    pub async fn status(&self) -> MatchStatus {
        let inner = self.inner.lock().await;
        inner.status
    }

    /// Get the player count.
    pub async fn player_count(&self) -> u8 {
        let inner = self.inner.lock().await;
        inner.player_count()
    }

    /// Check if a session is valid.
    pub async fn is_valid_session(&self, session: SessionToken) -> bool {
        let inner = self.inner.lock().await;
        inner.sessions.contains_key(&session)
    }

    /// Step one tick and update status.
    /// Returns true if the game is now finished.
    pub async fn step_one_tick(&self) -> bool {
        let mut inner = self.inner.lock().await;

        // Only step if running
        if !matches!(inner.status, MatchStatus::Running) {
            return matches!(
                inner.status,
                MatchStatus::Finished(_) | MatchStatus::Terminated
            );
        }

        if let Some(events) = inner.host.step_one_tick() {
            let tick = inner.host.current_tick();
            for event in events {
                inner.events.push(tick, event);
            }
        }

        // Check if terminal
        if let Some(outcome) = inner.host.is_terminal() {
            inner.status = MatchStatus::Finished(outcome);
            return true;
        }

        false
    }

    /// Terminate the match.
    pub async fn terminate(&self) {
        let mut inner = self.inner.lock().await;
        inner.status = MatchStatus::Terminated;
        drop(inner);
        self.request_shutdown();
    }
}
