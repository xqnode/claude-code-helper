use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "claude-code-helper",
    about = "Claude Code Helper - 轻量代理，让 Claude Code 使用 DeepSeek 等国产大模型",
    version
)]
pub struct Cli {
    /// 省略子命令时，Windows 默认执行 start（启动托盘与代理）
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 初始化配置并写入 Claude Code settings.json
    Init,
    /// 启动本地代理（Windows 默认显示系统托盘）
    Start {
        #[arg(long, help = "不显示托盘，仅用命令行模式")]
        no_tray: bool,
    },
    /// 查看当前状态
    Status,
    /// 列出可用模型预设
    List,
    /// 切换到指定模型
    Use {
        provider: String,
    },
    /// 测试当前模型连通性
    Test,
    /// 一键诊断环境
    Doctor,
    /// 打开 API Key 设置窗口
    Settings,
    /// 设置 API Key（命令行，高级）
    Env {
        #[command(subcommand)]
        action: EnvAction,
    },
    /// 恢复 Anthropic 官方配置
    RestoreAnthropic,
    /// 修复 Claude Desktop 缺失的 Claude Code 组件（claude.exe）
    RepairClaudeCode,
}

#[derive(Subcommand, Debug)]
pub enum EnvAction {
    /// 保存 API Key 到 ~/.claude-code-helper/.env
    Set {
        key: String,
        value: String,
    },
}
