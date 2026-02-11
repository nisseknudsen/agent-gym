//! URL helpers for SSE streaming endpoints.

/// Build the SSE stream URL for observing a match.
pub fn stream_url(server_url: &str, match_id: u64) -> String {
    if server_url.is_empty() {
        format!("/api/stream/{}", match_id)
    } else {
        format!("{}/api/stream/{}", server_url, match_id)
    }
}

/// Build the SSE stream URL for the match list.
pub fn match_list_stream_url(server_url: &str) -> String {
    if server_url.is_empty() {
        "/api/stream/matches".to_string()
    } else {
        format!("{}/api/stream/matches", server_url)
    }
}
