use crate::events::EventBuffer;
use crate::types::{EventCursor, MatchStatus, ServerEvent, SessionToken};
use sim_core::{ActionEnvelope, ActionId, Game, PlayerId, Tick};
use sim_host::MatchHost;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};

/// Per-session observation tracking for observe_next.
pub struct SessionObserveState {
    pub last_observed_tick: Tick,
    pub is_waiting: bool,
}

/// Internal state of a match.
pub struct MatchInner<G: Game> {
    pub host: MatchHost<G>,
    pub events: EventBuffer<G::Event>,
    pub sessions: HashMap<SessionToken, PlayerId>,
    pub players: HashMap<PlayerId, SessionToken>,
    pub spectators: HashSet<SessionToken>,
    pub next_session_id: u64,
    pub next_action_id: ActionId,
    pub required_players: u8,
    pub status: MatchStatus,

    // Decision tick support
    pub decision_stride: u64,
    pub last_decision_tick: Tick,
    pub decision_notify: Arc<Notify>,
    pub cached_observations: HashMap<PlayerId, G::Observation>,
    pub session_observe_state: HashMap<SessionToken, SessionObserveState>,
}

impl<G: Game> MatchInner<G> {
    pub fn new(
        host: MatchHost<G>,
        event_buffer_capacity: usize,
        required_players: u8,
        decision_hz: u32,
    ) -> Self {
        let tick_hz = host.tick_hz();
        let decision_stride = (tick_hz / decision_hz).max(1) as u64;
        Self {
            host,
            events: EventBuffer::new(event_buffer_capacity),
            sessions: HashMap::new(),
            players: HashMap::new(),
            spectators: HashSet::new(),
            next_session_id: 1,
            next_action_id: 1,
            required_players,
            status: MatchStatus::WaitingForPlayers {
                current: 0,
                required: required_players,
            },
            decision_stride,
            last_decision_tick: 0,
            decision_notify: Arc::new(Notify::new()),
            cached_observations: HashMap::new(),
            session_observe_state: HashMap::new(),
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
    pub fn new(
        host: MatchHost<G>,
        event_buffer_capacity: usize,
        required_players: u8,
        decision_hz: u32,
    ) -> Self {
        let tick_hz = host.tick_hz();
        Self {
            inner: Arc::new(Mutex::new(MatchInner::new(
                host,
                event_buffer_capacity,
                required_players,
                decision_hz,
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

    /// Create a spectator session for the match.
    /// Returns a session token. Spectators can observe and poll events but cannot submit actions.
    pub async fn spectate(&self) -> SessionToken {
        let mut inner = self.inner.lock().await;
        let session = SessionToken(inner.next_session_id);
        inner.next_session_id += 1;
        inner.spectators.insert(session);
        inner.session_observe_state.insert(
            session,
            SessionObserveState {
                last_observed_tick: 0,
                is_waiting: false,
            },
        );
        session
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
                inner.session_observe_state.insert(
                    session,
                    SessionObserveState {
                        last_observed_tick: 0,
                        is_waiting: false,
                    },
                );

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

    /// Remove a player or spectator from the match.
    pub async fn leave_player(&self, session: SessionToken) -> bool {
        let mut inner = self.inner.lock().await;

        inner.session_observe_state.remove(&session);

        if let Some(player_id) = inner.sessions.remove(&session) {
            inner.players.remove(&player_id);
            true
        } else {
            inner.spectators.remove(&session)
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

    /// Get the current observation for a player or spectator.
    pub async fn observe(&self, session: SessionToken) -> Option<G::Observation> {
        let inner = self.inner.lock().await;

        let player_id = if let Some(&pid) = inner.sessions.get(&session) {
            pid
        } else if inner.spectators.contains(&session) {
            0
        } else {
            return None;
        };
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

        // Verify session is valid (player or spectator)
        if !inner.sessions.contains_key(&session) && !inner.spectators.contains(&session) {
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

    /// Check if a session is valid (player or spectator).
    pub async fn is_valid_session(&self, session: SessionToken) -> bool {
        let inner = self.inner.lock().await;
        inner.sessions.contains_key(&session) || inner.spectators.contains(&session)
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

        // Check if this is a decision tick
        let current_tick = inner.host.current_tick();
        if current_tick % inner.decision_stride == 0 {
            // Collect player IDs first to avoid borrow conflict
            let mut player_ids: Vec<PlayerId> = inner.players.keys().copied().collect();
            // Also cache for spectators (player_id 0)
            if !inner.spectators.is_empty() && !player_ids.contains(&0) {
                player_ids.push(0);
            }

            for player_id in player_ids {
                let obs = inner.host.game().observe(current_tick, player_id);
                inner.cached_observations.insert(player_id, obs);
            }

            inner.last_decision_tick = current_tick;
            inner.decision_notify.notify_waiters();
        }

        // Check if terminal
        if let Some(outcome) = inner.host.is_terminal() {
            inner.status = MatchStatus::Finished(outcome);
            // Notify any waiting observers so they unblock
            inner.decision_notify.notify_waiters();
            return true;
        }

        false
    }

    /// Terminate the match.
    pub async fn terminate(&self) {
        let mut inner = self.inner.lock().await;
        inner.status = MatchStatus::Terminated;
        inner.decision_notify.notify_waiters();
        drop(inner);
        self.request_shutdown();
    }

    /// Wait for the next decision tick observation.
    ///
    /// # Parameters
    /// - `session`: Your session token (from join or spectate)
    /// - `after_tick`: The last tick you observed. Pass 0 for the first call.
    /// - `max_wait_ms`: Maximum time to wait in milliseconds before timing out.
    ///
    /// # Behavior
    /// - If a decision tick > `after_tick` is cached, returns immediately with that tick
    /// - If you call with `after_tick` equal to what you already observed, we force you to wait for the NEXT new tick
    ///   (anti-spam: prevents abuse from repeatedly calling with stale data)
    /// - Otherwise, blocks until the next decision tick occurs
    /// - Times out after `max_wait_ms` and returns current state
    ///
    /// # Returns
    /// - `Ok((observation, false))` if new data was available or notified
    /// - `Ok((observation, true))` if timed out and returning current state
    ///
    /// # Example
    /// ```ignore
    /// // First call - always returns immediately with current state
    /// let (obs1, _) = handle.observe_next(session, 0, 5000).await?;
    /// let tick1 = obs1.tick;
    ///
    /// // Correct usage: pass the returned tick for the next call
    /// let (obs2, _) = handle.observe_next(session, tick1, 5000).await?;
    /// // obs2.tick > tick1 (waited for and got next decision tick)
    ///
    /// // Incorrect usage: calling with the same tick again forces a wait
    /// let (obs3, _) = handle.observe_next(session, tick1, 5000).await?;
    /// // Will block until next decision tick occurs, even though we already have tick1 cached
    /// // This prevents spam - agents MUST track the returned tick
    /// ```
    pub async fn observe_next(
        &self,
        session: SessionToken,
        after_tick: Tick,
        max_wait_ms: u64,
    ) -> Result<(G::Observation, bool), crate::errors::ObserveNextError> {
        use crate::errors::ObserveNextError;
        use std::time::Instant;

        let start_time = Instant::now();

        loop {
            let elapsed = start_time.elapsed();
            let remaining_ms = max_wait_ms.saturating_sub(elapsed.as_millis() as u64);

            let (notify, should_return_timeout) = {
                let mut inner = self.inner.lock().await;

                // Resolve player_id for this session
                let player_id = if let Some(&pid) = inner.sessions.get(&session) {
                    pid
                } else if inner.spectators.contains(&session) {
                    0
                } else {
                    return Err(ObserveNextError::InvalidSession);
                };

                {
                    let state = inner
                        .session_observe_state
                        .get(&session)
                        .ok_or(ObserveNextError::InvalidSession)?;

                    if state.is_waiting {
                        return Err(ObserveNextError::AlreadyWaiting);
                    }
                }

                // Handle bootstrap case: very first call when no decision ticks have happened yet
                if after_tick == 0 && inner.last_decision_tick == 0 {
                    let tick = inner.host.current_tick();
                    let obs = inner.host.game().observe(tick, player_id);
                    if let Some(state) = inner.session_observe_state.get_mut(&session) {
                        state.last_observed_tick = tick;
                    }
                    return Ok((obs, false));
                }

                // Anti-spam: if agent calls with after_tick <= what they already observed,
                // force them to wait for the next NEW decision tick instead of returning cached data.
                // This prevents agents from abusing the API by repeatedly calling with the same after_tick.
                let mut wait_after_tick = after_tick;
                if let Some(state) = inner.session_observe_state.get(&session) {
                    if after_tick <= state.last_observed_tick && state.last_observed_tick > 0 {
                        // Agent is asking for data they already got (or older)
                        // Force them to wait for NEW data
                        wait_after_tick = state.last_observed_tick;
                    }
                }

                // Check if we already have a cached observation at or newer than after_tick
                let last_decision = inner.last_decision_tick;
                if last_decision > wait_after_tick && inner.cached_observations.contains_key(&player_id) {
                    if let Some(state) = inner.session_observe_state.get_mut(&session) {
                        state.last_observed_tick = last_decision;
                    }
                    let obs = inner
                        .cached_observations
                        .get(&player_id)
                        .cloned()
                        .ok_or(ObserveNextError::ObservationNotReady)?;
                    return Ok((obs, false));
                }

                // Check if we've exceeded max_wait_ms
                if remaining_ms == 0 {
                    let tick = inner.host.current_tick();
                    let obs = inner.host.game().observe(tick, player_id);
                    return Ok((obs, true));
                }

                // Mark as waiting and get notify handle
                if let Some(state) = inner.session_observe_state.get_mut(&session) {
                    state.is_waiting = true;
                }
                (Arc::clone(&inner.decision_notify), false)
            };

            // Wait outside the lock for the remaining time, then loop back
            let _timed_out = tokio::time::timeout(
                Duration::from_millis(remaining_ms),
                notify.notified(),
            )
            .await
            .is_err();

            // Clear is_waiting flag before looping back
            {
                let mut inner = self.inner.lock().await;
                if let Some(state) = inner.session_observe_state.get_mut(&session) {
                    state.is_waiting = false;
                }
            }

            // Loop back to check if we now have the data we need
            // The loop will either:
            // 1. Return the cached observation if last_decision >= after_tick
            // 2. Return timeout if max_wait_ms elapsed
            // 3. Loop and wait for next notification
        }
    }
}
