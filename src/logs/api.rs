use axum::extract::State;
use axum::response::{Html, IntoResponse, Json};

use crate::proxy::ProxyState;

const LOGS_HTML: &str = include_str!("page.html");

pub async fn logs_page() -> Html<&'static str> {
    Html(LOGS_HTML)
}

pub async fn logs_bootstrap(State(state): State<ProxyState>) -> impl IntoResponse {
    let entries = state.request_log.list().await;
    let summary = state.request_log.summary().await;
    Json(serde_json::json!({
        "entries": entries,
        "summary": summary,
    }))
}

pub async fn logs_clear(State(state): State<ProxyState>) -> impl IntoResponse {
    state.request_log.clear().await;
    Json(serde_json::json!({ "ok": true }))
}
