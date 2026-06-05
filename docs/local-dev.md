# 本地开发

## 环境

- Rust stable
- Windows（托盘 + WebView2 设置窗口）
- Claude Code 桌面端（可选，用于实际调用）

## 快速开始

```bash
# 1. 首次：初始化配置，写入 ~/.claude/settings.json
cargo run -- init

# 2. 启动代理（Windows 默认带托盘）
cargo run -- start

# 或仅 CLI 模式
cargo run -- start --no-tray
```

## 常用命令

| 命令 | 说明 |
|------|------|
| `cargo run -- init` | 初始化 `~/.claude-code-helper/` 并注入 Claude Code 配置 |
| `cargo run -- status` | 查看当前状态 |
| `cargo run -- doctor` | 一键诊断 |
| `cargo run -- test` | 测试上游连通性 |
| `cargo test` | 运行单元测试 |

## 验证流程

1. `cargo run -- init`
2. `cargo run -- start`
3. 托盘 → 设置 → 填 API Key → 保存
4. 完全退出并重新打开 **Claude Code 桌面端**（或托盘 → **重新同步配置**，会自动退出 Claude）
5. 新开一条对话测试（**Code** 与 **Cowork** 标签均走 Helper Gateway）

## 调试

```powershell
$env:RUST_LOG = "claude_code_helper=debug"
cargo run -- start --no-tray
```

## 发布构建

```powershell
cargo build --release
# 产物：target\release\claude-code-helper.exe
.\build-all.bat
```

## 配置路径

| 路径 | 说明 |
|------|------|
| `~/.claude-code-helper/config.json` | 当前模型、端口等设置 |
| `~/.claude-code-helper/.env` | API Keys |
| `~/.claude-code-helper/backups/` | settings.json 自动备份 |
| `~/.claude/settings.json` | Claude Code 配置（由 Helper 注入） |
| `%LOCALAPPDATA%\Claude-3p\configLibrary\` | Cowork Gateway 配置（重新同步时写入） |
