#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod about;
mod actions;
mod cli;
mod claude;
mod commands;
mod config;
mod env_sync;
mod icon;
mod logs;
mod paths;
mod platform;
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
    platform::init();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off")),
        )
        .with_target(false)
        .init();

    let cli = cli::Cli::parse();
    if let Err(err) = commands::run(cli).await {
        eprintln!("❌ {err:#}");
        std::process::exit(1);
    }
}
