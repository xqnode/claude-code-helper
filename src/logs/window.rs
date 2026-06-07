use std::sync::atomic::{AtomicBool, Ordering};

use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder, WindowId};
use wry::WebViewBuilder;

static LOGS_OPEN: AtomicBool = AtomicBool::new(false);

pub struct LogsWindow {
    pub window: Window,
    _webview: wry::WebView,
}

pub fn open_logs_on_loop<T>(
    elwt: &EventLoopWindowTarget<T>,
    proxy_port: u16,
) -> anyhow::Result<LogsWindow> {
    if LOGS_OPEN.swap(true, Ordering::SeqCst) {
        anyhow::bail!("logs already open");
    }

    match create_logs_window(elwt, proxy_port) {
        Ok(window) => Ok(window),
        Err(err) => {
            LOGS_OPEN.store(false, Ordering::SeqCst);
            Err(err)
        }
    }
}

pub fn close_logs_window(slot: &mut Option<LogsWindow>, window_id: WindowId) -> bool {
    let Some(logs) = slot.as_ref() else {
        return false;
    };
    if logs.window.id() != window_id {
        return false;
    }
    slot.take();
    LOGS_OPEN.store(false, Ordering::SeqCst);
    true
}

pub fn focus_logs_window(slot: &Option<LogsWindow>) {
    if let Some(logs) = slot {
        let _ = logs.window.set_focus();
    }
}

fn create_logs_window<T>(
    elwt: &EventLoopWindowTarget<T>,
    proxy_port: u16,
) -> anyhow::Result<LogsWindow> {
    let window = WindowBuilder::new()
        .with_title("Claude Code Helper · 请求日志")
        .with_window_icon(Some(crate::icon::window_icon()))
        .with_inner_size(tao::dpi::LogicalSize::new(1020.0, 580.0))
        .with_resizable(true)
        .with_min_inner_size(tao::dpi::LogicalSize::new(860.0, 400.0))
        .build(elwt)?;

    center_on_screen(&window);

    let url = format!("http://127.0.0.1:{proxy_port}/admin/logs");
    let webview = WebViewBuilder::new()
        .with_devtools(false)
        .with_url(&url)
        .build(&window)?;
    crate::icon::apply_window_icon(&window);

    Ok(LogsWindow {
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
