pub mod ccd_binary;
pub mod desktop_gateway;

use serde_json::{Map, Value};

use crate::config::{self, AppConfig, ProviderConfig};
use crate::paths;

pub use desktop_gateway::{
    desktop_app_uses_third_party_mode, desktop_gateway_matches, read_desktop_gateway_base_url,
};

pub fn dual_surface_synced(app: &AppConfig) -> bool {
    desktop_gateway_matches(app)
        && desktop_app_uses_third_party_mode()
        && claude_proxy_port_matches(app)
}

const HELPER_MARKER: &str = "claude-code-helper";
pub fn backup_claude_settings() -> anyhow::Result<Option<std::path::PathBuf>> {
    let source = paths::claude_settings_path()?;
    if !source.exists() {
        return Ok(None);
    }
    paths::ensure_helper_dirs()?;
    let backup_dir = paths::helper_backups_dir()?;
    let stamp = chrono_like_timestamp();
    let backup = backup_dir.join(format!("settings.json.{stamp}.bak"));
    std::fs::copy(&source, &backup)?;
    Ok(Some(backup))
}

pub fn inject_proxy_config(app: &AppConfig) -> anyhow::Result<()> {
    backup_claude_settings()?;
    let mut app = app.clone();
    if app.proxy.port != config::DEFAULT_PORT {
        app.proxy.port = config::DEFAULT_PORT;
    }
    sync_provider_presets(&mut app);
    app.save()?;

    let provider = app.active_provider()?;
    ensure_helper_env_keys(provider)?;
    crate::env_sync::sync_claude_desktop_credentials(provider)?;

    let settings_path = paths::claude_settings_path()?;
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = render_claude_settings(&app, provider)?;
    config::write_atomic(&settings_path, &content)?;
    desktop_gateway::sync_desktop_gateway(&app, provider)?;
    Ok(())
}

fn ensure_helper_env_keys(provider: &ProviderConfig) -> anyhow::Result<()> {
    config::save_env_value(config::DUMMY_ENV_KEY, "local-proxy-placeholder")?;
    if let Ok(key) = config::resolve_api_key(&provider.api_key_env) {
        config::save_env_value("ANTHROPIC_API_KEY", &key)?;
        config::save_env_value("ANTHROPIC_AUTH_TOKEN", &key)?;
        config::save_env_value(&provider.api_key_env, &key)?;
    }
    Ok(())
}

pub fn restore_anthropic_official() -> anyhow::Result<()> {
    backup_claude_settings()?;
    let settings_path = paths::claude_settings_path()?;
    if !settings_path.exists() {
        return Ok(());
    }

    let mut root = load_settings_root()?;
    if let Some(env) = root.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in helper_env_keys() {
            env.remove(key);
        }
        if env.is_empty() {
            root.as_object_mut().map(|obj| obj.remove("env"));
        }
    }
    remove_helper_marker(&mut root);
    write_settings_root(&settings_path, &root)?;
    let port = AppConfig::load()
        .map(|app| app.proxy.port)
        .unwrap_or(config::DEFAULT_PORT);
    desktop_gateway::clear_desktop_gateway(port)?;
    desktop_gateway::clear_desktop_app_config()?;
    Ok(())
}

pub fn reset_desktop_defaults() -> anyhow::Result<()> {
    restore_anthropic_official()?;
    clear_helper_claude_artifacts()?;
    Ok(())
}

fn clear_helper_claude_artifacts() -> anyhow::Result<()> {
    let _ = config::save_env_value(config::DUMMY_ENV_KEY, "");
    Ok(())
}

pub fn claude_settings_exists() -> bool {
    paths::claude_settings_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

pub fn claude_settings_uses_helper() -> bool {
    read_helper_base_url()
        .ok()
        .is_some_and(|url| is_local_helper_proxy_url(&url))
}

pub fn read_helper_base_url() -> anyhow::Result<String> {
    let root = load_settings_root()?;
    let env = root
        .get("env")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("settings.json 缺少 env 块"))?;
    env.get("ANTHROPIC_BASE_URL")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("settings.json 缺少 ANTHROPIC_BASE_URL"))
}

pub fn claude_proxy_port_matches(app: &AppConfig) -> bool {
    read_helper_base_url()
        .ok()
        .is_some_and(|url| normalize_proxy_url(&url) == normalize_proxy_url(&app.proxy_base_url()))
}

fn is_local_helper_proxy_url(url: &str) -> bool {
    url.contains("127.0.0.1") || url.contains("localhost")
}

fn normalize_proxy_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

fn render_claude_settings(app: &AppConfig, provider: &ProviderConfig) -> anyhow::Result<String> {
    let mut root = load_settings_root()?;
    let api_key = desktop_gateway::GATEWAY_AUTH_TOKEN.to_string();
    let proxy_base = app.proxy_base_url();
    let model_env = build_model_env(provider);

    let env = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json 根节点必须是对象"))?
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));

    let env_map = env
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json env 必须是对象"))?;

    env_map.insert(
        "ANTHROPIC_BASE_URL".into(),
        Value::String(proxy_base.clone()),
    );
    env_map.insert("ANTHROPIC_API_KEY".into(), Value::String(api_key.clone()));
    env_map.insert(
        "ANTHROPIC_AUTH_TOKEN".into(),
        Value::String(api_key),
    );
    env_map.insert(
        "ENABLE_TOOL_SEARCH".into(),
        Value::String("true".into()),
    );
    env_map.insert(
        "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY".into(),
        Value::String("1".into()),
    );

    for (key, value) in model_env {
        env_map.insert(key, Value::String(value));
    }

    mark_helper_managed(&mut root);
    write_settings_root(&paths::claude_settings_path()?, &root)?;
    verify_written_base_url(&root, &proxy_base)?;
    Ok(serde_json::to_string_pretty(&root)?)
}

fn build_model_env(provider: &ProviderConfig) -> Vec<(String, String)> {
    let flash = model_slug_for_tier(provider, "flash");
    let pro = model_slug_for_tier(provider, "pro");
    let default = provider.default_model.clone();
    let display = provider.catalog_display_name();

    vec![
        ("ANTHROPIC_MODEL".into(), default.clone()),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), flash.clone()),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), flash.clone()),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), pro.clone()),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME".into(),
            display.clone(),
        ),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME".into(), display),
        ("ANTHROPIC_REASONING_MODEL".into(), pro),
    ]
}

fn model_slug_for_tier(provider: &ProviderConfig, tier: &str) -> String {
    let models = crate::provider::models::popular_models(&provider.id);
    if let Some(variant) = models.iter().find(|m| m.menu_tag == tier) {
        return variant.slug.to_string();
    }
    if tier == "pro" {
        if let Some(first) = models.first() {
            return first.slug.to_string();
        }
    }
    if let Some(last) = models.last() {
        return last.slug.to_string();
    }
    provider.default_model.clone()
}

fn helper_env_keys() -> Vec<&'static str> {
    vec![
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
        "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
        "ANTHROPIC_REASONING_MODEL",
        "ENABLE_TOOL_SEARCH",
        "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY",
    ]
}

fn load_settings_root() -> anyhow::Result<Value> {
    let path = paths::claude_settings_path()?;
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    let raw = std::fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    Ok(serde_json::from_str(&raw).unwrap_or(Value::Object(Map::new())))
}

fn write_settings_root(path: &std::path::Path, root: &Value) -> anyhow::Result<()> {
    let rendered = serde_json::to_string_pretty(root)?;
    config::write_atomic(path, &format!("{rendered}\n"))
}

fn mark_helper_managed(root: &mut Value) {
    if let Some(obj) = root.as_object_mut() {
        obj.insert(
            "_claude_code_helper".into(),
            Value::String(HELPER_MARKER.into()),
        );
    }
}

fn remove_helper_marker(root: &mut Value) {
    if let Some(obj) = root.as_object_mut() {
        obj.remove("_claude_code_helper");
    }
}

fn verify_written_base_url(root: &Value, expected: &str) -> anyhow::Result<()> {
    let base_url = root
        .get("env")
        .and_then(|v| v.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("写入后校验失败：缺少 ANTHROPIC_BASE_URL"))?;
    if normalize_proxy_url(base_url) != normalize_proxy_url(expected) {
        anyhow::bail!("写入后校验失败：ANTHROPIC_BASE_URL 为 {base_url}，期望 {expected}");
    }
    Ok(())
}

pub(crate) fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn sync_provider_presets(app: &mut AppConfig) {
    crate::provider::sync_builtin_presets(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;

    #[test]
    fn normalize_proxy_url_strips_trailing_slash() {
        assert_eq!(
            normalize_proxy_url("http://127.0.0.1:25573/"),
            "http://127.0.0.1:25573"
        );
    }

    #[test]
    fn build_model_env_uses_provider_variants() {
        let provider = ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/anthropic".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            default_model: "deepseek-v4-pro".into(),
            api_model: "deepseek-v4-pro".into(),
            wire_api: "anthropic".into(),
        };
        let env = build_model_env(&provider);
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert_eq!(map.get("ANTHROPIC_MODEL").map(String::as_str), Some("deepseek-v4-pro"));
        assert_eq!(
            map.get("ANTHROPIC_DEFAULT_SONNET_MODEL").map(String::as_str),
            Some("deepseek-v4-flash")
        );
    }
}
