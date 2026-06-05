use std::path::PathBuf;

pub fn helper_dir() -> anyhow::Result<PathBuf> {
    let dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("无法定位用户主目录"))?
        .join(".claude-code-helper");
    Ok(dir)
}

pub fn helper_config_path() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("config.json"))
}

pub fn helper_env_path() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join(".env"))
}

pub fn helper_backups_dir() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("backups"))
}

pub fn helper_logs_dir() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("logs"))
}

pub fn helper_request_log_path() -> anyhow::Result<PathBuf> {
    Ok(helper_dir()?.join("request-log.sqlite"))
}

/// Claude Code 用户配置目录，默认 ~/.claude，可通过 CLAUDE_CONFIG_DIR 覆盖。
pub fn claude_config_dir() -> anyhow::Result<PathBuf> {
    std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
        .ok_or_else(|| anyhow::anyhow!("无法定位 Claude Code 配置目录"))
}

pub fn claude_settings_path() -> anyhow::Result<PathBuf> {
    Ok(claude_config_dir()?.join("settings.json"))
}

pub fn ensure_helper_dirs() -> anyhow::Result<()> {
    std::fs::create_dir_all(helper_dir()?)?;
    std::fs::create_dir_all(helper_backups_dir()?)?;
    std::fs::create_dir_all(helper_logs_dir()?)?;
    Ok(())
}
