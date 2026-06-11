#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub slug: String,
    pub display_name: String,
    pub api_model: String,
    pub context_window: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelVariant {
    pub slug: &'static str,
    pub display_name: &'static str,
    pub api_model: &'static str,
    pub context_window: u32,
    /// 托盘菜单简称，如 flash / pro
    pub menu_tag: &'static str,
}

pub fn popular_models(provider_id: &str) -> &'static [ModelVariant] {
    match provider_id {
        "deepseek" => &DEEPSEEK_MODELS,
        "qwen" => &QWEN_MODELS,
        "zhipu" => &ZHIPU_MODELS,
        "kimi" => &KIMI_MODELS,
        "minimax" => &MINIMAX_MODELS,
        "mimo" => &MIMO_MODELS,
        "custom" => &RELAY_CLAUDE_MODELS,
        _ => &[],
    }
}

pub fn list_models(provider: &crate::config::ProviderConfig) -> Vec<ModelEntry> {
    if provider.id == "custom" {
        if provider.custom_models.is_empty() {
            return relay_default_models();
        }
        return provider
            .custom_models
            .iter()
            .map(|slug| custom_model_entry(slug))
            .collect();
    }
    popular_models(&provider.id)
        .iter()
        .map(|m| static_model_entry(m))
        .collect()
}

pub fn find_model(provider_id: &str, slug: &str) -> Option<&'static ModelVariant> {
    popular_models(provider_id)
        .iter()
        .find(|m| m.slug == slug)
}

pub fn find_model_entry(provider: &crate::config::ProviderConfig, slug: &str) -> Option<ModelEntry> {
    list_models(provider)
        .into_iter()
        .find(|m| m.slug == slug)
}

/// 托盘菜单用的型号简称（如 flash、pro）。
pub fn menu_tag(provider: &crate::config::ProviderConfig) -> Option<String> {
    if provider.id == "custom" {
        return Some(short_model_tag(&provider.default_model));
    }
    find_model(&provider.id, &provider.default_model).map(|m| m.menu_tag.to_string())
}

/// 托盘菜单用，如 1M、256K。
pub fn format_context_window(tokens: u32) -> String {
    if tokens >= 1_000_000 && tokens % 1_000_000 == 0 {
        format!("{}M", tokens / 1_000_000)
    } else if tokens >= 1_000 && tokens % 1_000 == 0 {
        format!("{}K", tokens / 1_000)
    } else {
        tokens.to_string()
    }
}

pub fn tray_model_entry_label(model: &ModelEntry, active: bool) -> String {
    let label = if let Some(tokens) = model.context_window {
        format!("{} · {}", model.display_name, format_context_window(tokens))
    } else {
        model.display_name.clone()
    };
    if active {
        format!("✓ {label}")
    } else {
        label
    }
}

pub fn parse_custom_model_ids(raw: &str) -> Vec<String> {
    raw.split(['\n', ',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn normalize_custom_models(raw: &str) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    for part in parse_custom_model_ids(raw) {
        validate_custom_model_id(&part)?;
        if !out.iter().any(|existing| existing == &part) {
            out.push(part);
        }
    }
    if out.len() > MAX_CUSTOM_MODELS {
        anyhow::bail!("最多支持 {MAX_CUSTOM_MODELS} 个模型");
    }
    Ok(out)
}

pub fn validate_custom_model_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty() {
        anyhow::bail!("模型 ID 不能为空");
    }
    if id.len() > MAX_CUSTOM_MODEL_ID_LEN {
        anyhow::bail!("模型 ID 过长（最多 {MAX_CUSTOM_MODEL_ID_LEN} 字符）");
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        anyhow::bail!("模型 ID 不能为空");
    };
    if !first.is_ascii_alphanumeric() {
        anyhow::bail!("模型 ID 需以字母或数字开头");
    }
    if !id.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/')
    }) {
        anyhow::bail!("模型 ID 仅支持字母、数字、点、下划线、连字符、斜杠");
    }
    if id.contains("//") {
        anyhow::bail!("模型 ID 不能包含连续的斜杠");
    }
    Ok(())
}

const MAX_CUSTOM_MODELS: usize = 20;
const MAX_CUSTOM_MODEL_ID_LEN: usize = 128;

const DEEPSEEK_MODELS: &[ModelVariant] = &[
    ModelVariant {
        slug: "deepseek-v4-pro",
        display_name: "DeepSeek V4 Pro（旗舰）",
        api_model: "deepseek-v4-pro",
        context_window: 1_000_000,
        menu_tag: "pro",
    },
    ModelVariant {
        slug: "deepseek-v4-flash",
        display_name: "DeepSeek V4 Flash",
        api_model: "deepseek-v4-flash",
        context_window: 1_000_000,
        menu_tag: "flash",
    },
];

const QWEN_MODELS: &[ModelVariant] = &[
    ModelVariant {
        slug: "qwen3.7-max",
        display_name: "千问 3.7 Max（旗舰）",
        api_model: "qwen3.7-max",
        context_window: 1_000_000,
        menu_tag: "max",
    },
    ModelVariant {
        slug: "qwen3.7-plus",
        display_name: "千问 3.7 Plus（多模态·1M）",
        api_model: "qwen3.7-plus",
        context_window: 1_000_000,
        menu_tag: "plus",
    },
];

const ZHIPU_MODELS: &[ModelVariant] = &[
    ModelVariant {
        slug: "glm-5.1",
        display_name: "GLM-5.1（旗舰）",
        api_model: "glm-5.1",
        context_window: 200_000,
        menu_tag: "5.1",
    },
    ModelVariant {
        slug: "glm-5",
        display_name: "GLM-5",
        api_model: "glm-5",
        context_window: 200_000,
        menu_tag: "glm-5",
    },
    ModelVariant {
        slug: "glm-4.7",
        display_name: "GLM-4.7",
        api_model: "glm-4.7",
        context_window: 200_000,
        menu_tag: "4.7",
    },
];

const KIMI_MODELS: &[ModelVariant] = &[
    ModelVariant {
        slug: "kimi-k2.6",
        display_name: "Kimi K2.6（旗舰）",
        api_model: "kimi-k2.6",
        context_window: 256_000,
        menu_tag: "k2.6",
    },
];

const MINIMAX_MODELS: &[ModelVariant] = &[
    ModelVariant {
        slug: "minimax-m3",
        display_name: "MiniMax M3（旗舰·1M）",
        api_model: "MiniMax-M3",
        context_window: 1_000_000,
        menu_tag: "m3",
    },
];

const MIMO_MODELS: &[ModelVariant] = &[
    ModelVariant {
        slug: "mimo-v2.5-pro",
        display_name: "MiMo V2.5 Pro（旗舰·1M）",
        api_model: "mimo-v2.5-pro",
        context_window: 1_000_000,
        menu_tag: "pro",
    },
    ModelVariant {
        slug: "mimo-v2.5",
        display_name: "MiMo V2.5（全模态·1M）",
        api_model: "mimo-v2.5",
        context_window: 1_000_000,
        menu_tag: "2.5",
    },
    ModelVariant {
        slug: "mimo-v2-flash",
        display_name: "MiMo V2 Flash（256K）",
        api_model: "mimo-v2-flash",
        context_window: 256_000,
        menu_tag: "flash",
    },
];

const RELAY_CLAUDE_MODELS: &[ModelVariant] = &[
    ModelVariant {
        slug: "claude-opus-4-8",
        display_name: "Claude Opus 4.8（旗舰）",
        api_model: "claude-opus-4-8",
        context_window: 200_000,
        menu_tag: "opus-4.8",
    },
    ModelVariant {
        slug: "claude-opus-4-7",
        display_name: "Claude Opus 4.7",
        api_model: "claude-opus-4-7",
        context_window: 200_000,
        menu_tag: "opus-4.7",
    },
    ModelVariant {
        slug: "claude-sonnet-4-6",
        display_name: "Claude Sonnet 4.6",
        api_model: "claude-sonnet-4-6",
        context_window: 200_000,
        menu_tag: "sonnet-4.6",
    },
];

pub fn is_relay_default_slug(slug: &str) -> bool {
    RELAY_CLAUDE_MODELS.iter().any(|m| m.slug == slug)
}

pub fn apply_model_variant(
    provider: &mut crate::config::ProviderConfig,
    slug: &str,
) -> anyhow::Result<()> {
    let variant = find_model_entry(provider, slug).ok_or_else(|| {
        anyhow::anyhow!("未知模型: {slug}")
    })?;
    provider.default_model = variant.slug;
    provider.api_model = variant.api_model;
    Ok(())
}

fn migrate_legacy_model_slug(provider: &mut crate::config::ProviderConfig) {
    let new_slug = match (provider.id.as_str(), provider.default_model.as_str()) {
        ("deepseek", "deepseek-chat") => "deepseek-v4-flash",
        ("deepseek", "deepseek-reasoner") => "deepseek-v4-pro",
        ("qwen", "qwen-max") => "qwen3.7-max",
        ("qwen", "qwen-turbo" | "qwen-plus" | "qwen-long") => "qwen3.7-plus",
        ("zhipu", "glm-4-plus" | "glm-4-air" | "glm-4-long" | "glm-4-flash") => "glm-5.1",
        ("kimi", slug) if slug == "kimi-k2.5" || slug.starts_with("moonshot-v1") => "kimi-k2.6",
        ("minimax", "abab6.5s-chat" | "abab6.5g-chat" | "minimax-m2.7" | "minimax-m2.5") => {
            "minimax-m3"
        }
        ("mimo", "mimo-v2-pro") => "mimo-v2.5-pro",
        ("mimo", "mimo-v2-omni") => "mimo-v2.5",
        ("mimo", slug) if slug.starts_with("mimo-v1") => "mimo-v2-flash",
        ("custom", "gpt-5.5" | "gpt-4o") => "claude-opus-4-8",
        ("custom", "gpt-5.4") => "claude-opus-4-7",
        ("custom", "gpt-5.4-mini") => "claude-sonnet-4-6",
        _ => return,
    };
    provider.default_model = new_slug.to_string();
}

pub fn ensure_valid_model(provider: &mut crate::config::ProviderConfig) {
    migrate_legacy_model_slug(provider);
    if find_model_entry(provider, &provider.default_model).is_some() {
        if provider.id == "custom" {
            provider.api_model = provider.default_model.clone();
        } else if let Some(variant) = find_model(&provider.id, &provider.default_model) {
            provider.api_model = variant.api_model.to_string();
        }
        return;
    }
    if let Some(first) = list_models(provider).first() {
        provider.default_model = first.slug.clone();
        provider.api_model = first.api_model.clone();
    }
}

pub fn sync_model_metadata(provider: &mut crate::config::ProviderConfig) {
    ensure_valid_model(provider);
    if provider.id == "custom" {
        if find_model_entry(provider, &provider.default_model).is_some() {
            provider.api_model = provider.default_model.clone();
        }
        return;
    }
    if let Some(variant) = find_model(&provider.id, &provider.default_model) {
        provider.api_model = variant.api_model.to_string();
    }
}

pub fn model_for_tier(provider: &crate::config::ProviderConfig, tier: &str) -> String {
    if provider.id == "custom" || popular_models(&provider.id).is_empty() {
        let models = list_models(provider);
        return match tier {
            "pro" => models
                .first()
                .map(|m| m.api_model.clone())
                .unwrap_or_else(|| provider.default_model.clone()),
            _ => models
                .last()
                .map(|m| m.api_model.clone())
                .unwrap_or_else(|| provider.default_model.clone()),
        };
    }

    let models = popular_models(&provider.id);
    if let Some(variant) = models.iter().find(|m| m.menu_tag == tier) {
        return variant.api_model.to_string();
    }
    if tier == "pro" {
        if let Some(first) = models.first() {
            return first.api_model.to_string();
        }
    }
    if let Some(last) = models.last() {
        return last.api_model.to_string();
    }
    provider.upstream_model().to_string()
}

pub fn label_for_tier(provider: &crate::config::ProviderConfig, tier: &str) -> String {
    if provider.id == "custom" || popular_models(&provider.id).is_empty() {
        let models = list_models(provider);
        return match tier {
            "pro" => models
                .first()
                .map(|m| m.slug.clone())
                .unwrap_or_else(|| provider.default_model.clone()),
            _ => models
                .last()
                .map(|m| m.slug.clone())
                .unwrap_or_else(|| provider.default_model.clone()),
        };
    }

    let models = popular_models(&provider.id);
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

pub fn provider_supports_1m(provider: &crate::config::ProviderConfig) -> bool {
    list_models(provider)
        .iter()
        .filter_map(|m| m.context_window)
        .any(|tokens| tokens >= 1_000_000)
}

fn relay_default_models() -> Vec<ModelEntry> {
    RELAY_CLAUDE_MODELS
        .iter()
        .map(|m| static_model_entry(m))
        .collect()
}

fn static_model_entry(model: &ModelVariant) -> ModelEntry {
    ModelEntry {
        slug: model.slug.to_string(),
        display_name: model.display_name.to_string(),
        api_model: model.api_model.to_string(),
        context_window: Some(model.context_window),
    }
}

fn custom_model_entry(slug: &str) -> ModelEntry {
    ModelEntry {
        slug: slug.to_string(),
        display_name: slug.to_string(),
        api_model: slug.to_string(),
        context_window: None,
    }
}

fn short_model_tag(slug: &str) -> String {
    slug.rsplit(['-', '.', '/'])
        .next()
        .unwrap_or(slug)
        .chars()
        .take(12)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;

    fn provider(id: &str, model: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.into(),
            name: id.into(),
            base_url: "https://example.com/v1".into(),
            api_key_env: "KEY".into(),
            default_model: model.into(),
            api_model: model.into(),
            wire_api: "chat".into(),
            base_url_customized: false,
            custom_models: Vec::new(),
        }
    }

    #[test]
    fn each_provider_lists_only_core_models() {
        assert_eq!(popular_models("deepseek").len(), 2);
        assert_eq!(popular_models("qwen").len(), 2);
        assert_eq!(popular_models("zhipu").len(), 3);
        assert_eq!(popular_models("kimi").len(), 1);
        assert_eq!(popular_models("minimax").len(), 1);
        assert_eq!(popular_models("mimo").len(), 3);
        assert_eq!(popular_models("custom").len(), 3);
    }

    #[test]
    fn menu_tags_are_defined_for_core_models() {
        assert_eq!(find_model("deepseek", "deepseek-v4-pro").unwrap().menu_tag, "pro");
        assert_eq!(find_model("qwen", "qwen3.7-plus").unwrap().menu_tag, "plus");
    }

    #[test]
    fn tray_model_label_includes_context() {
        let model = find_model("deepseek", "deepseek-v4-flash").unwrap();
        assert_eq!(
            tray_model_entry_label(&static_model_entry(model), true),
            "✓ DeepSeek V4 Flash · 1M"
        );
        let glm = find_model("zhipu", "glm-5.1").unwrap();
        assert_eq!(
            tray_model_entry_label(&static_model_entry(glm), false),
            "GLM-5.1（旗舰） · 200K"
        );
    }

    #[test]
    fn format_context_window_labels() {
        assert_eq!(format_context_window(1_000_000), "1M");
        assert_eq!(format_context_window(256_000), "256K");
        assert_eq!(format_context_window(128_000), "128K");
    }

    #[test]
    fn relay_models_use_claude_ids() {
        let models = popular_models("custom");
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].slug, "claude-opus-4-8");
        assert_eq!(models[1].slug, "claude-opus-4-7");
        assert_eq!(models[2].slug, "claude-sonnet-4-6");
    }

    #[test]
    fn custom_models_override_defaults() {
        let mut p = provider("custom", "claude-opus-4-8");
        p.custom_models = vec![
            "my-opus".into(),
            "my-sonnet".into(),
        ];
        let models = list_models(&p);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].slug, "my-opus");
        assert_eq!(models[1].slug, "my-sonnet");
    }

    #[test]
    fn parse_custom_models_supports_newlines_and_commas() {
        let parsed = parse_custom_model_ids("claude-opus-4-8\nclaude-sonnet-4-6, claude-haiku-4-5");
        assert_eq!(
            parsed,
            vec![
                "claude-opus-4-8".to_string(),
                "claude-sonnet-4-6".to_string(),
                "claude-haiku-4-5".to_string(),
            ]
        );
    }

    #[test]
    fn normalize_custom_models_deduplicates() {
        let models = normalize_custom_models("a\na\nb").unwrap();
        assert_eq!(models, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn normalize_custom_models_accepts_namespaced_ids() {
        let models = normalize_custom_models("deepseek-ai/deepseek-v4-pro").unwrap();
        assert_eq!(models, vec!["deepseek-ai/deepseek-v4-pro".to_string()]);
    }

    #[test]
    fn migrates_deprecated_and_legacy_slugs() {
        let cases = [
            ("deepseek", "deepseek-chat", "deepseek-v4-flash"),
            ("deepseek", "deepseek-reasoner", "deepseek-v4-pro"),
            ("qwen", "qwen-plus", "qwen3.7-plus"),
            ("zhipu", "glm-4-flash", "glm-5.1"),
            ("kimi", "moonshot-v1-128k", "kimi-k2.6"),
            ("minimax", "minimax-m2.5", "minimax-m3"),
            ("custom", "gpt-5.5", "claude-opus-4-8"),
            ("custom", "gpt-5.4-mini", "claude-sonnet-4-6"),
        ];

        for (id, old, expected) in cases {
            let mut p = provider(id, old);
            ensure_valid_model(&mut p);
            assert_eq!(p.default_model, expected, "{id}/{old}");
        }
    }
}
