//! 可视化 API Key 设置窗口（Windows）。

mod api;

#[cfg(windows)]
mod window;

pub use api::{brand_icon_svg, settings_bootstrap, settings_clear_all, settings_page, settings_save, settings_test, test_api_key};

#[cfg(windows)]
pub use window::{
    close_settings_window, focus_settings_window, needs_first_run_setup,
    open_settings_on_loop, open_settings_window, SettingsWindow,
};

#[cfg(not(windows))]
pub fn open_settings_window(_proxy_port: u16) {
    eprintln!("设置窗口目前仅支持 Windows，请使用: claude-code-helper env set DEEPSEEK_API_KEY sk-xxx");
}

#[cfg(not(windows))]
pub fn needs_first_run_setup() -> bool {
    false
}

pub fn signup_url(provider_id: &str) -> &'static str {
    match provider_id {
        "deepseek" => "https://platform.deepseek.com/",
        "qwen" => "https://dashscope.aliyun.com/",
        "kimi" | "moonshot" => "https://platform.moonshot.cn/",
        "zhipu" => "https://www.bigmodel.cn/",
        "minimax" => "https://platform.minimaxi.com/",
        "mimo" => "https://platform.xiaomimimo.com/",
        "custom" => "",
        _ => "",
    }
}
