use std::path::{Path, PathBuf};
use std::process::Command;

const CCD_DIST_BASE: &str =
    "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CcdBinaryStatus {
    Ready { version: String, path: PathBuf },
    Missing { expected_version: Option<String>, root: PathBuf },
}

pub fn ccd_binary_root() -> anyhow::Result<PathBuf> {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let path = PathBuf::from(local).join("Claude-3p").join("claude-code");
        if path.exists() || std::fs::create_dir_all(&path).is_ok() {
            return Ok(path);
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        let path = local.join("Claude-3p").join("claude-code");
        std::fs::create_dir_all(&path)?;
        return Ok(path);
    }
    anyhow::bail!("无法定位 Claude Desktop 的 claude-code 目录")
}

pub fn status() -> anyhow::Result<CcdBinaryStatus> {
    let root = ccd_binary_root()?;
    if let Some((version, path)) = find_installed_binary(&root) {
        return Ok(CcdBinaryStatus::Ready { version, path });
    }
    Ok(CcdBinaryStatus::Missing {
        expected_version: detect_expected_version(&root),
        root,
    })
}

pub fn is_ready() -> bool {
    matches!(status(), Ok(CcdBinaryStatus::Ready { .. }))
}

pub fn repair() -> anyhow::Result<()> {
    let root = ccd_binary_root()?;
    if let Some((version, path)) = find_installed_binary(&root) {
        println!("✅ Claude Code 组件已就绪: {} ({})", version, path.display());
        return Ok(());
    }

    let version = detect_expected_version(&root)
        .ok_or_else(|| anyhow::anyhow!("未找到需要的 Claude Code 版本目录，请先打开一次 Claude Desktop"))?;
    let target_dir = root.join(&version);
    std::fs::create_dir_all(&target_dir)?;

    let source = find_local_claude_exe()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "未找到本地 claude.exe。可先运行: npm install -g @anthropic-ai/claude-code"
            )
        })?;

    let target_exe = target_dir.join("claude.exe");
    std::fs::copy(&source, &target_exe).map_err(|err| {
        anyhow::anyhow!(
            "复制 claude.exe 失败 ({} -> {}): {err}",
            source.display(),
            target_exe.display()
        )
    })?;
    std::fs::write(target_dir.join(".verified"), b"")?;

    println!("✅ Claude Code 组件修复完成");
    println!("   版本: {version}");
    println!("   路径: {}", target_exe.display());
    println!();
    println!("请完全退出并重新打开 Claude Desktop，再在 Code 里发一条消息验证。");
    Ok(())
}

pub async fn repair_with_download() -> anyhow::Result<()> {
    if is_ready() {
        if let Ok(CcdBinaryStatus::Ready { version, path }) = status() {
            println!("✅ Claude Code 组件已就绪: {version} ({})", path.display());
        }
        return Ok(());
    }

    if repair().is_ok() {
        return Ok(());
    }

    let root = ccd_binary_root()?;
    let version = detect_expected_version(&root)
        .ok_or_else(|| anyhow::anyhow!("未找到需要的 Claude Code 版本目录"))?;
    let target_dir = root.join(&version);
    std::fs::create_dir_all(&target_dir)?;
    let target_exe = target_dir.join("claude.exe");

    let url = format!("{CCD_DIST_BASE}/{version}/win32-x64/claude.exe");
    println!("正在下载 Claude Code 组件 {version} …");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;
    let bytes = client.get(&url).send().await?.error_for_status()?.bytes().await?;
    std::fs::write(&target_exe, &bytes)?;
    std::fs::write(target_dir.join(".verified"), b"")?;

    println!("✅ Claude Code 组件下载完成: {}", target_exe.display());
    println!("请完全退出并重新打开 Claude Desktop。");
    Ok(())
}

fn find_installed_binary(root: &Path) -> Option<(String, PathBuf)> {
    let mut candidates = Vec::new();
    collect_version_dirs(root, &mut candidates);
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    for (version, dir) in candidates {
        let exe = dir.join("claude.exe");
        let verified = dir.join(".verified");
        if exe.is_file() && verified.exists() {
            return Some((version, exe));
        }
    }
    None
}

fn collect_version_dirs(root: &Path, out: &mut Vec<(String, PathBuf)>) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if looks_like_version(name) {
            out.push((name.to_string(), path));
        }
    }
}

fn detect_expected_version(root: &Path) -> Option<String> {
    let mut versions = Vec::new();
    collect_version_dirs(root, &mut versions);
    versions.sort_by(|a, b| b.0.cmp(&a.0));

    for (version, dir) in &versions {
        let exe = dir.join("claude.exe");
        let verified = dir.join(".verified");
        if !exe.is_file() || !verified.exists() {
            return Some(version.clone());
        }
    }

    versions.first().map(|(version, _)| version.clone())
}

fn looks_like_version(name: &str) -> bool {
    let mut parts = name.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), Some(c))
            if !a.is_empty() && b.chars().all(|ch| ch.is_ascii_digit()) && c.chars().all(|ch| ch.is_ascii_digit())
    )
}

fn find_local_claude_exe() -> Option<PathBuf> {
    local_claude_exe_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn local_claude_exe_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(path) = which_claude_in_path() {
        out.push(path);
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        out.push(
            PathBuf::from(appdata)
                .join("npm")
                .join("node_modules")
                .join("@anthropic-ai")
                .join("claude-code")
                .join("bin")
                .join("claude.exe"),
        );
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        out.push(
            PathBuf::from(home)
                .join(".local")
                .join("bin")
                .join("claude.exe"),
        );
    }
    out
}

fn which_claude_in_path() -> anyhow::Result<PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let output = Command::new("where")
            .arg("claude")
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
        if !output.status.success() {
            anyhow::bail!("where claude failed");
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && line.to_ascii_lowercase().ends_with("claude.exe"))
            .ok_or_else(|| anyhow::anyhow!("claude.exe not in PATH"))?;
        Ok(PathBuf::from(line))
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("PATH lookup only implemented on Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_semver_like_directories() {
        assert!(looks_like_version("2.1.138"));
        assert!(!looks_like_version("claude-code"));
        assert!(!looks_like_version("2.1"));
    }

    #[test]
    fn detect_expected_version_targets_first_incomplete_install() {
        let root = std::env::temp_dir().join(format!("ccd-test-{}", uuid::Uuid::new_v4()));
        let complete = root.join("2.1.138");
        let incomplete = root.join("2.1.139");
        std::fs::create_dir_all(&complete).unwrap();
        std::fs::create_dir_all(&incomplete).unwrap();
        std::fs::write(complete.join("claude.exe"), b"").unwrap();
        std::fs::write(complete.join(".verified"), b"").unwrap();
        std::fs::write(incomplete.join("claude.exe"), b"").unwrap();

        assert_eq!(detect_expected_version(&root).as_deref(), Some("2.1.139"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_installed_binary_returns_highest_verified_install() {
        let root = std::env::temp_dir().join(format!("ccd-test-{}", uuid::Uuid::new_v4()));
        for (version, verified) in [("2.1.100", true), ("2.1.138", true), ("2.1.120", false)] {
            let dir = root.join(version);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("claude.exe"), b"").unwrap();
            if verified {
                std::fs::write(dir.join(".verified"), b"").unwrap();
            }
        }

        let found = find_installed_binary(&root).unwrap();
        assert_eq!(found.0, "2.1.138");
        assert!(found.1.ends_with("2.1.138\\claude.exe") || found.1.ends_with("2.1.138/claude.exe"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
