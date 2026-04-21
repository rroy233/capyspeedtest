//! 设置相关命令：内核版本列表/切换、IP库刷新。

use crate::commands::{runtime_app_version, AppState};
use crate::models::{
    GeoIpDownloadProgressEvent, IpDatabaseStatus, KernelDownloadProgressEvent,
    KernelListProgressEvent, KernelStatus, SettingsSnapshot, UpdatePreferences,
};
use crate::services;
use tauri::Emitter;
use tracing::{error, info, warn};

/// 获取设置页所需的聚合快照。
#[tauri::command]
pub async fn get_settings_snapshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    current_client_version: Option<String>,
) -> Result<SettingsSnapshot, String> {
    info!(
        "[命令] get_settings_snapshot called, client_version={:?}",
        current_client_version
    );
    let platform = services::detect_platform();
    info!("[命令] detect_platform={}", platform);

    let current_kernel = state.kernel_version.lock().unwrap().clone();
    info!("[命令] current_kernel={}", current_kernel);

    let current_ip_database = state.ip_database_version.lock().unwrap().clone();
    info!("[命令] current_ip_database={}", current_ip_database);
    let geoip_exists = services::geoip_database_exists().unwrap_or(false);
    let geoip_latest_version = services::latest_ip_database_version();

    let kernel_last_checked_at = state.kernel_last_checked_at.lock().unwrap().clone();
    let geoip_last_checked_at = state.geoip_last_checked_at.lock().unwrap().clone();

    let local_installed = match services::kernel::list_local_kernel_versions(&platform) {
        Ok(versions) => versions,
        Err(error) => {
            warn!("[命令] get_settings_snapshot 读取本地内核失败: {}", error);
            Vec::new()
        }
    };
    {
        let mut local_guard = state.installed_kernel_versions.lock().unwrap();
        *local_guard = local_installed.clone();
    }
    let current_exists =
        services::kernel::kernel_binary_exists(&platform, &current_kernel).unwrap_or(false);

    let mut installed = state.cached_kernel_versions.lock().unwrap().clone();
    if installed.is_empty() {
        installed = local_installed.clone();
    }
    if !installed.contains(&current_kernel) {
        installed.insert(0, current_kernel.clone());
    }

    let current_version_owned = current_client_version.unwrap_or_else(|| runtime_app_version(&app));
    let current_version = current_version_owned.as_str();
    let client_update = state
        .client_update_cache
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| crate::models::ClientUpdateStatus {
            current_version: current_version.to_string(),
            latest_version: "-".to_string(),
            has_update: false,
            download_url: String::new(),
            release_notes: String::new(),
        });

    let result = SettingsSnapshot {
        kernel: KernelStatus {
            platform,
            current_version: current_kernel,
            installed_versions: installed,
            current_exists,
            local_installed_versions: local_installed,
            last_checked_at: kernel_last_checked_at,
        },
        ip_database: IpDatabaseStatus {
            current_version: current_ip_database,
            current_exists: geoip_exists,
            latest_version: geoip_latest_version,
            last_checked_at: geoip_last_checked_at,
        },
        client_update,
        update_preferences: UpdatePreferences {
            receive_prerelease: *state.receive_prerelease_updates.lock().unwrap(),
        },
    };
    info!("[命令] get_settings_snapshot 返回成功");
    Ok(result)
}

/// 返回指定平台可用的内核版本。
#[tauri::command]
pub async fn list_kernel_versions_cmd(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    platform: Option<String>,
    force_refresh: Option<bool>,
) -> Result<Vec<String>, String> {
    let platform = platform.unwrap_or_else(services::detect_platform);
    let force_refresh = force_refresh.unwrap_or(false);
    info!("[命令] list_kernel_versions_cmd platform={}", platform);

    if !force_refresh {
        let cached = state.cached_kernel_versions.lock().unwrap().clone();
        if !cached.is_empty() {
            let _ = window.emit(
                "kernel://list/progress",
                &KernelListProgressEvent {
                    stage: "completed".to_string(),
                    progress: 100.0,
                    message: format!("读取缓存版本列表，共 {} 个版本", cached.len()),
                    versions_count: Some(cached.len()),
                },
            );
            return Ok(cached);
        }
    }

    let _ = window.emit(
        "kernel://list/progress",
        &KernelListProgressEvent {
            stage: "fetching".to_string(),
            progress: 30.0,
            message: "正在获取内核版本列表...".to_string(),
            versions_count: None,
        },
    );

    let result = services::list_kernel_versions(&platform).await;
    let checked_at = services::current_timestamp();
    {
        let mut cache_guard = state.cached_kernel_versions.lock().unwrap();
        *cache_guard = result.clone();
    }
    {
        let mut checked_guard = state.kernel_last_checked_at.lock().unwrap();
        *checked_guard = checked_at.clone();
    }
    let cached_versions = result.clone();
    let checked_at_for_save = checked_at.clone();
    tokio::task::spawn_blocking(move || {
        let _ = services::update_persisted_state(move |persisted| {
            persisted.cached_kernel_versions = cached_versions;
            persisted.kernel_last_checked_at = checked_at_for_save;
        });
    });

    let _ = window.emit(
        "kernel://list/progress",
        &KernelListProgressEvent {
            stage: "completed".to_string(),
            progress: 100.0,
            message: format!("获取到 {} 个版本", result.len()),
            versions_count: Some(result.len()),
        },
    );

    info!(
        "[命令] list_kernel_versions_cmd 返回 {} 个版本",
        result.len()
    );
    Ok(result)
}

/// 切换当前使用的内核版本（异步，不阻塞事件循环）。
#[tauri::command]
pub async fn select_kernel_version(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    version: String,
) -> Result<KernelStatus, String> {
    info!("[命令] select_kernel_version version={}", version);
    let platform = services::detect_platform();
    let mut available_versions = state.cached_kernel_versions.lock().unwrap().clone();
    if available_versions.is_empty() {
        available_versions = state.installed_kernel_versions.lock().unwrap().clone();
    }
    if available_versions.is_empty() {
        available_versions = services::DEFAULT_KERNEL_VERSIONS
            .iter()
            .map(|item| item.to_string())
            .collect();
    }
    if !available_versions.contains(&version) {
        error!(
            "[命令] select_kernel_version 版本 {} 不在可用列表中",
            version
        );
        return Err(format!("版本 {version} 不在当前平台可用列表中"));
    }

    let version_for_emit = version.clone();
    let progress_window = window.clone();
    let error_window = window.clone();
    let downloaded_path = services::kernel::download_kernel_version_with_progress(
        &platform,
        &version,
        move |event| {
            let _ = progress_window.emit("kernel://download/progress", &event);
        },
    )
    .await
    .map_err(|error| {
        let _ = error_window.emit(
            "kernel://download/progress",
            &KernelDownloadProgressEvent {
                version: version_for_emit.clone(),
                stage: "error".to_string(),
                progress: 0.0,
                message: format!("下载失败: {}", error),
            },
        );
        error
    })?;
    info!("[命令] select_kernel_version 下载完成: {}", downloaded_path);

    let current_ip_database = state.ip_database_version.lock().unwrap().clone();

    {
        let mut guard = state.kernel_version.lock().unwrap();
        *guard = version.clone();
    }
    {
        let mut guard = state.installed_kernel_versions.lock().unwrap();
        if !guard.contains(&version) {
            guard.push(version.clone());
        }
    }

    let installed_versions = state.installed_kernel_versions.lock().unwrap().clone();

    let v = version.clone();
    let ip = current_ip_database.clone();
    let ik = installed_versions.clone();
    tokio::task::spawn_blocking(move || {
        let _ = services::persist_runtime_state(&v, &ik, &ip);
    })
    .await
    .map_err(|e| format!("持久化失败: {e}"))?;

    info!("[命令] select_kernel_version 成功切换到 {}", version);
    Ok(KernelStatus {
        platform,
        current_version: version,
        installed_versions: installed_versions.clone(),
        current_exists: true,
        local_installed_versions: installed_versions,
        last_checked_at: services::current_timestamp(),
    })
}

/// 刷新 IP 库到最新版本（异步）。
#[tauri::command]
pub async fn refresh_ip_database(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
) -> Result<IpDatabaseStatus, String> {
    info!("[命令] refresh_ip_database 开始刷新");

    let _ = window.emit(
        "geoip://download/progress",
        &GeoIpDownloadProgressEvent {
            stage: "downloading".to_string(),
            progress: 10.0,
            message: "正在下载 GeoIP 数据库...".to_string(),
        },
    );

    let latest = services::download_geoip_database().await?;

    let _ = window.emit(
        "geoip://download/progress",
        &GeoIpDownloadProgressEvent {
            stage: "completed".to_string(),
            progress: 100.0,
            message: format!("已更新到 {}", latest),
        },
    );

    info!("[命令] refresh_ip_database 下载完成, version={}", latest);

    let current_kernel = state.kernel_version.lock().unwrap().clone();
    let installed_versions = state.installed_kernel_versions.lock().unwrap().clone();

    {
        let mut guard = state.ip_database_version.lock().unwrap();
        *guard = latest.clone();
    }
    let checked_at = services::current_timestamp();
    {
        let mut guard = state.geoip_last_checked_at.lock().unwrap();
        *guard = checked_at.clone();
    }

    let ik = installed_versions.clone();
    let ck = current_kernel.clone();
    let lt = latest.clone();
    let checked_at_for_save = checked_at.clone();
    tokio::task::spawn_blocking(move || {
        let _ = services::persist_runtime_state(&ck, &ik, &lt);
        let _ = services::update_persisted_state(move |persisted| {
            persisted.geoip_last_checked_at = checked_at_for_save;
        });
    })
    .await
    .map_err(|e| format!("持久化失败: {e}"))?;

    info!("[命令] refresh_ip_database 成功更新到 {}", latest);
    Ok(IpDatabaseStatus {
        current_version: latest,
        current_exists: true,
        latest_version: services::latest_ip_database_version(),
        last_checked_at: checked_at,
    })
}
