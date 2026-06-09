//! 将默认/请求中的 `reasoning` 字段翻译为各厂商 Chat API 可识别的参数。

use serde_json::{json, Value};

use crate::config::ProviderConfig;
use crate::provider::chat_reasoning::{chat_reasoning_config_for, ChatReasoningConfig};

pub fn apply_reasoning_options(chat: &mut Value, provider: &ProviderConfig) {
    let Some(config) = chat_reasoning_config_for(provider) else {
        return;
    };
    apply_reasoning_options_with_config(chat, &config);
}

pub fn apply_default_reasoning_effort(chat: &mut Value, effort: &str, provider: &ProviderConfig) {
    if chat.get("reasoning").is_some() {
        apply_reasoning_options(chat, provider);
        return;
    }
    let normalized = effort.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "" | "none" | "off" | "disabled") {
        return;
    }
    chat["reasoning"] = json!({ "effort": normalized });
    apply_reasoning_options(chat, provider);
}

fn apply_reasoning_options_with_config(chat: &mut Value, config: &ChatReasoningConfig) {
    let supports_effort = config.supports_effort.unwrap_or(false);
    let supports_thinking = config.supports_thinking.unwrap_or(false) || supports_effort;
    let Some(reasoning_enabled) = reasoning_requested(chat.get("reasoning")) else {
        return;
    };

    if supports_thinking {
        apply_thinking_param(chat, config, reasoning_enabled);
    }

    let effort_param = config
        .effort_param
        .as_deref()
        .unwrap_or("reasoning_effort")
        .trim()
        .to_ascii_lowercase();

    if !reasoning_enabled {
        if effort_param == "reasoning.effort" {
            chat["reasoning"] = json!({ "effort": "none" });
        } else {
            remove_reasoning_source(chat);
        }
        return;
    }

    if !supports_effort {
        remove_reasoning_source(chat);
        return;
    }

    let Some(effort) = chat
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(|v| v.as_str())
    else {
        remove_reasoning_source(chat);
        return;
    };

    let Some(mapped) = map_reasoning_effort(effort, config.effort_value_mode.as_deref()) else {
        remove_reasoning_source(chat);
        return;
    };

    match effort_param.as_str() {
        "reasoning_effort" => {
            chat["reasoning_effort"] = json!(mapped);
            remove_reasoning_source(chat);
        }
        "reasoning.effort" => {
            chat["reasoning"] = json!({ "effort": mapped });
        }
        _ => remove_reasoning_source(chat),
    }
}

fn apply_thinking_param(chat: &mut Value, config: &ChatReasoningConfig, reasoning_enabled: bool) {
    match config
        .thinking_param
        .as_deref()
        .unwrap_or("thinking")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "thinking" => {
            let enabled_type = config
                .thinking_type_when_enabled
                .as_deref()
                .unwrap_or("enabled");
            let mut thinking = serde_json::Map::new();
            thinking.insert(
                "type".into(),
                json!(if reasoning_enabled {
                    enabled_type
                } else {
                    "disabled"
                }),
            );
            if reasoning_enabled {
                if let Some(keep) = config.thinking_keep_when_enabled.as_ref() {
                    thinking.insert("keep".into(), json!(keep));
                }
                if let Some(clear) = config.thinking_clear_thinking_when_enabled {
                    thinking.insert("clear_thinking".into(), json!(clear));
                }
            }
            chat["thinking"] = Value::Object(thinking);
            if reasoning_enabled && config.reasoning_split_when_enabled == Some(true) {
                chat["reasoning_split"] = json!(true);
            }
        }
        "enable_thinking" => {
            chat["enable_thinking"] = json!(reasoning_enabled);
        }
        "reasoning_split" => {
            chat["reasoning_split"] = json!(reasoning_enabled);
        }
        _ => {}
    }
}

fn remove_reasoning_source(chat: &mut Value) {
    if let Some(obj) = chat.as_object_mut() {
        obj.remove("reasoning");
    }
}

fn reasoning_requested(reasoning: Option<&Value>) -> Option<bool> {
    let reasoning = reasoning?;
    if let Some(effort) = reasoning.get("effort").and_then(|v| v.as_str()) {
        return Some(!matches!(
            effort.trim().to_ascii_lowercase().as_str(),
            "none" | "off" | "disabled"
        ));
    }
    Some(!reasoning.is_null())
}

fn map_reasoning_effort(effort: &str, mode: Option<&str>) -> Option<&'static str> {
    let effort = effort.trim().to_ascii_lowercase();
    if matches!(effort.as_str(), "none" | "off" | "disabled") {
        return None;
    }

    match mode.unwrap_or("passthrough") {
        "deepseek" => match effort.as_str() {
            "max" | "xhigh" => Some("max"),
            _ => Some("high"),
        },
        "low_high" => match effort.as_str() {
            "minimal" | "low" => Some("low"),
            _ => Some("high"),
        },
        "openrouter" => match effort.as_str() {
            "max" | "xhigh" => Some("xhigh"),
            "high" => Some("high"),
            "medium" => Some("medium"),
            "low" => Some("low"),
            "minimal" => Some("minimal"),
            _ => None,
        },
        _ => match effort.as_str() {
            "minimal" => Some("minimal"),
            "low" => Some("low"),
            "medium" => Some("medium"),
            "high" => Some("high"),
            "xhigh" => Some("xhigh"),
            "max" => Some("max"),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;

    fn qwen() -> ProviderConfig {
        ProviderConfig {
            id: "qwen".into(),
            name: "千问".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            api_key_env: "DASHSCOPE_API_KEY".into(),
            default_model: "qwen3.7-max".into(),
            api_model: "qwen3.7-max".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    fn deepseek() -> ProviderConfig {
        ProviderConfig {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            default_model: "deepseek-v4-pro".into(),
            api_model: "deepseek-v4-pro".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    fn minimax() -> ProviderConfig {
        ProviderConfig {
            id: "minimax".into(),
            name: "Minimax".into(),
            base_url: "https://api.minimaxi.com/v1".into(),
            api_key_env: "MINIMAX_API_KEY".into(),
            default_model: "minimax-m3".into(),
            api_model: "MiniMax-M3".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    fn kimi() -> ProviderConfig {
        ProviderConfig {
            id: "kimi".into(),
            name: "Kimi".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            api_key_env: "MOONSHOT_API_KEY".into(),
            default_model: "kimi-k2.6".into(),
            api_model: "kimi-k2.6".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    fn zhipu() -> ProviderConfig {
        ProviderConfig {
            id: "zhipu".into(),
            name: "智谱".into(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            api_key_env: "ZHIPU_API_KEY".into(),
            default_model: "glm-5.1".into(),
            api_model: "glm-5.1".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    fn mimo() -> ProviderConfig {
        ProviderConfig {
            id: "mimo".into(),
            name: "MiMo".into(),
            base_url: "https://api.xiaomimimo.com/v1".into(),
            api_key_env: "MIMO_API_KEY".into(),
            default_model: "mimo-v2.5-pro".into(),
            api_model: "mimo-v2.5-pro".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    fn openrouter_custom() -> ProviderConfig {
        ProviderConfig {
            id: "custom".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key_env: "OPENROUTER_API_KEY".into(),
            default_model: "gpt-5.4".into(),
            api_model: "gpt-5.4".into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    #[test]
    fn apply_default_reasoning_effort_for_qwen() {
        let mut chat = json!({ "model": "qwen3.7-max", "messages": [] });
        apply_default_reasoning_effort(&mut chat, "medium", &qwen());
        assert_eq!(chat["enable_thinking"], true);
        assert!(chat.get("reasoning").is_none());
    }

    #[test]
    fn minimax_uses_adaptive_thinking_type() {
        let mut chat = json!({
            "model": "MiniMax-M3",
            "reasoning": { "effort": "high" }
        });
        apply_reasoning_options(&mut chat, &minimax());
        assert_eq!(chat["thinking"]["type"], "adaptive");
        assert_eq!(chat["reasoning_split"], true);
    }

    #[test]
    fn maps_deepseek_reasoning_effort() {
        let mut chat = json!({"model": "deepseek-v4-pro", "reasoning": {"effort": "high"}});
        apply_reasoning_options(&mut chat, &deepseek());
        assert_eq!(chat["reasoning_effort"], "high");
        assert_eq!(chat["thinking"]["type"], "enabled");
        assert!(chat.get("reasoning").is_none());
    }

    #[test]
    fn deepseek_explicit_none_disables_thinking() {
        let mut chat = json!({"model": "deepseek-v4-pro", "reasoning": {"effort": "none"}});
        apply_reasoning_options(&mut chat, &deepseek());
        assert_eq!(chat["thinking"]["type"], "disabled");
        assert!(chat.get("reasoning_effort").is_none());
    }

    #[test]
    fn maps_deepseek_max_effort_to_max() {
        let mut chat = json!({"model": "deepseek-v4-pro", "reasoning": {"effort": "max"}});
        apply_reasoning_options(&mut chat, &deepseek());
        assert_eq!(chat["reasoning_effort"], "max");
    }

    #[test]
    fn kimi_preserves_thinking_keep_for_tool_history() {
        let mut chat = json!({"model": "kimi-k2.6", "reasoning": {"effort": "high"}});
        apply_reasoning_options(&mut chat, &kimi());
        assert_eq!(chat["thinking"]["type"], "enabled");
        assert_eq!(chat["thinking"]["keep"], "all");
    }

    #[test]
    fn zhipu_uses_thinking_type_not_reasoning_effort() {
        let mut chat = json!({"model": "glm-5.1", "reasoning": {"effort": "low"}});
        apply_reasoning_options(&mut chat, &zhipu());
        assert_eq!(chat["thinking"]["type"], "enabled");
        assert_eq!(chat["thinking"]["clear_thinking"], false);
        assert!(chat.get("reasoning_effort").is_none());
    }

    #[test]
    fn mimo_uses_enable_thinking_without_unsupported_fields() {
        let mut chat = json!({"model": "mimo-v2.5-pro", "reasoning": {"effort": "high"}});
        apply_reasoning_options(&mut chat, &mimo());
        assert_eq!(chat["enable_thinking"], true);
        assert!(chat.get("thinking").is_none());
        assert!(chat.get("reasoning_split").is_none());
    }

    #[test]
    fn openrouter_maps_max_to_xhigh() {
        let mut chat = json!({"model": "gpt-5.4", "reasoning": {"effort": "max"}});
        apply_reasoning_options(&mut chat, &openrouter_custom());
        assert_eq!(chat["reasoning"]["effort"], "xhigh");
    }

    #[test]
    fn minimax_explicit_none_uses_disabled_thinking_type() {
        let mut chat = json!({"model": "MiniMax-M3", "reasoning": {"effort": "none"}});
        apply_reasoning_options(&mut chat, &minimax());
        assert_eq!(chat["thinking"]["type"], "disabled");
        assert!(chat.get("reasoning_split").is_none());
    }
}
