//! Anthropic → Chat Completions 的 reasoning 能力描述（各厂商参数形态）。

use crate::config::ProviderConfig;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatReasoningConfig {
    pub supports_thinking: Option<bool>,
    pub supports_effort: Option<bool>,
    pub thinking_param: Option<String>,
    pub effort_param: Option<String>,
    pub effort_value_mode: Option<String>,
    pub output_format: Option<String>,
    /// 多轮 tool 历史是否需补 `reasoning_content` 占位（DeepSeek/Kimi 等；千问不需要）。
    pub preserve_tool_call_reasoning: Option<bool>,
    /// `thinking.type` 在开启推理时的取值（默认 `enabled`；MiniMax M3 需 `adaptive`）。
    pub thinking_type_when_enabled: Option<String>,
    /// 开启推理时是否注入 `reasoning_split`（MiniMax M3 推荐开启）。
    pub reasoning_split_when_enabled: Option<bool>,
    /// `thinking.keep`（Kimi 多轮 tool 历史推荐 `all`）。
    pub thinking_keep_when_enabled: Option<String>,
    /// `thinking.clear_thinking`（智谱 GLM 跨轮保留推理时设为 `false`）。
    pub thinking_clear_thinking_when_enabled: Option<bool>,
}

impl ProviderConfig {
    pub fn chat_reasoning_config(&self) -> Option<ChatReasoningConfig> {
        chat_reasoning_config_for(self)
    }
}

pub fn chat_reasoning_config_for(provider: &ProviderConfig) -> Option<ChatReasoningConfig> {
    match provider.id.as_str() {
        "deepseek" if !provider.uses_anthropic_upstream() => Some(ChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".into()),
            effort_param: Some("reasoning_effort".into()),
            effort_value_mode: Some("deepseek".into()),
            output_format: Some("reasoning_content".into()),
            preserve_tool_call_reasoning: Some(true),
            ..Default::default()
        }),
        "kimi" => Some(kimi_thinking()),
        "qwen" => Some(thinking_only("enable_thinking", false)),
        "minimax" => Some(minimax_thinking()),
        "mimo" => Some(thinking_only("enable_thinking", true)),
        "zhipu" => Some(zhipu_thinking()),
        "custom" => infer_custom_reasoning_config(&provider.base_url),
        _ => None,
    }
}

fn kimi_thinking() -> ChatReasoningConfig {
    ChatReasoningConfig {
        supports_thinking: Some(true),
        supports_effort: Some(false),
        thinking_param: Some("thinking".into()),
        effort_param: Some("none".into()),
        output_format: Some("reasoning_content".into()),
        preserve_tool_call_reasoning: Some(true),
        thinking_keep_when_enabled: Some("all".into()),
        ..Default::default()
    }
}

fn zhipu_thinking() -> ChatReasoningConfig {
    ChatReasoningConfig {
        supports_thinking: Some(true),
        supports_effort: Some(false),
        thinking_param: Some("thinking".into()),
        effort_param: Some("none".into()),
        output_format: Some("reasoning_content".into()),
        preserve_tool_call_reasoning: Some(false),
        thinking_clear_thinking_when_enabled: Some(false),
        ..Default::default()
    }
}

fn minimax_thinking() -> ChatReasoningConfig {
    ChatReasoningConfig {
        supports_thinking: Some(true),
        supports_effort: Some(false),
        thinking_param: Some("thinking".into()),
        effort_param: Some("none".into()),
        output_format: Some("reasoning_content".into()),
        preserve_tool_call_reasoning: Some(true),
        thinking_type_when_enabled: Some("adaptive".into()),
        reasoning_split_when_enabled: Some(true),
        ..Default::default()
    }
}

fn thinking_only(thinking_param: &str, preserve_tool_call_reasoning: bool) -> ChatReasoningConfig {
    ChatReasoningConfig {
        supports_thinking: Some(true),
        supports_effort: Some(false),
        thinking_param: Some(thinking_param.into()),
        effort_param: Some("none".into()),
        output_format: Some("reasoning_content".into()),
        preserve_tool_call_reasoning: Some(preserve_tool_call_reasoning),
        ..Default::default()
    }
}

pub fn provider_needs_reasoning_content(provider: &ProviderConfig) -> bool {
    provider
        .chat_reasoning_config()
        .and_then(|config| config.preserve_tool_call_reasoning)
        .unwrap_or(false)
}

fn infer_custom_reasoning_config(base_url: &str) -> Option<ChatReasoningConfig> {
    let base = base_url.to_ascii_lowercase();
    if base.contains("moonshot") || base.contains("kimi") {
        return Some(kimi_thinking());
    }
    if base.contains("bigmodel") || base.contains("zhipu") {
        return Some(zhipu_thinking());
    }
    if base.contains("deepseek") {
        return Some(ChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(true),
            thinking_param: Some("thinking".into()),
            effort_param: Some("reasoning_effort".into()),
            effort_value_mode: Some("deepseek".into()),
            output_format: Some("reasoning_content".into()),
            preserve_tool_call_reasoning: Some(true),
            ..Default::default()
        });
    }
    if base.contains("openrouter") {
        return Some(ChatReasoningConfig {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".into()),
            effort_param: Some("reasoning.effort".into()),
            effort_value_mode: Some("openrouter".into()),
            output_format: Some("auto".into()),
            preserve_tool_call_reasoning: Some(false),
            ..Default::default()
        });
    }
    if base.contains("dashscope") || base.contains("aliyun") {
        return Some(thinking_only("enable_thinking", false));
    }
    if base.contains("minimax") || base.contains("minimaxi") {
        return Some(minimax_thinking());
    }
    if base.contains("mimo") || base.contains("xiaomimimo") {
        return Some(thinking_only("enable_thinking", true));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, base_url: &str, wire_api: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.into(),
            name: id.into(),
            base_url: base_url.into(),
            api_key_env: "KEY".into(),
            default_model: "model".into(),
            api_model: "model".into(),
            wire_api: wire_api.into(),
        }
    }

    #[test]
    fn deepseek_chat_enables_thinking_and_effort() {
        let config = provider("deepseek", "https://api.deepseek.com/v1", "chat")
            .chat_reasoning_config()
            .unwrap();
        assert_eq!(config.supports_thinking, Some(true));
        assert_eq!(config.supports_effort, Some(true));
    }

    #[test]
    fn deepseek_anthropic_upstream_has_no_chat_reasoning_config() {
        assert!(provider("deepseek", "https://api.deepseek.com/anthropic", "anthropic")
            .chat_reasoning_config()
            .is_none());
    }

    #[test]
    fn preserve_tool_call_reasoning_differs_for_qwen_and_mimo() {
        assert!(!provider_needs_reasoning_content(&provider(
            "qwen",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "chat"
        )));
        assert!(provider_needs_reasoning_content(&provider(
            "mimo",
            "https://api.xiaomimimo.com/v1",
            "chat"
        )));
    }
}
