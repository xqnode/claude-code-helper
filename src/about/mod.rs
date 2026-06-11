mod api;

#[cfg(windows)]
mod window;

pub use api::about_page;

#[cfg(windows)]
pub use window::{close_about_window, focus_about_window, open_about_on_loop, AboutWindow};
