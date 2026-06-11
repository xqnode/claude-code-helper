#[cfg(windows)]
use std::cell::RefCell;
#[cfg(windows)]
use std::rc::Rc;
#[cfg(windows)]
use std::sync::Arc;

#[cfg(windows)]
use tao::event::{Event, StartCause, WindowEvent};
#[cfg(windows)]
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
#[cfg(windows)]
use tao::platform::windows::EventLoopBuilderExtWindows;
#[cfg(windows)]
use tokio::sync::RwLock;
#[cfg(windows)]
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    TrayIcon, TrayIconBuilder,
};

#[cfg(windows)]
use crate::about;
#[cfg(windows)]
use crate::actions;
#[cfg(windows)]
use crate::config::{self, AppConfig};
#[cfg(windows)]
use crate::proxy::{self, ProxyState};
#[cfg(windows)]
use crate::logs;
#[cfg(windows)]
use crate::settings;

#[cfg(windows)]
enum TrayUserEvent {
    RefreshUi,
    OpenSettings,
    OpenLogs,
    OpenAbout,
    CheckHealth,
}

#[cfg(windows)]
#[derive(Clone, Debug)]
enum ConnectionStatus {
    Unknown,
    Checking,
    Ok,
    Skipped(String),
    Failed(String),
}

#[cfg(windows)]
#[derive(Clone, Debug)]
struct ProviderHealth {
    provider_id: String,
    connection: ConnectionStatus,
}

#[cfg(windows)]
impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            connection: ConnectionStatus::Unknown,
        }
    }
}

#[cfg(windows)]
pub async fn run_with_proxy(app: AppConfig) -> anyhow::Result<()> {
    detach_console_if_attached();
    let rt_handle = tokio::runtime::Handle::current();
    let config = Arc::new(RwLock::new(app.clone()));
    let proxy = proxy::spawn_server(app.clone())?;

    let mut builder = EventLoopBuilder::<TrayUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build();
    let loop_proxy = event_loop.create_proxy();

    proxy::register_tray_health_check(
        &proxy,
        Arc::new({
            let loop_proxy = loop_proxy.clone();
            move || {
                let _ = loop_proxy.send_event(TrayUserEvent::CheckHealth);
            }
        }),
    );

    let menu = build_menu(&app, &ProviderHealth::default())?;
    let tray = TrayIconBuilder::new()
        .with_icon(crate::icon::tray_icon())
        .with_menu(Box::new(menu.clone()))
        .with_tooltip(&tooltip_text(&app, &ProviderHealth::default()))
        .build()?;

    let settings_slot: Rc<RefCell<Option<settings::SettingsWindow>>> =
        Rc::new(RefCell::new(None));
    let logs_slot: Rc<RefCell<Option<logs::LogsWindow>>> = Rc::new(RefCell::new(None));
    let about_slot: Rc<RefCell<Option<about::AboutWindow>>> = Rc::new(RefCell::new(None));

    let ctx = Arc::new(TrayContext {
        config,
        proxy,
        tray,
        loop_proxy,
        rt: rt_handle,
        settings: settings_slot.clone(),
        logs: logs_slot.clone(),
        about: about_slot.clone(),
        health: Arc::new(RwLock::new(ProviderHealth::default())),
    });

    let menu_channel = MenuEvent::receiver();
    let ctx_for_loop = ctx.clone();

    // 托盘事件循环会阻塞当前线程；block_in_place 让 Tokio 继续在其它 worker 上跑代理
    tokio::task::block_in_place(|| {
        event_loop.run(move |event, elwt, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::NewEvents(StartCause::Init) => {
                    let _ = ctx_for_loop
                        .loop_proxy
                        .send_event(TrayUserEvent::CheckHealth);
                }
                Event::UserEvent(TrayUserEvent::RefreshUi) => {
                    refresh_tray_ui(&ctx_for_loop);
                }
                Event::UserEvent(TrayUserEvent::CheckHealth) => {
                    start_health_check(&ctx_for_loop);
                }
                Event::UserEvent(TrayUserEvent::OpenSettings) => {
                    let mut slot = ctx_for_loop.settings.borrow_mut();
                    if slot.is_some() {
                        settings::focus_settings_window(&slot);
                        return;
                    }
                    let port = ctx_for_loop.rt.block_on(async {
                        ctx_for_loop.config.read().await.proxy.port
                    });
                    match settings::open_settings_on_loop(elwt, port) {
                        Ok(window) => *slot = Some(window),
                        Err(err) if err.to_string().contains("already open") => {
                            settings::focus_settings_window(&slot);
                        }
                        Err(err) => tracing::error!("打开设置窗口: {err:#}"),
                    }
                }
                Event::UserEvent(TrayUserEvent::OpenLogs) => {
                    let mut slot = ctx_for_loop.logs.borrow_mut();
                    if slot.is_some() {
                        logs::focus_logs_window(&slot);
                        return;
                    }
                    let port = ctx_for_loop.rt.block_on(async {
                        ctx_for_loop.config.read().await.proxy.port
                    });
                    match logs::open_logs_on_loop(elwt, port) {
                        Ok(window) => *slot = Some(window),
                        Err(err) if err.to_string().contains("already open") => {
                            logs::focus_logs_window(&slot);
                        }
                        Err(err) => tracing::error!("打开请求日志: {err:#}"),
                    }
                }
                Event::UserEvent(TrayUserEvent::OpenAbout) => {
                    let mut slot = ctx_for_loop.about.borrow_mut();
                    if slot.is_some() {
                        about::focus_about_window(&slot);
                        return;
                    }
                    let port = ctx_for_loop.rt.block_on(async {
                        ctx_for_loop.config.read().await.proxy.port
                    });
                    match about::open_about_on_loop(elwt, port) {
                        Ok(window) => *slot = Some(window),
                        Err(err) if err.to_string().contains("already open") => {
                            about::focus_about_window(&slot);
                        }
                        Err(err) => tracing::error!("打开关于窗口: {err:#}"),
                    }
                }
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    let mut settings_slot = ctx_for_loop.settings.borrow_mut();
                    if settings::close_settings_window(&mut settings_slot, window_id) {
                        return;
                    }
                    let mut logs_slot = ctx_for_loop.logs.borrow_mut();
                    if logs::close_logs_window(&mut logs_slot, window_id) {
                        return;
                    }
                    let mut about_slot = ctx_for_loop.about.borrow_mut();
                    about::close_about_window(&mut about_slot, window_id);
                }
                _ => {}
            }

            if let Ok(menu_event) = menu_channel.try_recv() {
                handle_menu_click(&ctx_for_loop, menu_event.id.0.as_str());
            }
        });
    });

    Ok(())
}

#[cfg(windows)]
struct TrayContext {
    config: Arc<RwLock<AppConfig>>,
    proxy: Arc<ProxyState>,
    tray: TrayIcon,
    loop_proxy: EventLoopProxy<TrayUserEvent>,
    rt: tokio::runtime::Handle,
    settings: Rc<RefCell<Option<settings::SettingsWindow>>>,
    logs: Rc<RefCell<Option<logs::LogsWindow>>>,
    about: Rc<RefCell<Option<about::AboutWindow>>>,
    health: Arc<RwLock<ProviderHealth>>,
}

#[cfg(windows)]
struct TrayWorker {
    config: Arc<RwLock<AppConfig>>,
    proxy: Arc<ProxyState>,
}

#[cfg(windows)]
fn handle_menu_click(ctx: &Arc<TrayContext>, id: &str) {
    if id == "quit" {
        // 先释放 WebView，避免 Chromium 在进程退出时刷 stderr 噪音。
        ctx.settings.borrow_mut().take();
        ctx.logs.borrow_mut().take();
        ctx.about.borrow_mut().take();
        std::process::exit(0);
    }
    if id == "settings" {
        let _ = ctx.loop_proxy.send_event(TrayUserEvent::OpenSettings);
        return;
    }
    if id == "request_logs" {
        let _ = ctx.loop_proxy.send_event(TrayUserEvent::OpenLogs);
        return;
    }
    if id == "about" {
        let _ = ctx.loop_proxy.send_event(TrayUserEvent::OpenAbout);
        return;
    }
    if id == "open_helper" {
        let _ = actions::open_helper_dir();
        return;
    }
    if id == "open_claude" {
        let _ = actions::open_claude_dir();
        return;
    }
    if id == "resync" {
        spawn_tray_task(ctx, async move |w| {
            if let Err(err) = actions::resync_claude(&w.config, &w.proxy).await {
                tracing::error!("同步失败: {err:#}");
            }
        });
        return;
    }
    if id == "check_health" {
        let _ = ctx.loop_proxy.send_event(TrayUserEvent::CheckHealth);
        return;
    }
    if id == "repair_claude_code" {
        spawn_tray_task(ctx, async move |_w| {
            if let Err(err) = actions::repair_claude_code_component().await {
                tracing::error!("修复 Claude Code 组件失败: {err:#}");
            }
        });
        return;
    }
    if id == "restore_anthropic" {
        spawn_tray_task(ctx, async move |_w| {
            if let Err(err) = actions::restore_anthropic().await {
                tracing::error!("恢复失败: {err:#}");
            }
        });
        return;
    }
    if id == "kill_reset_claude" {
        spawn_tray_task(ctx, async move |_w| {
            if let Err(err) = actions::kill_claude_and_reset_defaults().await {
                tracing::error!("退出并恢复默认失败: {err:#}");
            }
        });
        return;
    }
    if let Some(rest) = id.strip_prefix("use:") {
        let (provider_id, model_slug) = match rest.split_once(':') {
            Some((provider_id, model_slug)) => (provider_id.to_string(), model_slug.to_string()),
            None => (rest.to_string(), String::new()),
        };
        spawn_tray_task(ctx, async move |w| {
            let result = if model_slug.is_empty() {
                actions::switch_provider(&w.config, &w.proxy, &provider_id).await
            } else {
                actions::switch_provider_model(&w.config, &w.proxy, &provider_id, &model_slug).await
            };
            match result {
                Ok(()) => tracing::info!("托盘切换完成: {provider_id} · {model_slug}"),
                Err(err) => tracing::error!("切换失败: {err:#}"),
            }
        });
    }
}

#[cfg(windows)]
fn spawn_tray_task<F, Fut>(ctx: &Arc<TrayContext>, task: F)
where
    F: FnOnce(TrayWorker) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let worker = TrayWorker {
        config: ctx.config.clone(),
        proxy: ctx.proxy.clone(),
    };
    let loop_proxy = ctx.loop_proxy.clone();
    let rt = ctx.rt.clone();
    rt.spawn(async move {
        task(worker).await;
        let _ = loop_proxy.send_event(TrayUserEvent::CheckHealth);
    });
}

#[cfg(windows)]
fn start_health_check(ctx: &Arc<TrayContext>) {
    let provider_id = ctx.rt.block_on(async {
        ctx.config.read().await.active.clone()
    });
    ctx.rt.block_on(async {
        let mut health = ctx.health.write().await;
        health.provider_id = provider_id;
        health.connection = ConnectionStatus::Checking;
    });
    refresh_tray_ui(ctx);

    let config = ctx.config.clone();
    let health = ctx.health.clone();
    let loop_proxy = ctx.loop_proxy.clone();
    ctx.rt.spawn(async move {
        run_health_check(config, health).await;
        let _ = loop_proxy.send_event(TrayUserEvent::RefreshUi);
    });
}

#[cfg(windows)]
async fn run_health_check(
    config: Arc<RwLock<AppConfig>>,
    health: Arc<RwLock<ProviderHealth>>,
) {
    let app = match AppConfig::load() {
        Ok(app) => app,
        Err(err) => {
            store_health(&health, "", ConnectionStatus::Failed(err.to_string())).await;
            return;
        }
    };
    *config.write().await = app.clone();

    let provider = match app.active_provider() {
        Ok(p) => p.clone(),
        Err(err) => {
            store_health(
                &health,
                &app.active,
                ConnectionStatus::Failed(err.to_string()),
            )
            .await;
            return;
        }
    };

    let api_key = match config::resolve_api_key(&provider.api_key_env) {
        Ok(key) => key,
        Err(_) => {
            store_health(
                &health,
                &provider.id,
                ConnectionStatus::Skipped("需先配置 Key".into()),
            )
            .await;
            return;
        }
    };

    if provider.id == "custom" && provider.base_url.trim().is_empty() {
        store_health(
            &health,
            &provider.id,
            ConnectionStatus::Skipped("需填写 Base URL".into()),
        )
        .await;
        return;
    }

    let connection = match settings::test_api_key(&provider, &api_key).await {
        Ok(()) => ConnectionStatus::Ok,
        Err(err) => ConnectionStatus::Failed(format!("{err:#}")),
    };
    store_health(&health, &provider.id, connection).await;
}

#[cfg(windows)]
async fn store_health(
    health: &Arc<RwLock<ProviderHealth>>,
    provider_id: &str,
    connection: ConnectionStatus,
) {
    let mut state = health.write().await;
    state.provider_id = provider_id.to_string();
    state.connection = connection;
}

#[cfg(windows)]
fn refresh_tray_ui(ctx: &Arc<TrayContext>) {
    let app = ctx.rt.block_on(async { ctx.config.read().await.clone() });
    let health = ctx.rt.block_on(async { ctx.health.read().await.clone() });
    if let Ok(menu) = build_menu(&app, &health) {
        let _ = ctx.tray.set_menu(Some(Box::new(menu)));
        let tip = tooltip_text(&app, &health);
        let _ = ctx.tray.set_tooltip(Some(tip.as_str()));
    }
}

#[cfg(windows)]
fn tooltip_text(app: &AppConfig, health: &ProviderHealth) -> String {
    let name = app
        .active_provider()
        .map(|p| p.name.as_str())
        .unwrap_or("未知");
    if !active_key_configured(app) {
        return format!("Claude Code Helper — {name}（需设置 API Key）");
    }
    if health.provider_id == app.active
        && matches!(health.connection, ConnectionStatus::Failed(_))
    {
        return format!("Claude Code Helper — {name}（连接失败）");
    }
    format!("Claude Code Helper — {name}（运行中）")
}

#[cfg(windows)]
fn active_key_configured(app: &AppConfig) -> bool {
    app.active_provider()
        .ok()
        .and_then(|p| config::resolve_api_key(&p.api_key_env).ok())
        .is_some()
}

#[cfg(windows)]
fn format_provider_with_tag(name: &str, provider: &crate::config::ProviderConfig) -> String {
    match crate::provider::models::menu_tag(provider) {
        Some(tag) => format!("{name} · {tag}"),
        None => name.to_string(),
    }
}

#[cfg(windows)]
fn menu_key_line(app: &AppConfig) -> String {
    if active_key_configured(app) {
        "√ API Key：已配置".into()
    } else {
        "× API Key：未配置".into()
    }
}

#[cfg(windows)]
fn menu_connection_line(app: &AppConfig, health: &ProviderHealth) -> String {
    if health.provider_id != app.active {
        return "… 连接：检测中…".into();
    }
    match &health.connection {
        ConnectionStatus::Unknown => "… 连接：未检测".into(),
        ConnectionStatus::Checking => "… 连接：检测中…".into(),
        ConnectionStatus::Ok => "√ 连接：正常".into(),
        ConnectionStatus::Skipped(msg) => format!("× 连接：{msg}"),
        ConnectionStatus::Failed(msg) => {
            format!("× 连接：{}", truncate_menu_text(msg, 32))
        }
    }
}

#[cfg(windows)]
fn truncate_menu_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max_chars).collect::<String>())
}

#[cfg(windows)]
fn build_menu(app: &AppConfig, health: &ProviderHealth) -> anyhow::Result<Menu> {
    let menu = Menu::new();
    menu.append(&MenuItem::with_id(
        "key_status",
        menu_key_line(app),
        false,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "conn_status",
        menu_connection_line(app, health),
        false,
        None,
    ))?;
    menu.append(&PredefinedMenuItem::separator())?;

    let active_label = app
        .active_provider()
        .ok()
        .map(|p| format_provider_with_tag(p.name.as_str(), p))
        .unwrap_or_else(|| "未选择".to_string());
    let model_menu = Submenu::with_id(
        "models",
        format!("切换模型  ·  {active_label}"),
        true,
    );
    for preset in crate::provider::list_presets(app) {
        let provider_label = if preset.id == app.active {
            format!("✓ {}", preset.name)
        } else {
            preset.name.clone()
        };
        let provider_sub = Submenu::with_id(
            format!("provider:{}", preset.id),
            provider_label,
            true,
        );
        for model in crate::provider::models::list_models(preset) {
            let is_active =
                preset.id == app.active && preset.default_model == model.slug;
            let label = crate::provider::models::tray_model_entry_label(&model, is_active);
            provider_sub.append(&MenuItem::with_id(
                format!("use:{}:{}", preset.id, model.slug),
                label,
                true,
                None,
            ))?;
        }
        model_menu.append(&provider_sub)?;
    }
    menu.append(&model_menu)?;

    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id("hdr_setup", "常用", false, None))?;
    menu.append(&MenuItem::with_id(
        "settings",
        "设置…",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "resync",
        "重新同步配置",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "check_health",
        "检测连接",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "repair_claude_code",
        "修复 Claude Code 组件",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "request_logs",
        "请求日志…",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "about",
        "关于…",
        true,
        None,
    ))?;

    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id("hdr_more", "更多", false, None))?;
    menu.append(&MenuItem::with_id(
        "open_helper",
        "打开配置文件夹",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "open_claude",
        "打开 Claude 文件夹",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "restore_anthropic",
        "切换回 Anthropic 官方",
        true,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        "kill_reset_claude",
        "重置 Claude Code 为默认设置",
        true,
        None,
    ))?;

    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id("quit", "退出 Claude Code Helper", true, None))?;
    Ok(menu)
}

#[cfg(windows)]
fn detach_console_if_attached() {
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
        fn FreeConsole() -> i32;
    }
    unsafe {
        if !GetConsoleWindow().is_null() {
            FreeConsole();
        }
    }
}

#[cfg(not(windows))]
pub async fn run_with_proxy(_app: AppConfig) -> anyhow::Result<()> {
    anyhow::bail!("系统托盘目前仅支持 Windows")
}
