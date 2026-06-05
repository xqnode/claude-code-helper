//! 应用图标（托盘与设置窗口共用）。

#[cfg(windows)]
const TRAY_ICON_SIZE: u32 = 128;
#[cfg(windows)]
const WINDOW_ICON_SIZE: u32 = 256;

#[cfg(windows)]
mod render {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/icon_render.rs"));
}

#[cfg(windows)]
pub fn tray_icon() -> tray_icon::Icon {
    let rgba = render::render_icon_rgba(TRAY_ICON_SIZE);
    tray_icon::Icon::from_rgba(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE).expect("tray icon")
}

#[cfg(windows)]
pub fn window_icon() -> tao::window::Icon {
    let rgba = render::render_icon_rgba(WINDOW_ICON_SIZE);
    tao::window::Icon::from_rgba(rgba, WINDOW_ICON_SIZE, WINDOW_ICON_SIZE).expect("window icon")
}

/// WebView2 创建子窗口后可能覆盖任务栏图标，需再次设置。
#[cfg(windows)]
pub fn apply_window_icon(window: &tao::window::Window) {
    window.set_window_icon(Some(window_icon()));
}
