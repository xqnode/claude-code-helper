# Changelog

## [0.1.0] - 2026-06-05

### Added

- 基于 codex-helper 架构，适配 Claude Code 桌面端
- Windows 系统托盘应用，将 Claude Code 代理到国产大模型 API
- 自动写入 `~/.claude/settings.json`（`ANTHROPIC_BASE_URL`、模型映射等）
- 本地 HTTP 代理 `127.0.0.1:25573`，支持 Anthropic Messages API
- DeepSeek 原生 Anthropic API 透传；其他厂商 Chat Completions 自动转换
- 设置窗口、请求日志、CLI 诊断、Inno Setup 安装包
