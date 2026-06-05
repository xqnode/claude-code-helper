mod api;

#[cfg(windows)]
mod window;

pub use api::{logs_bootstrap, logs_clear, logs_page};

#[cfg(windows)]
pub use window::{
    close_logs_window, focus_logs_window, open_logs_on_loop, LogsWindow,
};
