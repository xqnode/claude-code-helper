use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config::{self, AppConfig, ProviderConfig};

pub const GATEWAY_AUTH_TOKEN: &str = "PROXY_MANAGED";
pub const DESKTOP_ROLE_HAIKU: &str = "claude-haiku-4-5";
pub const DESKTOP_ROLE_SONNET: &str = "claude-sonnet-4-6";
pub const DESKTOP_ROLE_OPUS: &str = "claude-opus-4-8";
const HELPER_ENTRY_NAME: &str = "Claude Code Helper";

fn local_claude_3p_dir() -> anyhow::Result<PathBuf> {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|_| {
            dirs::data_local_dir().ok_or_else(|| anyhow::anyhow!("无法定位 LOCALAPPDATA"))
        })?;
    Ok(base.join("Claude-3p"))
}

/// Claude Desktop Cowork 读取的 3P Gateway 配置目录。
pub fn config_library_dir() -> anyhow::Result<PathBuf> {
    Ok(local_claude_3p_dir()?.join("configLibrary"))
}

fn claude_desktop_config_path() -> anyhow::Result<PathBuf> {
    Ok(local_claude_3p_dir()?.join("claude_desktop_config.json"))
}

pub fn gateway_config_id(port: u16) -> String {
    format!("00000000-0000-4000-8000-000000{:06}", port as u32 * 10)
}

pub fn desktop_gateway_base_url(app: &AppConfig) -> String {
    format!("{}/claude-desktop", app.proxy_base_url())
}

pub fn sync_desktop_gateway(app: &AppConfig, provider: &ProviderConfig) -> anyhow::Result<()> {
    let dir = config_library_dir()?;
    std::fs::create_dir_all(&dir)?;
    backup_config_library(&dir)?;

    let config_id = gateway_config_id(app.proxy.port);
    let gateway_url = desktop_gateway_base_url(app);
    let profile = build_gateway_profile(provider, &gateway_url);
    let profile_path = dir.join(format!("{config_id}.json"));
    config::write_atomic(
        &profile_path,
        &format!("{}\n", serde_json::to_string_pretty(&profile)?),
    )?;

    let meta = json!({
        "appliedId": config_id,
        "entries": [{
            "id": config_id,
            "name": HELPER_ENTRY_NAME
        }]
    });
    config::write_atomic(
        &dir.join("_meta.json"),
        &format!("{}\n", serde_json::to_string_pretty(&meta)?),
    )?;

    cleanup_stale_gateway_profiles(&dir, &config_id)?;
    sync_desktop_app_config()?;
    tracing::debug!(
        "已同步 Claude Desktop 双通道 Gateway（Code: {}，Cowork: {gateway_url}）",
        app.proxy_base_url()
    );
    Ok(())
}

/// 启用 Claude Desktop 第三方推理模式，让 Code / Cowork 都读取 Gateway 配置。
pub fn sync_desktop_app_config() -> anyhow::Result<()> {
    let path = claude_desktop_config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut root = if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("claude_desktop_config.json 根节点必须是对象"))?;
    obj.insert("deploymentMode".into(), Value::String("3p".into()));

    config::write_atomic(&path, &format!("{}\n", serde_json::to_string_pretty(&root)?))?;
    Ok(())
}

pub fn clear_desktop_app_config() -> anyhow::Result<()> {
    let path = match claude_desktop_config_path() {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };
    if !path.exists() {
        return Ok(());
    }

    let raw = std::fs::read_to_string(&path)?;
    let Ok(mut root) = serde_json::from_str::<Value>(&raw) else {
        return Ok(());
    };
    if let Some(obj) = root.as_object_mut() {
        obj.remove("deploymentMode");
    }
    config::write_atomic(&path, &format!("{}\n", serde_json::to_string_pretty(&root)?))?;
    Ok(())
}

pub fn clear_desktop_gateway(port: u16) -> anyhow::Result<()> {
    let dir = match config_library_dir() {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };
    if !dir.exists() {
        return Ok(());
    }

    let config_id = gateway_config_id(port);
    let profile_path = dir.join(format!("{config_id}.json"));
    if profile_path.exists() {
        std::fs::remove_file(&profile_path)?;
    }

    let meta_path = dir.join("_meta.json");
    if meta_path.exists() {
        let raw = std::fs::read_to_string(&meta_path)?;
        if let Ok(mut meta) = serde_json::from_str::<Value>(&raw) {
            if meta.get("appliedId").and_then(|v| v.as_str()) == Some(config_id.as_str()) {
                if let Some(obj) = meta.as_object_mut() {
                    obj.remove("appliedId");
                    obj.insert("entries".into(), Value::Array(vec![]));
                }
                config::write_atomic(
                    &meta_path,
                    &format!("{}\n", serde_json::to_string_pretty(&meta)?),
                )?;
            }
        }
    }
    Ok(())
}

pub fn read_desktop_gateway_base_url() -> anyhow::Result<String> {
    let dir = config_library_dir()?;
    let meta_path = dir.join("_meta.json");
    if !meta_path.exists() {
        anyhow::bail!("Claude Desktop Gateway 未配置");
    }
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;
    let applied_id = meta
        .get("appliedId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Claude Desktop Gateway 未应用配置"))?;
    let profile_path = dir.join(format!("{applied_id}.json"));
    if !profile_path.exists() {
        anyhow::bail!("Claude Desktop Gateway 配置文件不存在");
    }
    let profile: Value = serde_json::from_str(&std::fs::read_to_string(&profile_path)?)?;
    profile
        .get("inferenceGatewayBaseUrl")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("Gateway 配置缺少 inferenceGatewayBaseUrl"))
}

pub fn desktop_gateway_matches(app: &AppConfig) -> bool {
    read_desktop_gateway_base_url()
        .ok()
        .is_some_and(|url| normalize_gateway_url(&url) == normalize_gateway_url(&desktop_gateway_base_url(app)))
}

pub fn desktop_app_uses_third_party_mode() -> bool {
    let Ok(path) = claude_desktop_config_path() else {
        return false;
    };
    if !path.exists() {
        return false;
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<Value>(&raw) else {
        return false;
    };
    root.get("deploymentMode")
        .and_then(|v| v.as_str())
        .is_some_and(|mode| mode.eq_ignore_ascii_case("3p"))
}

pub fn request_uses_desktop_roles(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    let Some(model) = value.get("model").and_then(|v| v.as_str()) else {
        return false;
    };
    is_desktop_role_model(model)
}

pub fn is_desktop_role_model(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.starts_with("claude-")
        && (lower.contains("haiku") || lower.contains("sonnet") || lower.contains("opus"))
}

/// Map legacy Anthropic aliases (e.g. `sonnet[1m]`) to Gateway role model IDs.
pub fn normalize_settings_model_for_gateway(current: &str) -> String {
    let trimmed = current.trim();
    if trimmed.is_empty() {
        return DESKTOP_ROLE_SONNET.to_string();
    }

    let base = trimmed.split('[').next().unwrap_or(trimmed).trim();
    if base == DESKTOP_ROLE_HAIKU {
        return DESKTOP_ROLE_HAIKU.to_string();
    }
    if base == DESKTOP_ROLE_SONNET {
        return DESKTOP_ROLE_SONNET.to_string();
    }
    if base == DESKTOP_ROLE_OPUS {
        return DESKTOP_ROLE_OPUS.to_string();
    }

    let lower = base.to_ascii_lowercase();
    if lower.contains("haiku") {
        return DESKTOP_ROLE_HAIKU.to_string();
    }
    if lower.contains("opus") {
        return DESKTOP_ROLE_OPUS.to_string();
    }
    if lower.contains("sonnet") {
        return DESKTOP_ROLE_SONNET.to_string();
    }

    let lower_full = trimmed.to_ascii_lowercase();
    if lower_full.starts_with("haiku") {
        return DESKTOP_ROLE_HAIKU.to_string();
    }
    if lower_full.starts_with("opus") {
        return DESKTOP_ROLE_OPUS.to_string();
    }
    if lower_full.starts_with("sonnet") {
        return DESKTOP_ROLE_SONNET.to_string();
    }

    DESKTOP_ROLE_SONNET.to_string()
}

/// Gateway env vars must use Claude role IDs so Desktop validation passes.
pub fn gateway_role_for_default_model(provider: &ProviderConfig) -> String {
    let default = provider.default_model.as_str();
    if default == upstream_model_for_tier(provider, "pro") {
        return DESKTOP_ROLE_OPUS.to_string();
    }
    if default.contains("haiku") {
        return DESKTOP_ROLE_HAIKU.to_string();
    }
    if default.contains("opus") {
        return DESKTOP_ROLE_OPUS.to_string();
    }
    DESKTOP_ROLE_SONNET.to_string()
}

pub fn build_inference_models(provider: &ProviderConfig) -> Vec<Value> {
    let (haiku, pro, sonnet) = display_labels_for_desktop_roles(provider);
    let supports_1m = provider_supports_1m(provider);

    vec![
        json!({
            "labelOverride": haiku,
            "name": DESKTOP_ROLE_HAIKU
        }),
        json!({
            "labelOverride": pro,
            "name": DESKTOP_ROLE_OPUS,
            "supports1m": supports_1m
        }),
        json!({
            "labelOverride": sonnet,
            "name": DESKTOP_ROLE_SONNET,
            "supports1m": supports_1m
        }),
    ]
}

pub fn resolve_custom_upstream_model(requested: &str, provider: &ProviderConfig) -> String {
    if provider.id != "custom" || provider.uses_anthropic_upstream() {
        return requested.to_string();
    }
    if crate::provider::models::list_models(provider)
        .into_iter()
        .any(|m| m.slug == requested || m.api_model == requested)
    {
        return requested.to_string();
    }
    if is_desktop_role_model(requested) {
        return map_desktop_model(requested, provider);
    }
    if crate::provider::models::is_relay_default_slug(requested) {
        return map_relay_slug_to_upstream(requested, provider);
    }
    provider.upstream_model().to_string()
}

fn map_relay_slug_to_upstream(requested: &str, provider: &ProviderConfig) -> String {
    match requested {
        "claude-opus-4-8" | "claude-opus-4-7" => upstream_model_for_tier(provider, "pro"),
        _ => upstream_model_for_tier(provider, "flash"),
    }
}

pub fn map_desktop_model(requested: &str, provider: &ProviderConfig) -> String {
    let lower = requested.to_ascii_lowercase();
    if lower.contains("haiku") {
        return upstream_model_for_tier(provider, "flash");
    }
    if lower.contains("opus") {
        return upstream_model_for_tier(provider, "pro");
    }
    if lower.contains("sonnet") {
        return upstream_model_for_tier(provider, "flash");
    }
    provider.upstream_model().to_string()
}

pub fn rewrite_request_model(body: &[u8], provider: &ProviderConfig) -> anyhow::Result<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(body)?;
    let requested = value
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(DESKTOP_ROLE_SONNET);
    let mapped = map_desktop_model(requested, provider);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("model".into(), Value::String(mapped));
    }
    Ok(serde_json::to_vec(&value)?)
}

fn build_gateway_profile(provider: &ProviderConfig, gateway_url: &str) -> Value {
    json!({
        "coworkEgressAllowedHosts": ["*"],
        "disableDeploymentModeChooser": true,
        "inferenceGatewayApiKey": GATEWAY_AUTH_TOKEN,
        "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayBaseUrl": gateway_url,
        "inferenceModels": build_inference_models(provider),
        "inferenceProvider": "gateway"
    })
}

fn backup_config_library(dir: &Path) -> anyhow::Result<()> {
    let meta = dir.join("_meta.json");
    if !meta.exists() {
        return Ok(());
    }
    crate::paths::ensure_helper_dirs()?;
    let backup_dir = crate::paths::helper_backups_dir()?;
    let stamp = super::chrono_like_timestamp();
    let backup = backup_dir.join(format!("claude-desktop-gateway.{stamp}.bak"));
    std::fs::create_dir_all(&backup)?;
    backup_meta_tree(dir, &backup)?;
    Ok(())
}

fn backup_meta_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let target = dst.join(&name);
        if file_type.is_dir() {
            std::fs::create_dir_all(&target)?;
            backup_meta_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn cleanup_stale_gateway_profiles(dir: &Path, keep_id: &str) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name == "_meta.json" || !name.ends_with(".json") {
            continue;
        }
        let id = name.trim_end_matches(".json");
        if id != keep_id {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

fn normalize_gateway_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

fn display_label_for_tier(provider: &ProviderConfig, tier: &str) -> String {
    crate::provider::models::label_for_tier(provider, tier)
}

/// Haiku / Sonnet 常映射到同一上游型号，给 Desktop 下拉菜单加角色后缀避免重复。
fn display_labels_for_desktop_roles(provider: &ProviderConfig) -> (String, String, String) {
    let haiku_base = display_label_for_tier(provider, "flash");
    let sonnet_base = display_label_for_tier(provider, "flash");
    let pro = display_label_for_tier(provider, "pro");

    let shared_flash = haiku_base == sonnet_base;
    let haiku = if shared_flash {
        format!("{haiku_base} · Fast")
    } else {
        haiku_base
    };
    let sonnet = if shared_flash {
        format!("{sonnet_base} · Default")
    } else {
        sonnet_base
    };

    (haiku, pro, sonnet)
}

fn upstream_model_for_tier(provider: &ProviderConfig, tier: &str) -> String {
    crate::provider::models::model_for_tier(provider, tier)
}

fn provider_supports_1m(provider: &ProviderConfig) -> bool {
    crate::provider::models::provider_supports_1m(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_config_id_follows_cc_switch_pattern() {
        assert_eq!(
            gateway_config_id(15721),
            "00000000-0000-4000-8000-000000157210"
        );
        assert_eq!(
            gateway_config_id(25573),
            "00000000-0000-4000-8000-000000255730"
        );
    }

    #[test]
    fn normalizes_legacy_sonnet_alias_for_gateway() {
        assert_eq!(
            normalize_settings_model_for_gateway("sonnet[1m]"),
            DESKTOP_ROLE_SONNET
        );
        assert_eq!(
            normalize_settings_model_for_gateway("claude-opus-4-7"),
            DESKTOP_ROLE_OPUS
        );
        assert_eq!(
            normalize_settings_model_for_gateway("claude-sonnet-4-6[1m]"),
            DESKTOP_ROLE_SONNET
        );
        assert_eq!(
            normalize_settings_model_for_gateway(DESKTOP_ROLE_OPUS),
            DESKTOP_ROLE_OPUS
        );
    }

    #[test]
    fn detects_desktop_role_models() {
        let body = br#"{"model":"claude-sonnet-4-6","messages":[]}"#;
        assert!(request_uses_desktop_roles(body));
        let body = br#"{"model":"deepseek-v4-flash","messages":[]}"#;
        assert!(!request_uses_desktop_roles(body));
    }

    #[test]
    fn distinct_labels_when_haiku_and_sonnet_share_flash_tier() {
        let provider = ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/anthropic".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            default_model: "deepseek-v4-pro".into(),
            api_model: "deepseek-v4-pro".into(),
            wire_api: "anthropic".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        };
        let models = build_inference_models(&provider);
        assert_eq!(models[0]["labelOverride"], "deepseek-v4-flash · Fast");
        assert_eq!(models[1]["labelOverride"], "deepseek-v4-pro");
        assert_eq!(models[2]["labelOverride"], "deepseek-v4-flash · Default");
    }

    #[test]
    fn maps_desktop_roles_to_upstream_models() {
        let provider = ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/anthropic".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            default_model: "deepseek-v4-pro".into(),
            api_model: "deepseek-v4-pro".into(),
            wire_api: "anthropic".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        };
        assert_eq!(
            map_desktop_model(DESKTOP_ROLE_SONNET, &provider),
            "deepseek-v4-flash"
        );
        assert_eq!(
            map_desktop_model(DESKTOP_ROLE_OPUS, &provider),
            "deepseek-v4-pro"
        );
    }

    #[test]
    fn custom_nvidia_maps_claude_roles_to_namespaced_model() {
        let provider = ProviderConfig {
            id: "custom".into(),
            name: "自定义".into(),
            base_url: "https://integrate.api.nvidia.com/v1".into(),
            api_key_env: "CUSTOM_API_KEY".into(),
            default_model: "deepseek-ai/deepseek-v4-pro".into(),
            api_model: "deepseek-ai/deepseek-v4-pro".into(),
            wire_api: "chat".into(),
            base_url_customized: true,
            custom_models: vec!["deepseek-ai/deepseek-v4-pro".into()],
        };
        assert_eq!(
            resolve_custom_upstream_model("claude-opus-4-8", &provider),
            "deepseek-ai/deepseek-v4-pro"
        );
        assert_eq!(
            resolve_custom_upstream_model("deepseek-ai/deepseek-v4-pro", &provider),
            "deepseek-ai/deepseek-v4-pro"
        );
    }
}
