use std::sync::{Arc, RwLock as StdRwLock};

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use reqwest::Client;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::claude::desktop_gateway::{
    build_inference_models, request_uses_desktop_roles, rewrite_request_model, DESKTOP_ROLE_SONNET,
};
use crate::config::{self, AppConfig};

mod anthropic_to_chat;
mod chat_to_anthropic;
mod logged_stream;
use anthropic_to_chat::convert_anthropic_to_chat;
use chat_to_anthropic::{
    anthropic_stream_preamble, convert_chat_json_to_anthropic, wrap_chat_sse_as_anthropic_sse,
};
use logged_stream::LoggingByteStream;
use crate::logs::{logs_bootstrap, logs_clear, logs_page};
use crate::request_log::{
    extract_model_from_body, parse_usage_from_json, PendingRequest, RequestLogStore,
};
use crate::settings::{
    brand_icon_svg, settings_bootstrap, settings_clear_all, settings_page, settings_save,
    settings_test,
};

type TrayHealthCheckHook = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct ProxyState {
    pub config: Arc<RwLock<AppConfig>>,
    pub client: Client,
    pub request_log: RequestLogStore,
    tray_health_check: Arc<StdRwLock<Option<TrayHealthCheckHook>>>,
}

pub fn spawn_server(config: AppConfig) -> anyhow::Result<Arc<ProxyState>> {
    let state = Arc::new(ProxyState {
        config: Arc::new(RwLock::new(config.clone())),
        client: config::build_upstream_client(std::time::Duration::from_secs(300))
            .expect("failed to build HTTP client"),
        request_log: RequestLogStore::new(),
        tray_health_check: Arc::new(StdRwLock::new(None)),
    });
    let addr = format!("{}:{}", config.proxy.host, config.proxy.port);
    let serve_state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = run_listener(serve_state, &addr).await {
            tracing::error!("代理异常退出: {err:#}");
        }
    });
    Ok(state)
}

pub async fn notify_running_proxy_reload(app: &AppConfig) -> bool {
    let url = format!(
        "http://{}:{}/admin/reload",
        app.proxy.host, app.proxy.port
    );
    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client.post(&url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

pub async fn reload_config_in_state(state: &ProxyState) -> anyhow::Result<AppConfig> {
    let app = AppConfig::load()?;
    let mut cfg = state.config.write().await;
    *cfg = app.clone();
    Ok(app)
}

pub fn register_tray_health_check(state: &Arc<ProxyState>, hook: TrayHealthCheckHook) {
    if let Ok(mut slot) = state.tray_health_check.write() {
        *slot = Some(hook);
    }
}

pub fn request_tray_health_check(state: &ProxyState) {
    let hook = state
        .tray_health_check
        .read()
        .ok()
        .and_then(|slot| slot.clone());
    if let Some(hook) = hook {
        hook();
    }
}

pub async fn start_server(config: AppConfig) -> anyhow::Result<()> {
    let addr = format!("{}:{}", config.proxy.host, config.proxy.port);
    let state = ProxyState {
        config: Arc::new(RwLock::new(config.clone())),
        client: config::build_upstream_client(std::time::Duration::from_secs(300))?,
        request_log: RequestLogStore::new(),
        tray_health_check: Arc::new(StdRwLock::new(None)),
    };
    run_listener(Arc::new(state), &addr).await
}

async fn run_listener(state: Arc<ProxyState>, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/admin/reload", post(admin_reload))
        .route("/admin/settings", get(settings_page))
        .route("/admin/brand-icon.svg", get(brand_icon_svg))
        .route("/admin/settings/bootstrap", get(settings_bootstrap))
        .route("/admin/settings/save", post(settings_save))
        .route("/admin/settings/clear-all", post(settings_clear_all))
        .route("/admin/settings/test", post(settings_test))
        .route("/admin/logs", get(logs_page))
        .route("/admin/logs/bootstrap", get(logs_bootstrap))
        .route("/admin/logs/clear", post(logs_clear))
        .route("/v1/models", get(list_models))
        .route("/v1/messages", post(proxy_messages))
        .route("/claude-desktop/v1/models", get(list_desktop_models))
        .route("/claude-desktop/v1/messages", post(proxy_desktop_messages))
        .fallback(any(catch_all))
        .layer(TraceLayer::new_for_http())
        .with_state(state.as_ref().clone());

    info!("Claude Code Helper 代理已启动: http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        anyhow::anyhow!("无法绑定端口 {addr}: {e}。请检查端口是否被占用。")
    })?;

    axum::serve(listener, app).await?;
    Ok(())
}

async fn admin_reload(State(state): State<ProxyState>) -> impl IntoResponse {
    match reload_config_in_state(&state).await {
        Ok(app) => {
            let provider_name = app
                .active_provider()
                .map(|p| p.name.clone())
                .unwrap_or_else(|_| "unknown".into());
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "ok",
                    "active": app.active,
                    "provider": provider_name,
                })),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn health(State(state): State<ProxyState>) -> impl IntoResponse {
    let config = state.config.read().await;
    let active = config.active.clone();
    let provider = config
        .active_provider()
        .map(|p| p.name.clone())
        .unwrap_or_else(|_| "unknown".into());
    axum::Json(serde_json::json!({
        "status": "ok",
        "active": active,
        "provider": provider,
    }))
}

fn gateway_models_payload(provider: &config::ProviderConfig) -> serde_json::Value {
    let models = build_inference_models(provider)
        .into_iter()
        .map(|entry: serde_json::Value| {
            let id = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(DESKTOP_ROLE_SONNET);
            let display = entry
                .get("labelOverride")
                .and_then(|v| v.as_str())
                .unwrap_or(id);
            serde_json::json!({
                "id": id,
                "display_name": display,
                "type": "model",
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({ "data": models })
}

async fn list_models(State(state): State<ProxyState>) -> impl IntoResponse {
    gateway_models_response(&state).await
}

async fn list_desktop_models(State(state): State<ProxyState>) -> impl IntoResponse {
    gateway_models_response(&state).await
}

async fn gateway_models_response(state: &ProxyState) -> Response {
    let config = state.config.read().await;
    let provider = match config.active_provider() {
        Ok(p) => p,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };

    axum::Json(gateway_models_payload(provider)).into_response()
}

async fn proxy_messages(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    proxy_messages_inner(&state, headers, body, false).await
}

async fn proxy_desktop_messages(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    proxy_messages_inner(&state, headers, body, true).await
}

async fn proxy_messages_inner(
    state: &ProxyState,
    headers: HeaderMap,
    body: axum::body::Bytes,
    map_desktop_roles: bool,
) -> Response {
    let config = state.config.read().await.clone();
    let provider = match config.active_provider() {
        Ok(p) => p.clone(),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };

    let body = if map_desktop_roles || request_uses_desktop_roles(&body) {
        match rewrite_request_model(&body, &provider) {
            Ok(rewritten) => axum::body::Bytes::from(rewritten),
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "type": "error",
                        "error": {
                            "type": "invalid_request_error",
                            "message": format!("模型映射失败: {err}")
                        }
                    })),
                )
                    .into_response();
            }
        }
    } else {
        body
    };

    if provider.uses_anthropic_upstream() {
        return forward_anthropic_request(state, &provider, headers, body).await;
    }

    let upstream_model = extract_model_from_body(&body, provider.upstream_model());
    match convert_anthropic_to_chat(&body, &upstream_model) {
        Ok(chat_body) => {
            forward_chat_as_anthropic(state, &provider, headers, chat_body.into()).await
        }
        Err(err) => {
            warn!("Anthropic 请求转换失败: {err}");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "invalid_request_error",
                        "message": format!("Anthropic 请求转换失败: {err}")
                    }
                })),
            )
                .into_response()
        }
    }
}

async fn catch_all(
    State(state): State<ProxyState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let path = uri.path();
    if !path.starts_with("/v1/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let config = state.config.read().await.clone();
    let provider = match config.active_provider() {
        Ok(p) => p.clone(),
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    };
    if provider.uses_anthropic_upstream() {
        let upstream_path = path.trim_start_matches('/');
        return forward_anthropic_request_path(&state, &provider, upstream_path, method, headers, body)
            .await;
    }
    (StatusCode::NOT_FOUND, "not found").into_response()
}

async fn forward_anthropic_request(
    state: &ProxyState,
    provider: &config::ProviderConfig,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    forward_anthropic_request_path(state, provider, "v1/messages", Method::POST, headers, body)
        .await
}

async fn forward_anthropic_request_path(
    state: &ProxyState,
    provider: &config::ProviderConfig,
    upstream_path: &str,
    method: Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let started = std::time::Instant::now();
    let model = extract_model_from_body(&body, provider.upstream_model());
    let stream = body
        .get(..)
        .and_then(|slice| serde_json::from_slice::<serde_json::Value>(slice).ok())
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);

    let pending_base = PendingRequest {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        model,
        path: format!("/{upstream_path}"),
        stream,
        started,
        status: 0,
    };

    let api_key = match config::resolve_api_key(&provider.api_key_env) {
        Ok(key) => key,
        Err(err) => return auth_error_response(state, pending_base, err.to_string()).await,
    };

    let target = format!(
        "{}/{}",
        provider.base_url.trim_end_matches('/'),
        upstream_path.trim_start_matches('/')
    );

    let mut request = state.client.request(method, &target);
    request = request.header("x-api-key", &api_key);
    request = request.header("anthropic-version", anthropic_version_header(&headers));
    request = request.header("Content-Type", "application/json");
    if !body.is_empty() {
        request = request.body(body.to_vec());
    }

    forward_upstream_response(state, request, pending_base).await
}

async fn forward_chat_as_anthropic(
    state: &ProxyState,
    provider: &config::ProviderConfig,
    _headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let started = std::time::Instant::now();
    let model = extract_model_from_body(&body, provider.upstream_model());
    let stream = body
        .get(..)
        .and_then(|slice| serde_json::from_slice::<serde_json::Value>(slice).ok())
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);

    let pending_base = PendingRequest {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        model: model.clone(),
        path: "/v1/chat/completions".into(),
        stream,
        started,
        status: 0,
    };

    let api_key = match config::resolve_api_key(&provider.api_key_env) {
        Ok(key) => key,
        Err(err) => return auth_error_response(state, pending_base, err.to_string()).await,
    };

    let target = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );

    let mut request = state.client.request(Method::POST, &target);
    request = request.header("Authorization", format!("Bearer {api_key}"));
    request = request.header("Content-Type", "application/json");
    request = request.body(body.to_vec());

    let response = forward_upstream_response(state, request, pending_base).await;
    convert_chat_response_to_anthropic(response, &model).await
}

async fn forward_upstream_response(
    state: &ProxyState,
    request: reqwest::RequestBuilder,
    pending_base: PendingRequest,
) -> Response {
    match request.send().await {
        Ok(resp) => {
            let status = resp.status();
            let mut pending = pending_base;
            pending.status = status.as_u16();
            let mut response_headers = HeaderMap::new();
            let mut is_sse = false;
            for (name, value) in resp.headers() {
                if name == reqwest::header::TRANSFER_ENCODING {
                    continue;
                }
                if name == reqwest::header::CONTENT_TYPE {
                    if let Ok(v) = value.to_str() {
                        if v.to_ascii_lowercase().contains("text/event-stream") {
                            is_sse = true;
                        }
                    }
                }
                if let Ok(v) = HeaderValue::from_bytes(value.as_bytes()) {
                    response_headers.insert(name, v);
                }
            }

            if is_sse {
                pending.stream = true;
                response_headers.remove(reqwest::header::CONTENT_LENGTH);
                let stream = LoggingByteStream::new(
                    resp.bytes_stream(),
                    pending,
                    state.request_log.clone(),
                );
                let body = Body::from_stream(stream);
                (status, response_headers, body).into_response()
            } else {
                let bytes = resp.bytes().await.unwrap_or_default();
                let usage = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|value| parse_usage_from_json(&value));
                let entry = state.request_log.finalize(pending, usage);
                state.request_log.push(entry).await;
                (status, response_headers, Body::from(bytes)).into_response()
            }
        }
        Err(err) => {
            warn!("上游请求失败: {err}");
            let mut pending = pending_base;
            pending.status = StatusCode::BAD_GATEWAY.as_u16();
            let entry = state.request_log.finalize(pending, None);
            state.request_log.push(entry).await;
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("上游请求失败: {err}")
                    }
                })),
            )
                .into_response()
        }
    }
}

async fn auth_error_response(
    state: &ProxyState,
    mut pending: PendingRequest,
    message: String,
) -> Response {
    pending.status = StatusCode::UNAUTHORIZED.as_u16();
    let entry = state.request_log.finalize(pending, None);
    state.request_log.push(entry).await;
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "type": "error",
            "error": {
                "type": "authentication_error",
                "message": message
            }
        })),
    )
        .into_response()
}

async fn convert_chat_response_to_anthropic(response: Response, model: &str) -> Response {
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !status.is_success() {
        return response;
    }

    if content_type.contains("text/event-stream") {
        use async_stream::stream;
        use futures_util::TryStreamExt;

        let (mut parts, body) = response.into_parts();
        let message_id = format!("msg_{}", uuid::Uuid::new_v4());
        let preamble = anthropic_stream_preamble(model, &message_id);
        let upstream_stream = body.into_data_stream().map_err(std::io::Error::other);

        let translated = stream! {
            yield Ok::<_, std::io::Error>(axum::body::Bytes::from(preamble));
            let mut buffer = String::new();
            futures_util::pin_mut!(upstream_stream);
            while let Some(chunk) = upstream_stream.try_next().await? {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(pos) = buffer.find("\n\n") {
                    let event = buffer.drain(..pos + 2).collect::<String>();
                    if let Some(converted) = wrap_chat_sse_as_anthropic_sse(&event, &message_id) {
                        yield Ok(axum::body::Bytes::from(converted));
                    }
                }
            }
        };

        parts.headers.remove(reqwest::header::CONTENT_LENGTH);
        parts.headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        parts.headers.insert(
            reqwest::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        );
        return Response::from_parts(parts, Body::from_stream(translated)).into_response();
    }

    let body = response.into_body();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => {
            warn!("读取上游响应失败: {err}");
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("读取上游响应失败: {err}")
                    }
                })),
            )
                .into_response();
        }
    };

    match convert_chat_json_to_anthropic(&bytes) {
        Ok(converted) => (
            status,
            [(reqwest::header::CONTENT_TYPE.as_str(), "application/json")],
            converted,
        )
            .into_response(),
        Err(err) => {
            warn!("Chat 响应转换 Anthropic 失败: {err}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "api_error",
                        "message": format!("Chat 响应转换 Anthropic 失败: {err}")
                    }
                })),
            )
                .into_response()
        }
    }
}

fn anthropic_version_header(headers: &HeaderMap) -> String {
    headers
        .get("anthropic-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("2023-06-01")
        .to_string()
}
