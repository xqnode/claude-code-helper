use std::sync::atomic::{AtomicBool, Ordering};

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::EventLoopBuilderExtWindows;
use tao::window::{Window, WindowBuilder, WindowId};
use wry::WebViewBuilder;

use crate::config::{self, AppConfig};

static SETTINGS_OPEN: AtomicBool = AtomicBool::new(false);

pub struct SettingsWindow {
    pub window: Window,
    _webview: wry::WebView,
}

/// 当前模型没有 Key 时需要首次引导。
pub fn needs_first_run_setup() -> bool {
    let Ok(app) = AppConfig::load() else {
        return true;
    };
    let Ok(provider) = app.active_provider() else {
        return true;
    };
    config::resolve_api_key(&provider.api_key_env).is_err()
}

/// 在托盘事件循环中打开设置窗口（关闭窗口不会退出 Helper）。
pub fn open_settings_on_loop<T>(
    elwt: &EventLoopWindowTarget<T>,
    proxy_port: u16,
) -> anyhow::Result<SettingsWindow> {
    if SETTINGS_OPEN.swap(true, Ordering::SeqCst) {
        anyhow::bail!("settings already open");
    }

    match create_settings_window(elwt, proxy_port) {
        Ok(window) => Ok(window),
        Err(err) => {
            SETTINGS_OPEN.store(false, Ordering::SeqCst);
            Err(err)
        }
    }
}

pub fn close_settings_window(slot: &mut Option<SettingsWindow>, window_id: WindowId) -> bool {
    let Some(settings) = slot.as_ref() else {
        return false;
    };
    if settings.window.id() != window_id {
        return false;
    }
    slot.take();
    SETTINGS_OPEN.store(false, Ordering::SeqCst);
    true
}

pub fn focus_settings_window(slot: &Option<SettingsWindow>) {
    if let Some(settings) = slot {
        let _ = settings.window.set_focus();
    }
}

/// CLI 单独打开设置页（无托盘时使用 `run_return`，关闭不会误杀其它进程）。
pub fn open_settings_window(proxy_port: u16) {
    if SETTINGS_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }

    std::thread::spawn(move || {
        let result = run_standalone_window(proxy_port);
        SETTINGS_OPEN.store(false, Ordering::SeqCst);
        if let Err(err) = result {
            tracing::error!("设置窗口: {err:#}");
        }
    });
}

fn run_standalone_window(proxy_port: u16) -> anyhow::Result<()> {
    let mut builder = EventLoopBuilder::new();
    builder.with_any_thread(true);
    let mut event_loop = builder.build();

    let settings = create_settings_window(&event_loop, proxy_port)?;

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            window_id,
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            if window_id == settings.window.id() {
                *control_flow = ControlFlow::Exit;
            }
        }
    });

    Ok(())
}

fn create_settings_window<T>(
    elwt: &EventLoopWindowTarget<T>,
    proxy_port: u16,
) -> anyhow::Result<SettingsWindow> {
    let window = WindowBuilder::new()
        .with_title("Claude Code Helper · 设置")
        .with_window_icon(Some(crate::icon::window_icon()))
        .with_inner_size(tao::dpi::LogicalSize::new(440.0, 480.0))
        .with_resizable(false)
        .build(elwt)?;

    center_on_screen(&window);

    let url = format!("http://127.0.0.1:{proxy_port}/admin/settings");
    let webview = WebViewBuilder::new().with_url(&url).build(&window)?;
    crate::icon::apply_window_icon(&window);

    Ok(SettingsWindow {
        window,
        _webview: webview,
    })
}

fn center_on_screen(window: &Window) {
    let monitor = window
        .primary_monitor()
        .or_else(|| window.available_monitors().next());
    let Some(monitor) = monitor else {
        return;
    };

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let window_size = window.outer_size();

    let x = monitor_pos.x + (monitor_size.width as i32 - window_size.width as i32) / 2;
    let y = monitor_pos.y + (monitor_size.height as i32 - window_size.height as i32) / 2;
    window.set_outer_position(tao::dpi::PhysicalPosition::new(x, y));
}

