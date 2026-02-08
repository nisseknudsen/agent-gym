use std::fmt;

/// Error when creating a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateMatchError {
    /// Maximum number of concurrent matches reached.
    TooManyMatches,
}

impl fmt::Display for CreateMatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CreateMatchError::TooManyMatches => write!(f, "maximum number of matches reached"),
        }
    }
}

impl std::error::Error for CreateMatchError {}

/// Error for operations on a specific match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchError {
    /// Match not found.
    NotFound,
    /// Invalid session token.
    InvalidSession,
    /// Match has already terminated.
    Terminated,
}

impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatchError::NotFound => write!(f, "match not found"),
            MatchError::InvalidSession => write!(f, "invalid session token"),
            MatchError::Terminated => write!(f, "match has terminated"),
        }
    }
}

impl std::error::Error for MatchError {}

/// Error when joining a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    /// Match not found.
    NotFound,
    /// Match is full.
    MatchFull,
    /// Match has already started or finished.
    NotJoinable,
}

impl fmt::Display for JoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JoinError::NotFound => write!(f, "match not found"),
            JoinError::MatchFull => write!(f, "match is full"),
            JoinError::NotJoinable => write!(f, "match is not joinable"),
        }
    }
}

impl std::error::Error for JoinError {}

/// Error when submitting an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    /// Match not found.
    NotFound,
    /// Invalid session token.
    InvalidSession,
    /// Match has terminated.
    Terminated,
}

impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubmitError::NotFound => write!(f, "match not found"),
            SubmitError::InvalidSession => write!(f, "invalid session token"),
            SubmitError::Terminated => write!(f, "match has terminated"),
        }
    }
}

impl std::error::Error for SubmitError {}
