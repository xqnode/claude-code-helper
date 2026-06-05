#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod cli;
mod claude;
mod commands;
mod config;
mod env_sync;
mod icon;
mod logs;
mod paths;
mod provider;
mod proxy;
mod request_log;
mod settings;

#[cfg(windows)]
mod tray;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("claude_code_helper=warn".parse().unwrap()),
        )
        .with_target(false)
        .init();

    let cli = cli::Cli::parse();
    if let Err(err) = commands::run(cli).await {
        eprintln!("❌ {err:#}");
        std::process::exit(1);
    }
}
