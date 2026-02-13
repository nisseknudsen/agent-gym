use crate::errors::{CreateMatchError, JoinError, MatchError, ObserveNextError, SubmitError};
use crate::match_handle::MatchHandle;
use crate::tick_loop::spawn_tick_loop;
use crate::types::{EventCursor, MatchInfo, ServerConfig, ServerEvent, SessionToken};
use sim_core::{ActionId, Game, MatchId, Tick};
use sim_host::MatchHost;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

struct MatchEntry<G: Game> {
    handle: MatchHandle<G>,
    task: JoinHandle<()>,
}

/// Game server that manages multiple concurrent matches.
pub struct GameServer<G: Game> {
    pub config: ServerConfig,
    matches: Arc<RwLock<HashMap<MatchId, MatchEntry<G>>>>,
    next_match_id: AtomicU64,
}

impl<G: Game + Send + 'static> GameServer<G>
where
    G::Action: Send,
    G::Observation: Send,
    G::Event: Send,
    G::Config: Send,
{
    /// Create a new game server with the given configuration.
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            matches: Arc::new(RwLock::new(HashMap::new())),
            next_match_id: AtomicU64::new(1),
        }
    }

    /// Shutdown the server, terminating all matches.
    pub async fn shutdown(&self) {
        let mut matches = self.matches.write().await;

        for (_, entry) in matches.drain() {
            entry.handle.request_shutdown();
            let _ = entry.task.await;
        }
    }

    /// Create a new match with the given configuration and seed.
    pub async fn create_match(
        &self,
        game_config: G::Config,
        seed: u64,
    ) -> Result<MatchId, CreateMatchError> {
        self.create_match_with_players(game_config, seed, 1).await
    }

    /// Create a new match with the given configuration, seed, and required player count.
    pub async fn create_match_with_players(
        &self,
        game_config: G::Config,
        seed: u64,
        required_players: u8,
    ) -> Result<MatchId, CreateMatchError> {
        let matches = self.matches.read().await;
        if matches.len() >= self.config.max_matches {
            return Err(CreateMatchError::TooManyMatches);
        }
        drop(matches);

        let match_id = self.next_match_id.fetch_add(1, Ordering::Relaxed);
        let host = MatchHost::new(game_config, seed, self.config.simulation_rate);
        let handle = MatchHandle::new(
            host,
            self.config.event_buffer_capacity,
            required_players,
            self.config.interaction_rate,
        );

        let task = spawn_tick_loop(handle.clone());

        let entry = MatchEntry { handle, task };

        let mut matches = self.matches.write().await;
        matches.insert(match_id, entry);

        Ok(match_id)
    }

    /// List all matches.
    pub async fn list_matches(&self) -> Vec<MatchInfo> {
        let matches = self.matches.read().await;
        let mut infos = Vec::with_capacity(matches.len());

        for (&match_id, entry) in matches.iter() {
            let status = entry.handle.status().await;
            let current_tick = entry.handle.current_tick().await;
            let player_count = entry.handle.player_count().await;

            infos.push(MatchInfo {
                match_id,
                status,
                current_tick,
                player_count,
            });
        }

        infos
    }

    /// Terminate a match.
    pub async fn terminate_match(&self, match_id: MatchId) -> Result<(), MatchError> {
        let mut matches = self.matches.write().await;

        if let Some(entry) = matches.remove(&match_id) {
            entry.handle.terminate().await;
            let _ = entry.task.await;
            Ok(())
        } else {
            Err(MatchError::NotFound)
        }
    }

    /// Spectate a match (read-only session).
    pub async fn spectate_match(
        &self,
        match_id: MatchId,
    ) -> Result<SessionToken, MatchError> {
        let matches = self.matches.read().await;

        let entry = matches.get(&match_id).ok_or(MatchError::NotFound)?;

        Ok(entry.handle.spectate().await)
    }

    /// Join a match as a new player.
    pub async fn join_match(
        &self,
        match_id: MatchId,
    ) -> Result<(SessionToken, sim_core::PlayerId), JoinError> {
        let matches = self.matches.read().await;

        let entry = matches.get(&match_id).ok_or(JoinError::NotFound)?;

        entry
            .handle
            .join_player()
            .await
            .ok_or(JoinError::NotJoinable)
    }

    /// Leave a match.
    pub async fn leave_match(
        &self,
        match_id: MatchId,
        session: SessionToken,
    ) -> Result<(), MatchError> {
        let matches = self.matches.read().await;

        let entry = matches.get(&match_id).ok_or(MatchError::NotFound)?;

        if entry.handle.leave_player(session).await {
            Ok(())
        } else {
            Err(MatchError::InvalidSession)
        }
    }

    /// Submit an action for a player.
    /// Returns (action_id, scheduled_tick) - the tick when the action will actually execute.
    pub async fn submit_action(
        &self,
        match_id: MatchId,
        session: SessionToken,
        action: G::Action,
        intended_tick: Tick,
    ) -> Result<(ActionId, Tick), SubmitError> {
        let matches = self.matches.read().await;

        let entry = matches.get(&match_id).ok_or(SubmitError::NotFound)?;

        entry
            .handle
            .submit_action(session, action, intended_tick)
            .await
    }

    /// Get the current observation for a player.
    pub async fn observe(
        &self,
        match_id: MatchId,
        session: SessionToken,
    ) -> Result<G::Observation, MatchError> {
        let matches = self.matches.read().await;

        let entry = matches.get(&match_id).ok_or(MatchError::NotFound)?;

        entry
            .handle
            .observe(session)
            .await
            .ok_or(MatchError::InvalidSession)
    }

    /// Wait for the next decision tick observation (long-poll).
    /// Returns (observation, timed_out).
    pub async fn observe_next(
        &self,
        match_id: MatchId,
        session: SessionToken,
        after_tick: Tick,
        max_wait_ms: u64,
    ) -> Result<(G::Observation, bool), ObserveNextError> {
        let handle = {
            let matches = self.matches.read().await;
            let entry = matches.get(&match_id).ok_or(ObserveNextError::NotFound)?;
            entry.handle.clone()
        };

        handle.observe_next(session, after_tick, max_wait_ms).await
    }

    /// Poll events from the given cursor.
    pub async fn poll_events(
        &self,
        match_id: MatchId,
        session: SessionToken,
        cursor: EventCursor,
    ) -> Result<(Vec<ServerEvent<G::Event>>, EventCursor), MatchError> {
        let matches = self.matches.read().await;

        let entry = matches.get(&match_id).ok_or(MatchError::NotFound)?;

        entry
            .handle
            .poll_events(session, cursor)
            .await
            .ok_or(MatchError::InvalidSession)
    }

    /// Get the current tick for a match.
    pub async fn current_tick(&self, match_id: MatchId) -> Result<Tick, MatchError> {
        let matches = self.matches.read().await;

        let entry = matches.get(&match_id).ok_or(MatchError::NotFound)?;

        Ok(entry.handle.current_tick().await)
    }
}
