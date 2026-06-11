use std::sync::atomic::{AtomicBool, Ordering};

use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder, WindowId};
use wry::WebViewBuilder;

static ABOUT_OPEN: AtomicBool = AtomicBool::new(false);

pub struct AboutWindow {
    pub window: Window,
    _webview: wry::WebView,
}

pub fn open_about_on_loop<T>(
    elwt: &EventLoopWindowTarget<T>,
    proxy_port: u16,
) -> anyhow::Result<AboutWindow> {
    if ABOUT_OPEN.swap(true, Ordering::SeqCst) {
        anyhow::bail!("about already open");
    }

    match create_about_window(elwt, proxy_port) {
        Ok(window) => Ok(window),
        Err(err) => {
            ABOUT_OPEN.store(false, Ordering::SeqCst);
            Err(err)
        }
    }
}

pub fn close_about_window(slot: &mut Option<AboutWindow>, window_id: WindowId) -> bool {
    let Some(about) = slot.as_ref() else {
        return false;
    };
    if about.window.id() != window_id {
        return false;
    }
    if let Some(about) = slot.take() {
        crate::platform::suppress_native_stderr(|| drop(about));
    }
    ABOUT_OPEN.store(false, Ordering::SeqCst);
    true
}

pub fn focus_about_window(slot: &Option<AboutWindow>) {
    if let Some(about) = slot {
        let _ = about.window.set_focus();
    }
}

fn create_about_window<T>(
    elwt: &EventLoopWindowTarget<T>,
    proxy_port: u16,
) -> anyhow::Result<AboutWindow> {
    let window = WindowBuilder::new()
        .with_title("Claude Code Helper · 关于")
        .with_window_icon(Some(crate::icon::window_icon()))
        .with_inner_size(tao::dpi::LogicalSize::new(400.0, 360.0))
        .with_resizable(false)
        .with_maximizable(false)
        .build(elwt)?;

    center_on_screen(&window);

    let url = format!("http://127.0.0.1:{proxy_port}/admin/about");
    let webview = crate::platform::suppress_native_stderr(|| {
        WebViewBuilder::new()
            .with_devtools(false)
            .with_url(&url)
            .build(&window)
    })?;
    crate::icon::apply_window_icon(&window);

    Ok(AboutWindow {
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
