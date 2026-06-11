pub mod chat_reasoning;
pub mod models;
pub mod presets;

use crate::config::ProviderConfig;

const PRESET_ORDER: &[&str] = &["deepseek", "qwen", "zhipu", "kimi", "minimax", "mimo", "custom"];

pub fn list_presets(config: &crate::config::AppConfig) -> Vec<&ProviderConfig> {
    PRESET_ORDER
        .iter()
        .filter_map(|id| config.providers.get(*id))
        .collect()
}

pub fn get_preset<'a>(
    config: &'a crate::config::AppConfig,
    id: &str,
) -> anyhow::Result<&'a ProviderConfig> {
    config.providers.get(id).ok_or_else(|| {
        anyhow::anyhow!("未知模型预设: {id}。运行 claude-code-helper list 查看可用项。")
    })
}

/// 将旧版 moonshot 预设迁移为 kimi。
pub fn migrate_legacy_providers(app: &mut crate::config::AppConfig) {
    if app.active == "moonshot" {
        app.active = "kimi".to_string();
    }
    if let Some(mut old) = app.providers.remove("moonshot") {
        old.id = "kimi".into();
        old.name = "Kimi".into();
        app.providers.insert("kimi".into(), old);
    }
}

/// 根据自定义 Base URL 推断上游协议：OpenAI 兼容走 chat，Anthropic 原生走 anthropic。
pub fn infer_custom_wire_api(base_url: &str) -> &'static str {
    let base = base_url.trim().to_ascii_lowercase();
    if base.is_empty() {
        return "anthropic";
    }
    if base.contains("/openai")
        || base.contains("openrouter")
        || base.contains("compatible-mode")
        || base.contains("integrate.api.nvidia.com")
        || base.contains("api.nvidia.com")
        || base.contains("/v1/chat")
    {
        return "chat";
    }
    if base.contains("/anthropic") {
        return "anthropic";
    }
    "anthropic"
}

/// 合并内置模型预设（新增 Minimax 等、更新显示名）。
pub fn sync_builtin_presets(app: &mut crate::config::AppConfig) {
    migrate_legacy_providers(app);
    app.providers.remove("moonshot");
    if app.active == "nvidia" {
        app.active = "deepseek".to_string();
    }
    app.providers.remove("nvidia");
    for preset in presets::builtin_presets() {
        if let Some(existing) = app.providers.get_mut(&preset.id) {
            if existing.id != "custom" && !existing.base_url_customized {
                existing.base_url = preset.base_url.clone();
            }
            existing.api_key_env = preset.api_key_env.clone();
            existing.name = preset.name.clone();
            if existing.id == "custom" {
                if !existing.base_url_customized {
                    existing.wire_api = preset.wire_api.clone();
                }
            } else {
                existing.wire_api = preset.wire_api.clone();
            }
            models::sync_model_metadata(existing);
        } else {
            app.providers.insert(preset.id.clone(), preset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[test]
    fn sync_preserves_custom_base_url() {
        let mut app = AppConfig::default();
        {
            let custom = app.providers.get_mut("custom").unwrap();
            custom.base_url = "https://relay.example.com/v1".into();
            custom.base_url_customized = true;
        }
        sync_builtin_presets(&mut app);
        assert_eq!(
            app.providers.get("custom").unwrap().base_url,
            "https://relay.example.com/v1"
        );
    }

    #[test]
    fn sync_preserves_user_modified_official_base_url() {
        let mut app = AppConfig::default();
        {
            let deepseek = app.providers.get_mut("deepseek").unwrap();
            deepseek.base_url = "https://mirror.example.com/anthropic".into();
            deepseek.base_url_customized = true;
        }
        sync_builtin_presets(&mut app);
        assert_eq!(
            app.providers.get("deepseek").unwrap().base_url,
            "https://mirror.example.com/anthropic"
        );
    }

    #[test]
    fn sync_preserves_custom_models() {
        let mut app = AppConfig::default();
        app.providers
            .get_mut("custom")
            .unwrap()
            .custom_models = vec!["my-opus".into(), "my-sonnet".into()];
        sync_builtin_presets(&mut app);
        assert_eq!(
            app.providers.get("custom").unwrap().custom_models,
            vec!["my-opus".to_string(), "my-sonnet".to_string()]
        );
    }

    #[test]
    fn infer_custom_wire_api_detects_openai_relay() {
        assert_eq!(
            infer_custom_wire_api("https://freeapi.highwayapi.ai/openai/v1"),
            "chat"
        );
        assert_eq!(
            infer_custom_wire_api("https://api.example.com/anthropic"),
            "anthropic"
        );
    }

    #[test]
    fn sync_custom_preserves_user_wire_api() {
        let mut app = AppConfig::default();
        {
            let custom = app.providers.get_mut("custom").unwrap();
            custom.base_url = "https://freeapi.highwayapi.ai/openai/v1".into();
            custom.base_url_customized = true;
            custom.wire_api = "chat".into();
        }
        sync_builtin_presets(&mut app);
        assert_eq!(app.providers.get("custom").unwrap().wire_api, "chat");
        assert_eq!(app.providers.get("custom").unwrap().name, "自定义");
    }

    #[test]
    fn sync_adds_minimax_to_legacy_config() {
        let mut app = AppConfig::default();
        app.providers.remove("minimax");
        app.providers.remove("kimi");
        sync_builtin_presets(&mut app);
        assert!(app.providers.contains_key("minimax"));
        assert!(app.providers.contains_key("kimi"));
        assert_eq!(app.providers.get("qwen").unwrap().name, "千问");
    }

    #[test]
    fn sync_removes_deprecated_nvidia_provider() {
        let mut app = AppConfig::default();
        app.active = "nvidia".to_string();
        sync_builtin_presets(&mut app);
        assert!(!app.providers.contains_key("nvidia"));
        assert_eq!(app.active, "deepseek");
    }
}
