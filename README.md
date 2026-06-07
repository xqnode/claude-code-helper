# Claude Code Helper

> 轻量 Windows 托盘工具 — 让 **Claude Code 桌面端** 一键切换到 DeepSeek、通义千问、智谱、Kimi、MiniMax 等国产大模型。

> **零门槛设计**：双击安装 → 自动配置 → 托盘右键选模型 → 开始用 Claude Code。
> 全程不需要打开终端、不需要改配置文件、不需要懂技术。

基于 [codex-helper](https://github.com/xqnode/codex-helper) 架构，针对 Claude Code 的 Anthropic Messages API 与 `settings.json` 配置体系做了适配。

---

## 这是给谁用的？

- **小白用户**：第一次接触 Claude Code，只想用 DeepSeek 替代 Anthropic 省钱
- **怕折腾的用户**：看到 `~/.claude/settings.json` 就头大
- **多账号用户**：在 DeepSeek、通义、Moonshot 之间快速切换

---

## 一图看懂

```
┌──────────────────────────────────────────────────────────┐
│  系统托盘  ┌───┐                                          │
│            │ ⚡ │  ← 右键点这里                             │
│            └───┘                                          │
│       × API Key：未配置                                     │
│       × 连接：需先配置 Key                                  │
│       ─────────────────                                    │
│       切换模型 · DeepSeek · pro  >   ← 厂商 → 具体型号      │
│       ─────────────────                                    │
│       常用                                                 │
│         设置…                                              │
│         重新同步配置 / 检测连接 / 请求日志…                   │
│       更多  → 配置文件夹、切回 Anthropic 官方…               │
│       ─────────────────                                    │
│       退出 Claude Code Helper                              │
└──────────────────────────────────────────────────────────┘
                       ↓ 托盘里切换模型
┌──────────────────────────────────────────────────────────┐
│  Claude Code 桌面端  →  Helper 代理  →  国产大模型 API     │
└──────────────────────────────────────────────────────────┘
```

---

## 三步上手

### 第 1 步：下载安装

去 [GitHub Releases](https://github.com/xqnode/claude-code-helper/releases) 下载：

| 平台 | 类型 | 文件 | 操作 |
|------|------|------|------|
| **Windows** | 安装版 | `ClaudeCodeHelper-x.x.x-Setup.exe` | 双击 → 一路下一步 |
| **Windows** | 便携版 | `ClaudeCodeHelper-x.x.x-win64.zip` | 解压后双击 `claude-code-helper.exe` |
| **macOS** | DMG | `ClaudeCodeHelper-x.x.x-macos.dmg` | 拖入「应用程序」后打开 |
| **macOS** | 便携版 | `ClaudeCodeHelper-x.x.x-macos.app.zip` | 解压后运行 `Claude Code Helper.app` |

> macOS 当前以 CLI 代理模式运行（无菜单栏托盘）；Windows 支持完整托盘与设置窗口。

### 第 2 步：填 API Key

首次启动后，右键托盘 → **设置…** → 选择厂商 → 粘贴 API Key → 保存。

### 第 3 步：重启 Claude Code

**完全退出** Claude 桌面端后重新打开（或托盘 → **重新同步配置**，会自动退出 Claude）。Code 与 Cowork 将同时走 Helper Gateway。

---

## 工作原理

Claude Code 通过 `ANTHROPIC_BASE_URL` 发送 Anthropic Messages API 请求。Helper 在本地 `127.0.0.1:25573` 启动代理：

1. **自动写入** `~/.claude/settings.json` 的 `env` 块（`ANTHROPIC_BASE_URL`、模型映射等）
2. **接收** Claude Code 的 `/v1/messages` 请求
3. **转发** 到上游 API：
   - DeepSeek：原生 Anthropic API（`https://api.deepseek.com/anthropic`）
   - 其他厂商：代理转换为 OpenAI Chat Completions 格式

---

## CLI 命令

| 命令 | 说明 |
|------|------|
| `claude-code-helper` | Windows 下默认启动托盘 + 代理 |
| `init` | 初始化并写入 Claude Code 配置 |
| `start` | 启动代理（`--no-tray` 仅 CLI 模式） |
| `status` | 查看当前状态 |
| `list` | 列出可用厂商 |
| `use <id>` | 切换厂商 |
| `test` | 测试上游连通性 |
| `doctor` | 一键诊断 |
| `settings` | 打开设置窗口 |
| `env set KEY value` | 命令行保存 API Key |
| `restore-anthropic` | 恢复 Anthropic 官方配置 |

---

## 配置目录

| 路径 | 用途 |
|------|------|
| `~/.claude-code-helper/config.json` | Helper 当前厂商与代理端口 |
| `~/.claude-code-helper/.env` | API Key |
| `~/.claude-code-helper/request-log.sqlite` | 请求日志 |
| `~/.claude/settings.json` | Claude Code 用户配置（自动注入） |

---

## 开发

```bash
cargo run -- init
cargo run -- start --no-tray
cargo test
```

打包发布：

```powershell
# Windows
.\scripts\build-all.bat

# macOS（在 Mac 上）
./scripts/build-macos-release.sh
```

推送 `v*` 标签可由 GitHub Actions 自动构建四端产物并发布，见 [RELEASE.md](RELEASE.md)。

---

## 与 codex-helper 的差异

| 项目 | codex-helper | claude-code-helper |
|------|-------------|-------------------|
| 目标应用 | OpenAI Codex Desktop | Claude Code 桌面端 |
| 配置注入 | `~/.codex/config.toml` | `~/.claude/settings.json` |
| 客户端 API | OpenAI Responses API | Anthropic Messages API |
| 代理端口 | 25543 | 25573 |
| 上游 DeepSeek | Chat Completions | Anthropic API |

---

## License

MIT
