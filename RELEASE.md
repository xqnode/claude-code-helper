# 版本发布说明

## 当前版本

**v0.2.0**（2026-06-07）

---

## 推荐：GitHub Actions 自动发版（Windows + macOS）

推送 **`v*`** 标签后，[`.github/workflows/release.yml`](.github/workflows/release.yml) 会自动：

| Runner | 产物 |
|--------|------|
| `windows-latest` | `ClaudeCodeHelper-{version}-win64.zip`、`ClaudeCodeHelper-{version}-Setup.exe` |
| `macos-latest` | `ClaudeCodeHelper-{version}-macos.app.zip`、`ClaudeCodeHelper-{version}-macos.dmg`（Universal arm64 + x86_64） |

Release 说明从 `CHANGELOG.md` 对应版本段落自动提取。

### 发版步骤（Actions）

```powershell
# 1. 本地改版本与 CHANGELOG，commit 并 push main
#    Cargo.toml / CHANGELOG.md / installer 版本号

git push origin main

# 2. 打 tag 并推送 —— 触发 CI
git tag v0.2.0
git push origin v0.2.0
```

在 GitHub → **Actions** → **Release** 查看进度；完成后 Release 页会自动出现全部产物。

> **注意**：Actions 发版**不需要**本地构建 macOS 产物。  
> macOS DMG 为 ad-hoc 签名，**未公证**；Gatekeeper 可能拦截，用户需右键打开或 `xattr -cr`（见 README）。

### 手动重跑某 tag

GitHub → Actions → Release → **Run workflow** → 填写已有 tag（如 `v0.2.0`）。

---

## 本地构建

### Windows

```powershell
# 仅 ZIP（便携版）
.\scripts\build-zip.bat

# ZIP + Inno Setup 安装包（需 Inno Setup 6）
.\scripts\build-all.bat
```

产物输出到 `dist/`（已在 `.gitignore`，不提交仓库）。

### macOS

```bash
chmod +x scripts/build-macos-release.sh
./scripts/build-macos-release.sh
# 产物：dist/Claude Code Helper.app、dist/ClaudeCodeHelper-x.y.z-macos.dmg
# 本机快速测试：NATIVE_ONLY=1 ./scripts/build-macos-release.sh
```

`main` 分支 push 也会触发 [build-macos-dmg.yml](.github/workflows/build-macos-dmg.yml) 做 CI 验证（仅上传 Artifact，不创建 Release）。

---

## Windows 本地一键发版（备用）

仅上传 **Windows zip + Setup**；macOS 产物仍建议走 GitHub Actions。

```powershell
.\scripts\release.bat
# 同版本热修复
.\scripts\release.bat -Retag
```

脚本会：结束 `claude-code-helper.exe` → 构建 ZIP + Setup → 打 tag 推送 → `gh release upload`。

首次 Windows 本地发版可准备 `dist/RELEASE_NOTES_vX.Y.Z.md`；Actions 发版则直接读 `CHANGELOG.md`。

---

## 下载

预编译安装包见 [GitHub Releases](https://github.com/xqnode/claude-code-helper/releases)：

| 文件 | 说明 |
|------|------|
| `ClaudeCodeHelper-{version}-win64.zip` | Windows 便携版 — 解压后运行 `claude-code-helper.exe` |
| `ClaudeCodeHelper-{version}-Setup.exe` | Windows Inno Setup 安装包（中文界面） |
| `ClaudeCodeHelper-{version}-macos.app.zip` | macOS 便携 — 解压后运行 `Claude Code Helper.app` |
| `ClaudeCodeHelper-{version}-macos.dmg` | macOS 安装镜像 — 拖入「应用程序」 |

### 运行要求

**Windows**

- Windows 10/11（64 位）
- [WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/)

**macOS**

- macOS 12+（Apple Silicon / Intel）
- 当前 macOS 包以 CLI 代理模式运行（`start --no-tray`）；菜单栏托盘后续版本补齐
- 测试包未签名/未公证：若提示「已损坏」，终端执行 `xattr -cr "/Applications/Claude Code Helper.app"`

**通用**

- Claude Code 桌面端
- 至少一个支持厂商的 API Key

---

## 版本规则

本项目遵循[语义化版本](https://semver.org/lang/zh-CN/)：

- **主版本号** — 不兼容的配置或代理行为变更
- **次版本号** — 新增厂商、模型或功能
- **修订号** — Bug 修复与小改进
