//! 平台相关初始化（抑制 WebView2/Chromium 等原生层 stderr 噪音）。

const WEBVIEW_QUIET_ARGS: &str =
    "--disable-logging --log-level=3 --noerrdialogs --disable-breakpad --disable-gpu";

#[cfg(windows)]
pub fn init() {
    // 须在首次创建 WebView2 之前设置；降低 Chromium 日志级别。
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", WEBVIEW_QUIET_ARGS);
}

#[cfg(windows)]
pub fn suppress_native_stderr<R>(f: impl FnOnce() -> R) -> R {
    use std::fs::OpenOptions;
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};

    unsafe extern "system" {
        fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
        fn SetStdHandle(nStdHandle: u32, h: *mut std::ffi::c_void) -> i32;
    }

    const STD_ERROR_HANDLE: u32 = 12;

    let saved = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    let nul = OpenOptions::new().write(true).open(r"\\.\NUL");
    if let Ok(nul_file) = nul {
        let nul_handle = nul_file.into_raw_handle();
        let redirected = unsafe { SetStdHandle(STD_ERROR_HANDLE, nul_handle as *mut _) != 0 };
        let result = f();
        if redirected {
            unsafe {
                SetStdHandle(STD_ERROR_HANDLE, saved);
            }
        }
        let _ = unsafe { std::fs::File::from_raw_handle(nul_handle) };
        result
    } else {
        f()
    }
}

#[cfg(not(windows))]
pub fn init() {}

#[cfg(not(windows))]
pub fn suppress_native_stderr<R>(f: impl FnOnce() -> R) -> R {
    f()
}
