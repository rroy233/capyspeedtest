//! 更新检查相关命令：客户端更新、内核/GeoIP 检查、定时任务。

use crate::commands::{parse_ts_seconds, runtime_app_version, AppState, WEEK_SECONDS};
use crate::models::{
    ClientUpdateDownloadResult, ClientUpdateStatus, KernelGeoIpCheckResult,
    ScheduledUpdateCheckResult, UpdateCheckProgressEvent, UpdateDownloadProgressEvent,
    UpdatePreferences,
};
use crate::services;
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

fn is_prerelease_version(version: &str) -> bool {
    version.contains('-')
}

/// 检查客户端是否存在更新（异步）。
#[tauri::command]
pub async fn check_client_update(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    _current_version: String,
) -> Result<ClientUpdateStatus, String> {
    let current_version = runtime_app_version(&app);
    let receive_prerelease = *state.receive_prerelease_updates.lock().unwrap();
    info!("[命令] check_client_update current={}", current_version);

    let _ = window.emit(
        "updater://check/progress",
        &UpdateCheckProgressEvent {
            stage: "checking".to_string(),
            progress: 50.0,
            message: "正在检查更新...".to_string(),
        },
    );

    let update = match app.updater_builder().build() {
        Ok(updater) => match updater.check().await {
            Ok(result) => result,
            Err(error) => {
                let message = format!("检查失败: {}", error);
                let _ = window.emit(
                    "updater://check/progress",
                    &UpdateCheckProgressEvent {
                        stage: "error".to_string(),
                        progress: 0.0,
                        message: message.clone(),
                    },
                );
                return Err(message);
            }
        },
        Err(error) => {
            let message = format!("初始化更新器失败: {}", error);
            let _ = window.emit(
                "updater://check/progress",
                &UpdateCheckProgressEvent {
                    stage: "error".to_string(),
                    progress: 0.0,
                    message: message.clone(),
                },
            );
            return Err(message);
        }
    };

    let result = if let Some(update) = update {
        if !receive_prerelease && is_prerelease_version(&update.version) {
            ClientUpdateStatus {
                current_version: current_version.clone(),
                latest_version: current_version.clone(),
                has_update: false,
                download_url: String::new(),
                release_notes: String::new(),
            }
        } else {
        ClientUpdateStatus {
            current_version: current_version.clone(),
            latest_version: update.version.clone(),
            has_update: true,
            download_url: String::new(),
            release_notes: update.body.unwrap_or_default(),
        }
        }
    } else {
        ClientUpdateStatus {
            current_version: current_version.clone(),
            latest_version: current_version.clone(),
            has_update: false,
            download_url: String::new(),
            release_notes: String::new(),
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
    let local_versions =
        services::kernel::list_local_kernel_versions(&platform).unwrap_or_default();
    let current_kernel = state.kernel_version.lock().unwrap().clone();
    let current_exists =
        services::kernel::kernel_binary_exists(&platform, &current_kernel).unwrap_or(false);
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
    let receive_prerelease = *state.receive_prerelease_updates.lock().unwrap();

    let update_result = match app.updater_builder().build() {
        Ok(updater) => updater.check().await.map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };

    match update_result {
        Ok(update) => {
            let status = if let Some(update) = update {
                if !receive_prerelease && is_prerelease_version(&update.version) {
                    ClientUpdateStatus {
                        current_version: current_version.clone(),
                        latest_version: current_version.clone(),
                        has_update: false,
                        download_url: String::new(),
                        release_notes: String::new(),
                    }
                } else {
                    ClientUpdateStatus {
                        current_version: current_version.clone(),
                        latest_version: update.version.clone(),
                        has_update: true,
                        download_url: String::new(),
                        release_notes: update.body.unwrap_or_default(),
                    }
                }
            } else {
                ClientUpdateStatus {
                    current_version: current_version.clone(),
                    latest_version: current_version.clone(),
                    has_update: false,
                    download_url: String::new(),
                    release_notes: String::new(),
                }
            };
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
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    version: Option<String>,
) -> Result<ClientUpdateDownloadResult, String> {
    let label_version = version.unwrap_or_else(|| "latest".to_string());
    info!("[命令] download_client_update version={}", label_version);

    let _ = window.emit(
        "updater://download/progress",
        &UpdateDownloadProgressEvent {
            version: label_version.clone(),
            stage: "downloading".to_string(),
            progress: 0.0,
            message: "正在下载更新包...".to_string(),
        },
    );

    let receive_prerelease = *state.receive_prerelease_updates.lock().unwrap();

    let update = match app.updater_builder().build() {
        Ok(updater) => match updater.check().await {
            Ok(Some(update)) => {
                if !receive_prerelease && is_prerelease_version(&update.version) {
                    let message = "当前已禁用预发布版本更新".to_string();
                    let _ = window.emit(
                        "updater://download/progress",
                        &UpdateDownloadProgressEvent {
                            version: update.version.clone(),
                            stage: "error".to_string(),
                            progress: 0.0,
                            message: message.clone(),
                        },
                    );
                    return Err(message);
                }
                update
            }
            Ok(None) => {
                let message = "当前没有可安装更新".to_string();
                let _ = window.emit(
                    "updater://download/progress",
                    &UpdateDownloadProgressEvent {
                        version: label_version.clone(),
                        stage: "error".to_string(),
                        progress: 0.0,
                        message: message.clone(),
                    },
                );
                return Err(message);
            }
            Err(error) => {
                let message = format!("检查更新失败: {}", error);
                let _ = window.emit(
                    "updater://download/progress",
                    &UpdateDownloadProgressEvent {
                        version: label_version.clone(),
                        stage: "error".to_string(),
                        progress: 0.0,
                        message: message.clone(),
                    },
                );
                return Err(message);
            }
        },
        Err(error) => {
            let message = format!("初始化更新器失败: {}", error);
            let _ = window.emit(
                "updater://download/progress",
                &UpdateDownloadProgressEvent {
                    version: label_version.clone(),
                    stage: "error".to_string(),
                    progress: 0.0,
                    message: message.clone(),
                },
            );
            return Err(message);
        }
    };

    let mut downloaded_bytes: u64 = 0;
    let target_version = update.version.clone();

    let install_result = update
        .download_and_install(
            |chunk_length, content_length| {
                downloaded_bytes += chunk_length as u64;
                let progress = content_length
                    .map(|total| {
                        if total == 0 {
                            0.0
                        } else {
                            ((downloaded_bytes as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
                                as f32
                        }
                    })
                    .unwrap_or(0.0);

                let message = content_length
                    .map(|total| format!("已下载 {downloaded_bytes}/{total} 字节"))
                    .unwrap_or_else(|| format!("已下载 {downloaded_bytes} 字节"));

                let _ = window.emit(
                    "updater://download/progress",
                    &UpdateDownloadProgressEvent {
                        version: target_version.clone(),
                        stage: "downloading".to_string(),
                        progress,
                        message,
                    },
                );
            },
            || {
                let _ = window.emit(
                    "updater://download/progress",
                    &UpdateDownloadProgressEvent {
                        version: target_version.clone(),
                        stage: "verifying".to_string(),
                        progress: 100.0,
                        message: "下载完成，正在安装更新...".to_string(),
                    },
                );
            },
        )
        .await;

    match install_result {
        Ok(()) => {
            let _ = window.emit(
                "updater://download/progress",
                &UpdateDownloadProgressEvent {
                    version: target_version.clone(),
                    stage: "completed".to_string(),
                    progress: 100.0,
                    message: "更新安装完成，正在重启应用...".to_string(),
                },
            );

            #[cfg(not(target_os = "windows"))]
            {
                app.restart();
            }

            #[cfg(target_os = "windows")]
            {
                Ok(ClientUpdateDownloadResult {
                    version: target_version,
                    package_path: "managed-by-tauri-updater".to_string(),
                    backup_path: None,
                    rolled_back: false,
                })
            }
        }
        Err(e) => {
            let message = format!("安装更新失败: {}", e);
            let _ = window.emit(
                "updater://download/progress",
                &UpdateDownloadProgressEvent {
                    version: target_version,
                    stage: "error".to_string(),
                    progress: 0.0,
                    message: message.clone(),
                },
            );
            tracing::error!("[命令] download_client_update 失败: {}", e);
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn set_update_preferences(
    state: tauri::State<'_, AppState>,
    receive_prerelease: bool,
) -> Result<UpdatePreferences, String> {
    {
        let mut guard = state.receive_prerelease_updates.lock().unwrap();
        *guard = receive_prerelease;
    }

    let _ = tokio::task::spawn_blocking(move || {
        services::update_persisted_state(move |persisted| {
            persisted.receive_prerelease_updates = receive_prerelease;
        })
    })
    .await
    .map_err(|e| format!("保存更新偏好失败: {e}"))?
    .map_err(|e| format!("保存更新偏好失败: {e}"))?;

    Ok(UpdatePreferences { receive_prerelease })
}
