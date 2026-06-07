//! 平台相关初始化（抑制 WebView2/Chromium 等原生层 stderr 噪音）。

#[cfg(windows)]
pub fn init() {
    // 须在首次创建 WebView2 之前设置；降低 Chromium 日志级别。
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--disable-logging --log-level=3 --noerrdialogs",
    );
}

#[cfg(not(windows))]
pub fn init() {}
