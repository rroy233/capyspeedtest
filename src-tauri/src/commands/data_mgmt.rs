//! 数据管理命令：目录信息、导出、清理。

use crate::commands::fs_utils::{add_dir_to_zip, collect_dir_stats};
use crate::models::{DataDirectoryInfo, UserDataExportResult};
use crate::services;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

/// 获取本地用户数据目录信息。
#[tauri::command]
pub fn get_data_directory_info() -> Result<DataDirectoryInfo, String> {
    let root = services::state_app_data_root()?;
    let logs = root.join("logs");
    let (total_bytes, file_count) = collect_dir_stats(&root)?;
    Ok(DataDirectoryInfo {
        path: root.display().to_string(),
        logs_path: logs.display().to_string(),
        total_bytes,
        file_count,
    })
}

/// 在系统文件管理器中打开用户数据目录。
#[tauri::command]
pub fn open_data_directory() -> Result<(), String> {
    let root = services::state_app_data_root()?;

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&root)
            .spawn()
            .map_err(|e| format!("打开数据目录失败({root:?}): {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&root)
            .spawn()
            .map_err(|e| format!("打开数据目录失败({root:?}): {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&root)
            .spawn()
            .map_err(|e| format!("打开数据目录失败({root:?}): {e}"))?;
    }

    Ok(())
}

/// 导出当前用户数据目录到 zip 包。
#[tauri::command]
pub fn export_user_data_archive() -> Result<UserDataExportResult, String> {
    let root = services::state_app_data_root()?;
    let export_dir = root.join("exports");
    fs::create_dir_all(&export_dir)
        .map_err(|e| format!("创建导出目录失败({export_dir:?}): {e}"))?;

    let archive_name = format!(
        "capyspeedtest-user-data-{}.zip",
        services::current_timestamp()
    );
    let archive_path = export_dir.join(archive_name);
    let file = fs::File::create(&archive_path)
        .map_err(|e| format!("创建导出文件失败({archive_path:?}): {e}"))?;
    let mut writer = zip::ZipWriter::new(file);

    add_dir_to_zip(&mut writer, &root, &root, &export_dir)?;
    writer.finish().map_err(|e| format!("完成导出失败: {e}"))?;

    Ok(UserDataExportResult {
        archive_path: archive_path.display().to_string(),
    })
}

/// 清理用户历史数据（保留运行资产：kernels、geoip）。
#[tauri::command]
pub fn clear_user_data() -> Result<(), String> {
    let root = services::state_app_data_root()?;
    if !root.exists() {
        return Ok(());
    }

    let preserve_entries = ["kernels", "geoip"];
    let entries = fs::read_dir(&root).map_err(|e| format!("读取数据目录失败({root:?}): {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败({root:?}): {e}"))?;
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };
        if preserve_entries.contains(&file_name) {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| format!("清理目录失败({path:?}): {e}"))?;
        } else {
            fs::remove_file(&path).map_err(|e| format!("清理文件失败({path:?}): {e}"))?;
        }
    }

    Ok(())
}

/// 应用退出前收尾清理：关闭所有 Mihomo 进程，移除测速临时配置与遗留本地历史缓存文件。
#[tauri::command]
pub fn prepare_app_exit() -> Result<(), String> {
    info!("[命令] prepare_app_exit 开始");

    // 1. 关闭所有 Mihomo 进程
    services::kernel::MihomoProcessRegistry::global().shutdown_all();

    let app_data = services::state_app_data_root()?;

    // 2. 清理配置文件目录
    let speedtest_configs_dir = app_data.join("speedtest_configs");
    if speedtest_configs_dir.exists() {
        fs::remove_dir_all(&speedtest_configs_dir)
            .map_err(|e| format!("清理测速配置目录失败({:?}): {}", speedtest_configs_dir, e))?;
        info!("[命令] 已清理测速配置目录: {:?}", speedtest_configs_dir);
    }

    // 3. 清理遗留历史文件
    let legacy_history_file = app_data.join("speedtest_history.json");
    if legacy_history_file.exists() {
        fs::remove_file(&legacy_history_file)
            .map_err(|e| format!("清理遗留历史文件失败({:?}): {}", legacy_history_file, e))?;
        info!("[命令] 已清理遗留历史文件: {:?}", legacy_history_file);
    }

    info!("[命令] prepare_app_exit 完成");
    Ok(())
}
