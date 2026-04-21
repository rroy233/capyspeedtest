//! 版本更新模块：负责 GitHub 版本检查、客户端更新包下载。

use std::time::Duration;

use semver::Version;
use serde::Deserialize;

use crate::models::{ClientUpdateDownloadResult, ClientUpdateStatus};
use crate::services::http_client::shared_http_client;

const CLIENT_RELEASES_API: &str =
    "https://api.github.com/repos/rroy233/capyspeedtest/releases?per_page=30";

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
    let client = shared_http_client()?;
    let releases = client
        .get(api_url)
        .timeout(Duration::from_secs(30))
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
        if best_version
            .as_ref()
            .is_none_or(|current| version > *current)
        {
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
            .or_else(|| {
                assets
                    .iter()
                    .find(|asset| matches(asset, &["windows", ".exe"]))
            })
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
            .or_else(|| {
                assets
                    .iter()
                    .find(|asset| matches(asset, &["linux", linux_arch, ".deb"]))
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|asset| matches(asset, &["linux", ".appimage"]))
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|asset| matches(asset, &["linux", ".deb"]))
            })
            .or_else(|| assets.first());
    }

    assets.first()
}

/// 比较两个语义化版本号。返回正数 if left > right。
/// 遵循 semver 2.0.0 规范：major.minor.patch 数值比较优先，
/// 若基版本相同，有 prerelease 的版本视为更小。
fn compare_versions(left: &str, right: &str) -> i32 {
    let left_v = Version::parse(left.trim_start_matches('v'));
    let right_v = Version::parse(right.trim_start_matches('v'));

    match (left_v, right_v) {
        (Ok(l), Ok(r)) => l.cmp(&r) as i32,
        // Fallback: custom numeric comparison for non-semver strings
        _ => {
            let left_nums: Vec<u32> = left
                .trim_start_matches('v')
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
            let right_nums: Vec<u32> = right
                .trim_start_matches('v')
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
            let max_len = left_nums.len().max(right_nums.len());

            for i in 0..max_len {
                let l = left_nums.get(i).copied().unwrap_or(0);
                let r = right_nums.get(i).copied().unwrap_or(0);
                if l != r {
                    return l as i32 - r as i32;
                }
            }
            0
        }
    }
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
    let client = shared_http_client()?;

    for attempt in 0..=retries {
        match download_file_once_async(client, url, target_path).await {
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
        .timeout(Duration::from_secs(60))
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

// =============================================================================
// Unit tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // normalize_semver
    // -------------------------------------------------------------------------
    #[test]
    fn test_normalize_semver_with_v_prefix() {
        assert_eq!(normalize_semver("v1.2.3").unwrap(), Version::new(1, 2, 3));
    }

    #[test]
    fn test_normalize_semver_without_v_prefix() {
        assert_eq!(normalize_semver("2.0.0").unwrap(), Version::new(2, 0, 0));
    }

    #[test]
    fn test_normalize_semver_invalid() {
        assert!(normalize_semver("not-a-version").is_err());
    }

    // -------------------------------------------------------------------------
    // compare_versions
    // -------------------------------------------------------------------------
    #[test]
    fn test_compare_versions_newer() {
        assert!(compare_versions("1.2.3", "1.2.0") > 0);
        assert!(compare_versions("2.0.0", "1.9.9") > 0);
        assert!(compare_versions("1.0.1", "1.0.0") > 0);
    }

    #[test]
    fn test_compare_versions_older() {
        assert!(compare_versions("1.2.0", "1.2.3") < 0);
        assert!(compare_versions("1.9.9", "2.0.0") < 0);
    }

    #[test]
    fn test_compare_versions_equal() {
        assert_eq!(compare_versions("1.2.3", "1.2.3"), 0);
        assert_eq!(compare_versions("v1.0.0", "1.0.0"), 0);
    }

    #[test]
    fn test_compare_versions_different_lengths() {
        assert!(compare_versions("1.2.3.4", "1.2.3") > 0);
        assert!(compare_versions("1.2.3", "1.2.3.4") < 0);
    }

    #[test]
    fn test_compare_versions_ignores_v_prefix() {
        assert_eq!(compare_versions("v1.0.0", "1.0.0"), 0);
        assert!(compare_versions("v2.0.0", "1.0.0") > 0);
    }

    // -------------------------------------------------------------------------
    // compare_versions — prerelease
    // -------------------------------------------------------------------------
    #[test]
    fn test_compare_versions_prerelease_vs_stable() {
        // prerelease 版本视为更低（semver 规范）
        assert!(compare_versions("v1.0.1", "v1.0.1-beta1") > 0);
        assert!(compare_versions("v1.0.1-beta1", "v1.0.1") < 0);
        assert!(compare_versions("v2.0.0", "v2.0.0-rc1") > 0);
        assert!(compare_versions("v2.0.0-rc1", "v2.0.0") < 0);
    }

    #[test]
    fn test_compare_versions_prerelease_between_themselves() {
        // alpha < beta < rc < stable
        assert!(compare_versions("v1.0.0-beta1", "v1.0.0-alpha") > 0);
        assert!(compare_versions("v1.0.0-rc1", "v1.0.0-beta1") > 0);
        assert!(compare_versions("v1.0.0-beta2", "v1.0.0-beta1") > 0);
    }

    #[test]
    fn test_compare_versions_prerelease_different_lengths() {
        assert!(compare_versions("v1.0.0-beta.1", "v1.0.0-beta") > 0);
        assert!(compare_versions("v1.0.0-alpha.1.2.3", "v1.0.0-alpha.1") > 0);
    }

    // -------------------------------------------------------------------------
    // normalize_semver — prerelease
    // -------------------------------------------------------------------------
    #[test]
    fn test_normalize_semver_prerelease() {
        // normalize_semver 使用 Version::parse，保留 prerelease 标签
        let v1 = normalize_semver("v1.0.1-beta1").unwrap();
        assert_eq!((v1.major, v1.minor, v1.patch), (1, 0, 1));
        assert_eq!(v1.pre.as_str(), "beta1");

        let v2 = normalize_semver("v2.0.0-rc1").unwrap();
        assert_eq!((v2.major, v2.minor, v2.patch), (2, 0, 0));
        assert_eq!(v2.pre.as_str(), "rc1");

        let v3 = normalize_semver("v3.0.0-alpha.2").unwrap();
        assert_eq!((v3.major, v3.minor, v3.patch), (3, 0, 0));
        assert_eq!(v3.pre.as_str(), "alpha.2");
    }

    #[test]
    fn test_normalize_semver_prerelease_with_build() {
        // prerelease + build metadata: 1.0.0-beta+build123
        let v = normalize_semver("v1.0.0-beta+build123").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (1, 0, 0));
        assert_eq!(v.pre.as_str(), "beta");
        assert_eq!(v.build.as_str(), "build123");
    }

    // -------------------------------------------------------------------------
    // select_latest_semver_release — prerelease
    // -------------------------------------------------------------------------
    #[test]
    fn test_select_latest_semver_release_stable_beats_prerelease() {
        // semver::Version 认定 v2.0.0 > v2.0.0-beta regardless of prerelease tag
        let releases = vec![
            make_release("v2.0.0-beta", false),
            make_release("v2.0.0", false),
        ];
        let result = select_latest_semver_release(&releases).unwrap();
        assert_eq!(result.tag_name, "v2.0.0");
    }

    // -------------------------------------------------------------------------
    // select_latest_semver_release
    // -------------------------------------------------------------------------
    fn make_release(tag: &str, draft: bool) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.to_string(),
            html_url: "https://github.com/test/test".to_string(),
            body: None,
            assets: vec![],
            draft,
        }
    }

    #[test]
    fn test_select_latest_semver_release_picks_highest() {
        let releases = vec![
            make_release("v1.0.0", false),
            make_release("v2.0.0", false),
            make_release("v1.5.0", false),
        ];
        let result = select_latest_semver_release(&releases).unwrap();
        assert_eq!(result.tag_name, "v2.0.0");
    }

    #[test]
    fn test_select_latest_semver_release_skips_draft() {
        let releases = vec![make_release("v3.0.0", true), make_release("v2.0.0", false)];
        let result = select_latest_semver_release(&releases).unwrap();
        assert_eq!(result.tag_name, "v2.0.0");
    }

    #[test]
    fn test_select_latest_semver_release_skips_invalid() {
        let releases = vec![
            make_release("not-semver", false),
            make_release("v2.0.0", false),
        ];
        let result = select_latest_semver_release(&releases).unwrap();
        assert_eq!(result.tag_name, "v2.0.0");
    }

    #[test]
    fn test_select_latest_semver_release_empty() {
        let releases: Vec<GitHubRelease> = vec![];
        assert!(select_latest_semver_release(&releases).is_err());
    }

    #[test]
    fn test_select_latest_semver_release_all_drafts() {
        let releases = vec![make_release("v3.0.0", true), make_release("v2.0.0", true)];
        assert!(select_latest_semver_release(&releases).is_err());
    }

    // -------------------------------------------------------------------------
    // select_release_asset — Windows
    // -------------------------------------------------------------------------
    #[cfg(target_os = "windows")]
    mod windows_asset_tests {
        use super::*;

        fn windows_asset(name: &str) -> GitHubAsset {
            GitHubAsset {
                name: name.to_string(),
                browser_download_url: format!("https://example.com/{}", name),
            }
        }

        fn release_with_assets(names: &[&str]) -> GitHubRelease {
            GitHubRelease {
                tag_name: "v1.0.0".to_string(),
                html_url: "https://github.com/test/test".to_string(),
                body: None,
                assets: names.iter().map(|n| windows_asset(n)).collect(),
                draft: false,
            }
        }

        #[test]
        fn test_windows_prefers_msi() {
            let release = release_with_assets(&[
                "capyspeedtest_windows_x64.exe",
                "capyspeedtest_windows_x64.msi",
            ]);
            let asset = select_release_asset(&release).unwrap();
            assert!(asset.name.ends_with(".msi"));
        }

        #[test]
        fn test_windows_falls_back_to_exe() {
            let release = release_with_assets(&["capyspeedtest_windows_x64.exe"]);
            let asset = select_release_asset(&release).unwrap();
            assert!(asset.name.ends_with(".exe"));
        }

        #[test]
        fn test_windows_falls_back_to_first() {
            let release = release_with_assets(&["capyspeedtest_linux_x64"]);
            let asset = select_release_asset(&release).unwrap();
            assert_eq!(asset.name, "capyspeedtest_linux_x64");
        }

        #[test]
        fn test_windows_empty_assets() {
            let release = release_with_assets(&[]);
            assert!(select_release_asset(&release).is_none());
        }
    }

    // -------------------------------------------------------------------------
    // select_release_asset — Linux
    // -------------------------------------------------------------------------
    #[cfg(target_os = "linux")]
    mod linux_asset_tests {
        use super::*;

        fn linux_asset(name: &str) -> GitHubAsset {
            GitHubAsset {
                name: name.to_string(),
                browser_download_url: format!("https://example.com/{}", name),
            }
        }

        fn release_with_assets(names: &[&str]) -> GitHubRelease {
            GitHubRelease {
                tag_name: "v1.0.0".to_string(),
                html_url: "https://github.com/test/test".to_string(),
                body: None,
                assets: names.iter().map(|n| linux_asset(n)).collect(),
                draft: false,
            }
        }

        #[test]
        fn test_linux_prefers_arch_specific_appimage() {
            let release = release_with_assets(&[
                "capyspeedtest_linux_x86_64.deb",
                "capyspeedtest_linux_x86_64.appimage",
            ]);
            let asset = select_release_asset(&release).unwrap();
            assert!(asset.name.contains("appimage"));
        }

        #[test]
        fn test_linux_falls_back_to_deb() {
            let release = release_with_assets(&["capyspeedtest_linux_x86_64.deb"]);
            let asset = select_release_asset(&release).unwrap();
            assert!(asset.name.contains("deb"));
        }
    }

    // -------------------------------------------------------------------------
    // select_release_asset — macOS
    // -------------------------------------------------------------------------
    #[cfg(target_os = "macos")]
    mod macos_asset_tests {
        use super::*;

        fn macos_asset(name: &str) -> GitHubAsset {
            GitHubAsset {
                name: name.to_string(),
                browser_download_url: format!("https://example.com/{}", name),
            }
        }

        fn release_with_assets(names: &[&str]) -> GitHubRelease {
            GitHubRelease {
                tag_name: "v1.0.0".to_string(),
                html_url: "https://github.com/test/test".to_string(),
                body: None,
                assets: names.iter().map(|n| macos_asset(n)).collect(),
                draft: false,
            }
        }

        #[test]
        fn test_macos_prefers_applesilicon() {
            let release = release_with_assets(&[
                "capyspeedtest_macos_intel",
                "capyspeedtest_macos_applesilicon",
            ]);
            let asset = select_release_asset(&release).unwrap();
            assert!(asset.name.contains("applesilicon"));
        }

        #[test]
        fn test_macos_falls_back_to_intel() {
            let release = release_with_assets(&["capyspeedtest_macos_intel"]);
            let asset = select_release_asset(&release).unwrap();
            assert!(asset.name.contains("intel"));
        }
    }
}
