# Changelog

## [0.2.0] - 2026-06-07

### Added

- 多轮 tool 调用稳定性：`message_repair` 合并连续 assistant tool_calls、补齐缺失 tool 回复、system 合并至首条
- 思考模型兼容：`reasoning_content` 双向转换（Anthropic `thinking` ↔ Chat Completions）、厂商感知 `reasoning_options` 与 `chat_reasoning` 配置表
- 上游重试：429/502/503/504 最多 3 次指数退避重试（`upstream_retry`）
- 双 HTTP 客户端：非流式 600s / 流式 300s 读空闲超时
- 流式 tool_calls SSE 增强转换（`AnthropicSseTranslator`）
- 配置项 `model_reasoning_effort`（默认 medium）、`tool_output_max_chars`（默认 0 禁用截断）
- Claude Code 桌面端 CCD binary 检测与修复（`repair-claude-code` CLI、托盘菜单、`doctor` 诊断）
- macOS 打包：Universal `.app`、`.app.zip`、DMG（`scripts/build-macos-release.sh`）
- GitHub Actions 自动发版：推 `v*` tag 发布 Windows zip + Setup + macOS app zip + DMG

### Changed

- `tool_result` 正确展开为独立 `role: tool` 消息（修复多轮 tool 上下文断裂）
- `build.rs` 全平台生成 PNG 图标；Windows 仍生成 `.ico`
- 安装包与文档仓库地址统一为 `xqnode/claude-code-helper`

### Fixed

- 修复「Host Claude Code binary not available」：自动检测/复制/下载 `claude.exe` 到 `%LOCALAPPDATA%\Claude-3p\claude-code\{version}\`
- 流式 `reasoning_content` 正确映射为 Anthropic `thinking_delta`（不再混入 `text_delta`）
- `tool_result` 数组内容扁平化为字符串，避免上游 Chat API 400
- 合并连续 assistant tool_calls 时保留并拼接多条 `content`
- `tool_output_max_chars` 支持截断非字符串 tool 内容（JSON 序列化后 head+tail）
- 流式读空闲超时 300s → 600s，降低超长思考被误断概率
- SSE 流缓冲：UTF-8 安全拼接 + `\r\n\r\n` 分隔符支持（移植 codex-helper `sse.rs`）
- 测试覆盖 44 → 85：补齐 `proxy/mod` 集成测、`reasoning_options` 厂商矩阵、`message_repair` 边界用例
- `logged_stream` 无 runtime 时 `push_sync` 兜底；新增流式日志与多轮 tool 黄金夹具测试
- `env_sync` 抽出 `parse_reg_query_value`；`ccd_binary` 补版本目录检测测试

## [0.1.0] - 2026-06-05

### Added

- 基于 codex-helper 架构，适配 Claude Code 桌面端
- Windows 系统托盘应用，将 Claude Code 代理到国产大模型 API
- 自动写入 `~/.claude/settings.json`（`ANTHROPIC_BASE_URL`、模型映射等）
- 本地 HTTP 代理 `127.0.0.1:25573`，支持 Anthropic Messages API
- DeepSeek 原生 Anthropic API 透传；其他厂商 Chat Completions 自动转换
- 设置窗口、请求日志、CLI 诊断、Inno Setup 安装包
