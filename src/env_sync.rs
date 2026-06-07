use crate::config;

/// 让 Claude Code 桌面端 / CLI 能拿到 API Key：写入 settings.json，并在 Windows 写入用户级环境变量。
pub fn sync_claude_desktop_credentials(provider: &config::ProviderConfig) -> anyhow::Result<()> {
    let token = config::resolve_api_key(&provider.api_key_env)
        .or_else(|_| config::resolve_api_key(config::DUMMY_ENV_KEY))?;

    sync_windows_user_env("ANTHROPIC_API_KEY", &token)?;
    sync_windows_user_env("ANTHROPIC_AUTH_TOKEN", &token)?;
    std::env::set_var("ANTHROPIC_API_KEY", &token);
    std::env::set_var("ANTHROPIC_AUTH_TOKEN", &token);
    Ok(())
}

#[cfg(windows)]
fn sync_windows_user_env(key: &str, value: &str) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = Command::new("setx")
        .args([key, value])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| anyhow::anyhow!("无法执行 setx: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("setx {key} 失败: {stderr}");
    }
    Ok(())
}

#[cfg(not(windows))]
fn sync_windows_user_env(_key: &str, _value: &str) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
pub fn windows_user_env_is_set(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some()
        || read_windows_user_env(key).is_some()
}

#[cfg(windows)]
fn read_windows_user_env(key: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = Command::new("cmd")
        .args(["/C", &format!("reg query HKCU\\Environment /v {key}")])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_reg_query_value(&String::from_utf8_lossy(&output.stdout), key)
}

#[cfg(windows)]
pub(crate) fn parse_reg_query_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with(key) {
            continue;
        }
        let rest = line.strip_prefix(key)?.trim();
        let rest = rest.strip_prefix("REG_SZ").unwrap_or(rest).trim();
        if !rest.is_empty() {
            return Some(rest.to_string());
        }
        return line.split_whitespace().last().map(str::to_string);
    }
    None
}

#[cfg(not(windows))]
pub fn windows_user_env_is_set(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some()
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::parse_reg_query_value;

    #[cfg(windows)]
    #[test]
    fn parse_reg_query_value_reads_reg_sz_payload() {
        let text = "HKEY_CURRENT_USER\\Environment\r\n    ANTHROPIC_API_KEY    REG_SZ    sk-test-token\r\n";
        assert_eq!(
            parse_reg_query_value(text, "ANTHROPIC_API_KEY").as_deref(),
            Some("sk-test-token")
        );
    }

    #[cfg(windows)]
    #[test]
    fn parse_reg_query_value_returns_none_for_missing_key() {
        let text = "HKEY_CURRENT_USER\\Environment\r\n    OTHER_KEY    REG_SZ    value\r\n";
        assert!(parse_reg_query_value(text, "ANTHROPIC_API_KEY").is_none());
    }
}
