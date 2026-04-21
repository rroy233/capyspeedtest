//! 更新检查相关命令：客户端更新、内核/GeoIP 检查、定时任务。

use crate::commands::{parse_ts_seconds, runtime_app_version, AppState, WEEK_SECONDS};
use crate::models::{
    ClientUpdateDownloadResult, ClientUpdateStatus, KernelGeoIpCheckResult,
    ScheduledUpdateCheckResult, UpdateCheckProgressEvent, UpdateDownloadProgressEvent,
};
use crate::services;
use tauri::Emitter;
use tracing::{info, warn};

/// 检查客户端是否存在更新（异步）。
#[tauri::command]
pub async fn check_client_update(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    current_version: String,
) -> Result<ClientUpdateStatus, String> {
    info!("[命令] check_client_update current={}", current_version);

    let _ = window.emit(
        "updater://check/progress",
        &UpdateCheckProgressEvent {
            stage: "checking".to_string(),
            progress: 50.0,
            message: "正在检查更新...".to_string(),
        },
    );

    let result = match services::try_check_client_update(&current_version).await {
        Ok(status) => status,
        Err(error) => {
            let _ = window.emit(
                "updater://check/progress",
                &UpdateCheckProgressEvent {
                    stage: "error".to_string(),
                    progress: 0.0,
                    message: format!("检查失败: {}", error),
                },
            );
            return Err(error);
        }
    };
    let checked_at = services::current_timestamp();
    {
        let mut checked_guard = state.client_update_last_checked_at.lock().unwrap();
        *checked_guard = checked_at.clone();
    }
    {
        let mut cache_guard = state.client_update_cache.lock().unwrap();
        *cache_guard = Some(result.clone());
    }
    let result_for_save = result.clone();
    let checked_at_for_save = checked_at.clone();
    let _ = tokio::task::spawn_blocking(move || {
        services::update_persisted_state(move |persisted| {
            persisted.client_update_last_checked_at = checked_at_for_save;
            persisted.client_update_cache = Some(result_for_save);
        })
    })
    .await;

    let _ = window.emit(
        "updater://check/progress",
        &UpdateCheckProgressEvent {
            stage: "completed".to_string(),
            progress: 100.0,
            message: if result.has_update {
                format!("发现新版本 {}", result.latest_version)
            } else {
                "已是最新版本".to_string()
            },
        },
    );

    info!(
        "[命令] check_client_update has_update={}, latest={}",
        result.has_update, result.latest_version
    );
    Ok(result)
}

/// 手动检查 Mihomo 内核版本列表与 GeoIP 版本（不下载）。
#[tauri::command]
pub async fn check_kernel_geoip_updates(
    state: tauri::State<'_, AppState>,
) -> Result<KernelGeoIpCheckResult, String> {
    let platform = services::detect_platform();
    let local_versions = services::kernel::list_local_kernel_versions(&platform).unwrap_or_default();
    let current_kernel = state.kernel_version.lock().unwrap().clone();
    let current_exists = services::kernel::kernel_binary_exists(&platform, &current_kernel)
        .unwrap_or(false);
    let versions = services::list_kernel_versions(&platform).await;
    let now = services::current_timestamp();
    let ip_database_version = state.ip_database_version.lock().unwrap().clone();
    let geoip_exists = services::geoip_database_exists().unwrap_or(false);

    {
        let mut cache_guard = state.cached_kernel_versions.lock().unwrap();
        *cache_guard = versions.clone();
    }
    {
        let mut checked_guard = state.kernel_last_checked_at.lock().unwrap();
        *checked_guard = now.clone();
    }
    {
        let mut checked_guard = state.geoip_last_checked_at.lock().unwrap();
        *checked_guard = now.clone();
    }

    let versions_for_save = versions.clone();
    let now_for_save = now.clone();
    tokio::task::spawn_blocking(move || {
        let _ = services::update_persisted_state(move |persisted| {
            persisted.cached_kernel_versions = versions_for_save;
            persisted.kernel_last_checked_at = now_for_save.clone();
            persisted.geoip_last_checked_at = now_for_save;
        });
    });

    let mut installed = versions;
    if !installed.contains(&current_kernel) {
        installed.insert(0, current_kernel.clone());
    }

    Ok(KernelGeoIpCheckResult {
        kernel: crate::models::KernelStatus {
            platform,
            current_version: current_kernel,
            installed_versions: installed,
            current_exists,
            local_installed_versions: local_versions,
            last_checked_at: now.clone(),
        },
        ip_database: crate::models::IpDatabaseStatus {
            current_version: ip_database_version,
            current_exists: geoip_exists,
            latest_version: services::latest_ip_database_version(),
            last_checked_at: now,
        },
    })
}

/// 按调度周期执行后台检查：内核/GeoIP 每周一次，客户端更新每次启动检查一次。
#[tauri::command]
pub async fn run_scheduled_update_checks(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    current_client_version: Option<String>,
) -> Result<ScheduledUpdateCheckResult, String> {
    let now = parse_ts_seconds(&services::current_timestamp());

    let kernel_last_checked =
        parse_ts_seconds(&state.kernel_last_checked_at.lock().unwrap().clone());
    if now - kernel_last_checked >= WEEK_SECONDS {
        let platform = services::detect_platform();
        let versions = services::list_kernel_versions(&platform).await;
        let checked_at = now.to_string();
        {
            let mut cache_guard = state.cached_kernel_versions.lock().unwrap();
            *cache_guard = versions.clone();
        }
        {
            let mut checked_guard = state.kernel_last_checked_at.lock().unwrap();
            *checked_guard = checked_at.clone();
        }
        let versions_for_save = versions.clone();
        let checked_for_save = checked_at.clone();
        tokio::task::spawn_blocking(move || {
            let _ = services::update_persisted_state(move |persisted| {
                persisted.cached_kernel_versions = versions_for_save;
                persisted.kernel_last_checked_at = checked_for_save;
            });
        });
    }

    let geoip_last_checked = parse_ts_seconds(&state.geoip_last_checked_at.lock().unwrap().clone());
    if now - geoip_last_checked >= WEEK_SECONDS {
        let checked_at = now.to_string();
        {
            let mut checked_guard = state.geoip_last_checked_at.lock().unwrap();
            *checked_guard = checked_at.clone();
        }
        let checked_for_save = checked_at.clone();
        tokio::task::spawn_blocking(move || {
            let _ = services::update_persisted_state(move |persisted| {
                persisted.geoip_last_checked_at = checked_for_save;
            });
        });
    }

    let current_version = current_client_version.unwrap_or_else(|| runtime_app_version(&app));

    match services::try_check_client_update(&current_version).await {
        Ok(status) => {
            let checked_at = now.to_string();
            {
                let mut checked_guard = state.client_update_last_checked_at.lock().unwrap();
                *checked_guard = checked_at.clone();
            }
            {
                let mut cache_guard = state.client_update_cache.lock().unwrap();
                *cache_guard = Some(status.clone());
            }
            let status_for_save = status.clone();
            let checked_for_save = checked_at.clone();
            tokio::task::spawn_blocking(move || {
                let _ = services::update_persisted_state(move |persisted| {
                    persisted.client_update_last_checked_at = checked_for_save;
                    persisted.client_update_cache = Some(status_for_save);
                });
            });
            Ok(ScheduledUpdateCheckResult {
                client_update: if status.has_update {
                    Some(status)
                } else {
                    None
                },
            })
        }
        Err(error) => {
            warn!(
                "[命令] run_scheduled_update_checks 客户端更新检查失败: {}",
                error
            );
            let checked_at = now.to_string();
            {
                let mut checked_guard = state.client_update_last_checked_at.lock().unwrap();
                *checked_guard = checked_at.clone();
            }
            {
                let mut cache_guard = state.client_update_cache.lock().unwrap();
                *cache_guard = None;
            }
            let checked_for_save = checked_at.clone();
            tokio::task::spawn_blocking(move || {
                let _ = services::update_persisted_state(move |persisted| {
                    persisted.client_update_last_checked_at = checked_for_save;
                    persisted.client_update_cache = None;
                });
            });
            Ok(ScheduledUpdateCheckResult {
                client_update: None,
            })
        }
    }
}

/// 下载并校验客户端更新包，失败时保留回滚备份（异步）。
#[tauri::command]
pub async fn download_client_update(
    window: tauri::Window,
    version: String,
    download_url: String,
    expected_sha256: Option<String>,
) -> Result<ClientUpdateDownloadResult, String> {
    info!(
        "[命令] download_client_update version={}, url={}",
        version, download_url
    );

    let _ = window.emit(
        "updater://download/progress",
        &UpdateDownloadProgressEvent {
            version: version.clone(),
            stage: "downloading".to_string(),
            progress: 10.0,
            message: "正在下载更新包...".to_string(),
        },
    );

    let result = services::download_client_update_package(
        &version,
        &download_url,
        expected_sha256.as_deref(),
    )
    .await;

    match &result {
        Ok(r) => {
            let _ = window.emit(
                "updater://download/progress",
                &UpdateDownloadProgressEvent {
                    version: version.clone(),
                    stage: "verifying".to_string(),
                    progress: 90.0,
                    message: "正在验证更新包...".to_string(),
                },
            );
            info!(
                "[命令] download_client_update 成功, path={}",
                r.package_path
            );
        }
        Err(e) => {
            let _ = window.emit(
                "updater://download/progress",
                &UpdateDownloadProgressEvent {
                    version: version.clone(),
                    stage: "error".to_string(),
                    progress: 0.0,
                    message: format!("下载失败: {}", e),
                },
            );
            tracing::error!("[命令] download_client_update 失败: {}", e);
        }
    }
    result
}
