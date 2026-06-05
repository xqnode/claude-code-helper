use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::paths;

#[allow(dead_code)]
pub const PROVIDER_ID: &str = "claude-code-helper";
pub const DUMMY_ENV_KEY: &str = "CLAUDE_CODE_HELPER_DUMMY_KEY";
pub const DEFAULT_HOST: &str = "127.0.0.1";
/// 本地代理固定端口；Claude Code 通过 ANTHROPIC_BASE_URL=http://127.0.0.1:25573 访问 Helper。
pub const DEFAULT_PORT: u16 = 25573;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key_env: String,
    /// Claude Code settings / 托盘菜单中展示的模型 slug
    pub default_model: String,
    /// 实际上游 API 的 model 名称（可与 default_model 不同）
    #[serde(default)]
    pub api_model: String,
    /// anthropic = 上游原生 Anthropic API；chat = OpenAI Chat Completions（由代理转换）
    pub wire_api: String,
}

impl ProviderConfig {
    pub fn upstream_model(&self) -> &str {
        if self.api_model.trim().is_empty() {
            &self.default_model
        } else {
            &self.api_model
        }
    }

    pub fn uses_anthropic_upstream(&self) -> bool {
        self.wire_api == "anthropic"
    }

    pub fn catalog_display_name(&self) -> String {
        match (self.id.as_str(), self.default_model.as_str()) {
            ("deepseek", "deepseek-v4-flash") => "DeepSeek V4 Flash".into(),
            ("deepseek", "deepseek-v4-pro") => "DeepSeek V4 Pro".into(),
            ("qwen", "qwen3.7-max") => "千问 3.7 Max".into(),
            ("qwen", "qwen3.7-plus") => "千问 3.7 Plus".into(),
            ("zhipu", "glm-5") => "GLM-5".into(),
            ("zhipu", "glm-5.1") => "GLM-5.1".into(),
            ("zhipu", "glm-4.7") => "GLM-4.7".into(),
            ("kimi", "kimi-k2.6") => "Kimi K2.6".into(),
            ("minimax", "minimax-m3") => "MiniMax M3".into(),
            ("mimo", "mimo-v2.5-pro") => "MiMo V2.5 Pro".into(),
            ("mimo", "mimo-v2.5") => "MiMo V2.5".into(),
            ("mimo", "mimo-v2-flash") => "MiMo V2 Flash".into(),
            ("custom", "gpt-5.5") => "GPT-5.5".into(),
            ("custom", "gpt-5.4") => "GPT-5.4".into(),
            ("custom", "gpt-5.4-mini") => "GPT-5.4 Mini".into(),
            _ => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub proxy: ProxyConfig,
    pub active: String,
    pub providers: HashMap<String, ProviderConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        for preset in crate::provider::presets::builtin_presets() {
            providers.insert(preset.id.clone(), preset);
        }
        Self {
            proxy: ProxyConfig::default(),
            active: "deepseek".to_string(),
            providers,
        }
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let path = paths::helper_config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let mut app: AppConfig = serde_json::from_str(&raw)?;
        crate::provider::sync_builtin_presets(&mut app);
        Ok(app)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        paths::ensure_helper_dirs()?;
        let path = paths::helper_config_path()?;
        let raw = serde_json::to_string_pretty(self)?;
        write_atomic(&path, &raw)
    }

    pub fn clear_all_settings() -> anyhow::Result<Self> {
        let current = Self::load().unwrap_or_default();
        let mut app = Self::default();
        app.proxy.host = current.proxy.host.clone();
        app.proxy.port = current.proxy.port;
        app.save()?;

        let env_path = paths::helper_env_path()?;
        if env_path.exists() {
            std::fs::remove_file(env_path)?;
        }
        Ok(app)
    }

    pub fn active_provider(&self) -> anyhow::Result<&ProviderConfig> {
        self.providers
            .get(&self.active)
            .ok_or_else(|| anyhow::anyhow!("未找到当前模型预设: {}", self.active))
    }

    /// Claude Code 的 ANTHROPIC_BASE_URL 不带 /v1 后缀。
    pub fn proxy_base_url(&self) -> String {
        format!("http://{}:{}", self.proxy.host, self.proxy.port)
    }
}

pub fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

pub fn validate_base_url(url: &str) -> anyhow::Result<String> {
    let normalized = normalize_base_url(url);
    if normalized.is_empty() {
        anyhow::bail!("请填写 Base URL");
    }
    if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
        anyhow::bail!("Base URL 需以 http:// 或 https:// 开头");
    }
    Ok(normalized)
}

pub fn build_upstream_client(timeout: std::time::Duration) -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .build()
        .map_err(Into::into)
}

pub fn write_atomic(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn load_env_file() -> anyhow::Result<HashMap<String, String>> {
    let path = paths::helper_env_path()?;
    let mut map = HashMap::new();
    if !path.exists() {
        return Ok(map);
    }
    for line in std::fs::read_to_string(path)?.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Ok(map)
}

pub fn save_env_value(key: &str, value: &str) -> anyhow::Result<()> {
    paths::ensure_helper_dirs()?;
    let path = paths::helper_env_path()?;
    let mut map = load_env_file()?;
    map.insert(key.to_string(), value.to_string());
    let mut lines: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    lines.sort();
    write_atomic(&path, &format!("{}\n", lines.join("\n")))
}

pub fn resolve_api_key(env_key: &str) -> anyhow::Result<String> {
    if let Ok(value) = std::env::var(env_key) {
        if !value.trim().is_empty() {
            return Ok(value.trim().to_string());
        }
    }
    let file_env = load_env_file()?;
    file_env
        .get(env_key)
        .cloned()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "未找到 API Key，请先运行: claude-code-helper env set {env_key} <your-key>"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_proxy_uses_fixed_port() {
        let config = ProxyConfig::default();
        assert_eq!(config.host, DEFAULT_HOST);
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(
            AppConfig::default().proxy_base_url(),
            format!("http://{DEFAULT_HOST}:{DEFAULT_PORT}")
        );
    }
}
