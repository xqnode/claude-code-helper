use std::sync::Arc;

use tokio::sync::RwLock;

use crate::claude;
use crate::config::AppConfig;
use crate::provider;
use crate::proxy::ProxyState;

pub async fn switch_provider(
    config: &Arc<RwLock<AppConfig>>,
    proxy: &Arc<ProxyState>,
    provider_id: &str,
) -> anyhow::Result<()> {
    let model_slug = {
        let app = config.read().await;
        let provider = provider::get_preset(&app, provider_id)?;
        provider.default_model.clone()
    };
    switch_provider_model(config, proxy, provider_id, &model_slug).await
}

pub async fn switch_provider_model(
    config: &Arc<RwLock<AppConfig>>,
    proxy: &Arc<ProxyState>,
    provider_id: &str,
    model_slug: &str,
) -> anyhow::Result<()> {
    let mut app = config.write().await;
    provider::get_preset(&app, provider_id)?;
    app.active = provider_id.to_string();

    let provider = app
        .providers
        .get_mut(provider_id)
        .ok_or_else(|| anyhow::anyhow!("未知模型预设: {provider_id}"))?;
    provider::models::apply_model_variant(provider, model_slug)?;
    app.save()?;
    claude::inject_proxy_config(&app)?;

    let mut proxy_cfg = proxy.config.write().await;
    *proxy_cfg = app.clone();
    drop(proxy_cfg);

    let name = app.active_provider()?.name.clone();
    tracing::info!("已切换模型: {name} · {model_slug}");
    Ok(())
}

pub fn open_helper_dir() -> anyhow::Result<()> {
    let dir = crate::paths::helper_dir()?;
    crate::paths::ensure_helper_dirs()?;
    open_in_explorer(&dir)
}

pub fn open_claude_dir() -> anyhow::Result<()> {
    let dir = crate::paths::claude_config_dir()?;
    std::fs::create_dir_all(&dir)?;
    open_in_explorer(&dir)
}

fn open_in_explorer(path: &std::path::Path) -> anyhow::Result<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|e| anyhow::anyhow!("无法打开资源管理器: {e}"))?;
    Ok(())
}

pub async fn resync_claude(
    config: &Arc<RwLock<AppConfig>>,
    _proxy: &Arc<ProxyState>,
) -> anyhow::Result<()> {
    let app = config.read().await.clone();
    claude::inject_proxy_config(&app)?;
    kill_claude_desktop()?;
    tracing::info!(
        "已重新同步 Claude Code + Cowork Gateway 配置，并已退出 Claude 桌面端，请重新打开"
    );
    Ok(())
}

pub async fn restore_anthropic() -> anyhow::Result<()> {
    claude::restore_anthropic_official()?;
    tracing::info!("已恢复 Anthropic 官方配置");
    Ok(())
}

pub fn kill_claude_desktop() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        for exe in ["Claude.exe", "claude.exe"] {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", exe])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    Ok(())
}

pub async fn kill_claude_and_reset_defaults() -> anyhow::Result<()> {
    kill_claude_desktop()?;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    claude::reset_desktop_defaults()?;
    tracing::info!("已彻底退出 Claude Code 并恢复默认配置");
    Ok(())
}
