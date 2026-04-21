#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]
#![allow(unused_assignments)]
#![allow(unused_imports)]
#![allow(unused_mut)]
#![allow(unused_variables)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::get_first)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::manual_flatten)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::nonminimal_bool)]
#![allow(clippy::question_mark)]
#![allow(clippy::useless_format)]

mod commands;
mod database;
mod models;
mod services;

use std::fs;
use tracing::info;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn setup_logging() {
    let log_dir = services::state::app_data_root()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("logs");

    // 创建日志目录
    let _ = fs::create_dir_all(&log_dir);

    // 文件日志滚动输出到 app_data/logs/
    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "capyspeedtest.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 保留日志文件 guard，不让它被 drop
    std::mem::forget(_guard);

    // 控制台 + 文件双输出
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(true)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .init();
}

fn main() {
    setup_logging();

    info!("CapySpeedtest 启动中...");
    info!(
        "日志目录: {:?}",
        services::state::app_data_root()
            .ok()
            .map(|p| p.join("logs"))
    );

    tauri::Builder::default()
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings_snapshot,
            commands::list_kernel_versions_cmd,
            commands::select_kernel_version,
            commands::refresh_ip_database,
            commands::parse_subscription_nodes,
            commands::fetch_subscription_from_url,
            commands::check_kernel_geoip_updates,
            commands::check_client_update,
            commands::download_client_update,
            commands::run_scheduled_update_checks,
            commands::run_speedtest_batch,
            commands::resume_speedtest_from_checkpoint,
            commands::get_speedtest_checkpoint,
            commands::clear_speedtest_checkpoint,
            commands::db_save_batch,
            commands::db_get_batches,
            commands::db_get_batch_results,
            commands::db_delete_batches,
            commands::db_delete_batches_older_than,
            commands::db_clear_all_batches,
            commands::db_get_scatter_data,
            commands::db_get_all_countries,
            commands::get_data_directory_info,
            commands::open_data_directory,
            commands::export_user_data_archive,
            commands::clear_user_data,
            commands::prepare_app_exit,
        ])
        .setup(|app| {
            info!("Tauri 应用 setup 完成，窗口创建完毕");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
