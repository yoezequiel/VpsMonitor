//! Pushes [`ServerStatusEvent`]s to connected dashboard clients so a
//! server flipping online/offline updates the UI without a page reload
//! (`Proyecto.md` section 34).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::auth::hash_token;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct WsAuthQuery {
    token: String,
}

/// Browsers can't set an `Authorization` header on a WebSocket handshake,
/// so the dashboard passes its session token as a query parameter instead
/// — validated against the same session store as every other route.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(auth): Query<WsAuthQuery>,
) -> Response {
    match state
        .store
        .get_user_by_session_token_hash(&hash_token(&auth.token))
        .await
    {
        Ok(Some(_)) => ws.on_upgrade(move |socket| handle_socket(socket, state)),
        Ok(None) => ApiError::Unauthorized.into_response(),
        Err(err) => ApiError::from(err).into_response(),
    }
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut events = state.status_tx.subscribe();

    loop {
        tokio::select! {
            event = events.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "ws client too slow, dropped status events");
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                };
                let Ok(payload) = serde_json::to_string(&event) else { continue };
                if socket.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                // The dashboard doesn't send anything meaningful on this
                // socket today; any close/error from the client ends the loop.
                if !matches!(incoming, Some(Ok(_))) {
                    break;
                }
            }
        }
    }
}
