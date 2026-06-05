use axum::extract::State;
use axum::response::{Html, IntoResponse, Json};
use axum::http::{header, HeaderMap, HeaderValue};
use serde::Deserialize;

use crate::claude;
use crate::config::{self, AppConfig};
use crate::provider;
use crate::proxy::{reload_config_in_state, request_tray_health_check, ProxyState};
use crate::settings;

const SETTINGS_HTML: &str = include_str!("page.html");
const BRAND_ICON_SVG: &str = include_str!("../../assets/brand-icon.svg");

pub async fn settings_page() -> Html<String> {
    Html(SETTINGS_HTML.replace("<!--BRAND_ICON-->", BRAND_ICON_SVG.trim()))
}

pub async fn brand_icon_svg() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    (headers, BRAND_ICON_SVG)
}

fn mask_key_preview(key: &str) -> String {
    if key.len() <= 8 {
        return "***".into();
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}

pub async fn settings_bootstrap() -> impl IntoResponse {
    let app = match AppConfig::load() {
        Ok(a) => a,
        Err(err) => {
            return Json(serde_json::json!({
                "error": err.to_string(),
                "providers": [],
                "active": "",
            }))
            .into_response();
        }
    };

    let mut providers = Vec::new();
    for preset in provider::list_presets(&app) {
        let key_preview = config::resolve_api_key(&preset.api_key_env)
            .ok()
            .map(|k| mask_key_preview(&k));
        providers.push(serde_json::json!({
            "id": preset.id,
            "name": preset.name,
            "signup_url": settings::signup_url(&preset.id),
            "key_configured": key_preview.is_some(),
            "key_preview": key_preview,
            "base_url": preset.base_url,
            "is_custom": preset.id == "custom",
        }));
    }

    Json(serde_json::json!({
        "active": app.active,
        "providers": providers,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct SettingsSaveBody {
    provider_id: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    base_url: String,
}

pub async fn settings_save(
    State(state): State<ProxyState>,
    Json(body): Json<SettingsSaveBody>,
) -> impl IntoResponse {
    match save_api_key(
        &state,
        &body.provider_id,
        body.api_key.trim(),
        body.base_url.trim(),
    )
    .await
    {
        Ok(message) => Json(serde_json::json!({
            "ok": true,
            "message": message,
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SettingsTestBody {
    provider_id: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    base_url: String,
}
pub async fn settings_test(Json(body): Json<SettingsTestBody>) -> impl IntoResponse {
    let app = match AppConfig::load() {
        Ok(a) => a,
        Err(err) => {
            return Json(serde_json::json!({
                "ok": false,
                "message": err.to_string(),
            }))
            .into_response();
        }
    };

    let mut provider = match provider::get_preset(&app, &body.provider_id) {
        Ok(p) => p.clone(),
        Err(err) => {
            return Json(serde_json::json!({
                "ok": false,
                "message": err.to_string(),
            }))
            .into_response();
        }
    };

    if let Err(err) = apply_custom_base_url(&mut provider, body.base_url.trim()) {
        return Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response();
    }

    let api_key = match resolve_key_for_request(&provider, body.api_key.trim()) {
        Ok(key) => key,
        Err(err) => {
            return Json(serde_json::json!({
                "ok": false,
                "message": format!("{err:#}"),
            }))
            .into_response();
        }
    };
    match test_api_key(&provider, &api_key).await {
        Ok(()) => Json(serde_json::json!({
            "ok": true,
            "message": format!("{} 连接成功！", provider.name),
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

pub async fn settings_clear_all(State(state): State<ProxyState>) -> impl IntoResponse {
    match clear_all_settings(&state).await {
        Ok(message) => Json(serde_json::json!({
            "ok": true,
            "message": message,
        }))
        .into_response(),
        Err(err) => Json(serde_json::json!({
            "ok": false,
            "message": format!("{err:#}"),
        }))
        .into_response(),
    }
}

async fn clear_all_settings(state: &ProxyState) -> anyhow::Result<String> {
    let app = AppConfig::clear_all_settings()?;
    claude::inject_proxy_config(&app)?;
    reload_config_in_state(state).await?;
    state.request_log.clear().await;
    request_tray_health_check(state);
    Ok(
        "已清除所有 Helper 配置（API Key、厂商选择、中转站地址）。请重新填写 Key 并重启 Claude Code。"
            .into(),
    )
}

async fn save_api_key(
    state: &ProxyState,
    provider_id: &str,
    api_key: &str,
    base_url: &str,
) -> anyhow::Result<String> {
    let mut app = AppConfig::load()?;
    provider::get_preset(&app, provider_id)?;
    let provider_cfg = app
        .providers
        .get_mut(provider_id)
        .ok_or_else(|| anyhow::anyhow!("未知模型预设: {provider_id}"))?;

    apply_custom_base_url(provider_cfg, base_url)?;

    let provider = provider_cfg.clone();
    if !api_key.is_empty() {
        config::save_env_value(&provider.api_key_env, api_key)?;
    } else if config::resolve_api_key(&provider.api_key_env).is_err() {
        anyhow::bail!("API Key 不能为空");
    }

    app.save()?;

    claude::inject_proxy_config(&app)?;
    reload_config_in_state(state).await?;
    request_tray_health_check(state);

    Ok(format!(
        "已保存 {} 配置。请完全退出并重新打开 Claude Code。",
        provider.name
    ))
}

fn apply_custom_base_url(
    provider: &mut config::ProviderConfig,
    base_url: &str,
) -> anyhow::Result<()> {
    if provider.id != "custom" {
        return Ok(());
    }
    if base_url.is_empty() && provider.base_url.trim().is_empty() {
        anyhow::bail!("请填写 Base URL");
    }
    if !base_url.is_empty() {
        provider.base_url = config::validate_base_url(base_url)?;
    }
    Ok(())
}

fn resolve_key_for_request(
    provider: &config::ProviderConfig,
    api_key: &str,
) -> anyhow::Result<String> {
    if !api_key.is_empty() {
        return Ok(api_key.to_string());
    }
    config::resolve_api_key(&provider.api_key_env)
}

pub async fn test_api_key(provider: &config::ProviderConfig, api_key: &str) -> anyhow::Result<()> {
    if api_key.is_empty() {
        anyhow::bail!("API Key 不能为空");
    }
    if provider.id == "custom" && provider.base_url.trim().is_empty() {
        anyhow::bail!("请填写 Base URL");
    }

    let client = config::build_upstream_client(std::time::Duration::from_secs(30))?;

    if provider.uses_anthropic_upstream() {
        let url = format!(
            "{}/v1/messages",
            provider.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": provider.upstream_model(),
            "max_tokens": 8,
            "messages": [{"role": "user", "content": "ping"}]
        });
        let resp = client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("连接失败 ({status})：{text}");
    }

    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": provider.upstream_model(),
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 8
    });
    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    anyhow::bail!("连接失败 ({status})：{text}")
}
