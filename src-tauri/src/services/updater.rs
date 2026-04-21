//! 版本更新模块：负责 GitHub 版本检查、客户端更新包下载。

use std::time::Duration;

use serde::Deserialize;
use semver::Version;

use crate::models::{ClientUpdateDownloadResult, ClientUpdateStatus};

const CLIENT_RELEASES_API: &str = "https://api.github.com/repos/rroy233/capyspeedtest/releases?per_page=30";

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    assets: Vec<GitHubAsset>,
    #[serde(default)]
    draft: bool,
}

/// 检查客户端更新并返回显式错误。
pub async fn try_check_client_update(current_version: &str) -> Result<ClientUpdateStatus, String> {
    let releases = fetch_releases(CLIENT_RELEASES_API).await?;
    let release = select_latest_semver_release(&releases)?;
    let latest_version = normalize_semver(&release.tag_name)?.to_string();
    let has_update = compare_versions(&latest_version, current_version) > 0;
    let download_url = select_release_asset(release)
        .map(|item| item.browser_download_url.clone())
        .unwrap_or_else(|| release.html_url.clone());
    Ok(ClientUpdateStatus {
        current_version: current_version.to_string(),
        latest_version,
        has_update,
        download_url,
        release_notes: release.body.clone().unwrap_or_default(),
    })
}

async fn fetch_releases(api_url: &str) -> Result<Vec<GitHubRelease>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("capyspeedtest/0.1")
        .build()
        .map_err(|error| format!("初始化 GitHub 客户端失败: {error}"))?;
    let releases = client
        .get(api_url)
        .send()
        .await
        .map_err(|error| format!("获取最新版本失败: {error}"))?
        .error_for_status()
        .map_err(|error| format!("版本检查响应异常: {error}"))?
        .json::<Vec<GitHubRelease>>()
        .await
        .map_err(|error| format!("解析版本信息失败: {error}"))?;
    if releases.is_empty() {
        return Err("版本列表为空".to_string());
    }
    Ok(releases)
}

fn normalize_semver(tag: &str) -> Result<Version, String> {
    Version::parse(tag.trim_start_matches('v'))
        .map_err(|error| format!("无法解析语义化版本号 `{}`: {}", tag, error))
}

fn select_latest_semver_release(releases: &[GitHubRelease]) -> Result<&GitHubRelease, String> {
    let mut best_release: Option<&GitHubRelease> = None;
    let mut best_version: Option<Version> = None;

    for release in releases {
        if release.draft {
            continue;
        }
        let Ok(version) = normalize_semver(&release.tag_name) else {
            continue;
        };
        if best_version.as_ref().is_none_or(|current| version > *current) {
            best_version = Some(version);
            best_release = Some(release);
        }
    }

    best_release.ok_or_else(|| "未找到可用的语义化版本发布".to_string())
}

fn select_release_asset(release: &GitHubRelease) -> Option<&GitHubAsset> {
    if release.assets.is_empty() {
        return None;
    }

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let assets = &release.assets;

    let matches = |asset: &GitHubAsset, patterns: &[&str]| {
        let name = asset.name.to_ascii_lowercase();
        patterns.iter().all(|pattern| name.contains(pattern))
    };

    if os == "windows" {
        return assets
            .iter()
            .find(|asset| matches(asset, &["windows", ".msi"]))
            .or_else(|| assets.iter().find(|asset| matches(asset, &["windows", ".exe"])))
            .or_else(|| assets.first());
    }

    if os == "macos" {
        let mac_tag = if arch == "aarch64" {
            "applesilicon"
        } else {
            "intel"
        };
        return assets
            .iter()
            .find(|asset| matches(asset, &["macos", mac_tag]))
            .or_else(|| assets.iter().find(|asset| matches(asset, &["macos"])))
            .or_else(|| assets.first());
    }

    if os == "linux" {
        let linux_arch = if arch == "aarch64" { "arm64" } else { "x86_64" };
        return assets
            .iter()
            .find(|asset| matches(asset, &["linux", linux_arch, ".appimage"]))
            .or_else(|| assets.iter().find(|asset| matches(asset, &["linux", linux_arch, ".deb"])))
            .or_else(|| assets.iter().find(|asset| matches(asset, &["linux", ".appimage"])))
            .or_else(|| assets.iter().find(|asset| matches(asset, &["linux", ".deb"])))
            .or_else(|| assets.first());
    }

    assets.first()
}

/// 比较两个语义化版本号。返回正数 if left > right。
fn compare_versions(left: &str, right: &str) -> i32 {
    let normalize = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    let left_parts = normalize(left);
    let right_parts = normalize(right);
    let max_len = left_parts.len().max(right_parts.len());

    for i in 0..max_len {
        let l = left_parts.get(i).copied().unwrap_or(0);
        let r = right_parts.get(i).copied().unwrap_or(0);
        if l != r {
            return l as i32 - r as i32;
        }
    }
    0
}

/// 返回客户端更新包保存路径。
fn client_package_path() -> Result<std::path::PathBuf, String> {
    let base = crate::services::state::app_data_root()?;
    Ok(base.join("updates"))
}

/// 下载客户端更新包并做校验/回滚保护（异步）。
pub async fn download_client_update_package(
    version: &str,
    download_url: &str,
    expected_sha256: Option<&str>,
) -> Result<ClientUpdateDownloadResult, String> {
    let target_path = client_package_path()?;
    let parent = target_path
        .parent()
        .ok_or_else(|| "无法获取更新目录".to_string())?;

    let parent_owned = parent.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&parent_owned).map_err(|e| format!("创建更新目录失败: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))??;

    let temp_path = target_path.with_extension("download");
    download_file_async(download_url, &temp_path, 3).await?;

    if let Some(expected) = expected_sha256 {
        let temp_owned = temp_path.to_path_buf();
        let expected_owned = expected.to_string();
        let _ =
            tokio::task::spawn_blocking(move || verify_sha256_file(&temp_owned, &expected_owned))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {e}"))??;
    }

    let backup_path = target_path.with_extension("bak");
    let backup_result = if target_path.exists() {
        let target_owned = target_path.to_path_buf();
        let backup_owned = backup_path.to_path_buf();
        let n = tokio::task::spawn_blocking(move || {
            std::fs::copy(&target_owned, &backup_owned)
                .map(|_| backup_owned.display().to_string())
                .map_err(|e| format!("创建更新回滚备份失败: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking 失败: {e}"))??;
        Some(n)
    } else {
        None
    };

    if let Err(error) = replace_file_with_backup_async(&temp_path, &target_path).await {
        if backup_path.exists() {
            let bp = backup_path.to_path_buf();
            let tp = target_path.to_path_buf();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = std::fs::copy(&bp, &tp);
            })
            .await;
        }
        return Err(error);
    }

    Ok(ClientUpdateDownloadResult {
        version: version.to_string(),
        package_path: target_path.display().to_string(),
        backup_path: backup_result,
        rolled_back: false,
    })
}

async fn download_file_async(
    url: &str,
    target_path: &std::path::Path,
    retries: usize,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("capyspeedtest/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    for attempt in 0..=retries {
        match download_file_once_async(&client, url, target_path).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < retries => {
                eprintln!("下载失败 (尝试 {}/{}): {}", attempt + 1, retries + 1, e);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

async fn download_file_once_async(
    client: &reqwest::Client,
    url: &str,
    target_path: &std::path::Path,
) -> Result<(), String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载响应异常: {e}"))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取下载内容失败: {e}"))?;

    let target = target_path.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        std::fs::write(&target, &bytes).map_err(|e| format!("写入文件失败: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))??;

    Ok(())
}

async fn replace_file_with_backup_async(
    temp_path: &std::path::Path,
    target_path: &std::path::Path,
) -> Result<(), String> {
    let temp = temp_path.to_path_buf();
    let target = target_path.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        std::fs::rename(&temp, &target).map_err(|e| format!("文件替换失败: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))??;

    Ok(())
}

fn verify_sha256_file(path: &std::path::Path, expected_sha256: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(path).map_err(|e| format!("读取文件失败: {e}"))?;
    let hash = format!("{:x}", Sha256::digest(&data));
    if hash != expected_sha256 {
        return Err(format!(
            "SHA256 校验失败: 期望 {} 得到 {}",
            expected_sha256, hash
        ));
    }
    Ok(())
}
