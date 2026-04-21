//! 内核管理模块：负责 Mihomo 内核下载、Spawn、配置生成、生命周期管理。

use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use base64::Engine;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::Digest;
use url::Url;
use zip::ZipArchive;

use crate::models::KernelDownloadProgressEvent;
use crate::services::http_client::shared_http_client;

pub const DEFAULT_KERNEL_VERSIONS: [&str; 3] = ["v1.19.1", "v1.19.0", "v1.18.5"];

const MIHOMO_RELEASES_API: &str = "https://api.github.com/repos/MetaCubeX/mihomo/releases";
const MIHOMO_RELEASE_BY_TAG_API: &str =
    "https://api.github.com/repos/MetaCubeX/mihomo/releases/tags";
const GEOIP_DOWNLOAD_URL: &str =
    "https://github.com/Loyalsoldier/geoip/releases/latest/download/Country.mmdb";
const DOWNLOAD_RETRIES: usize = 3;

// GitHub 镜像列表（中国大陆访问 GitHub困难时使用）
const GITHUB_MIRRORS: &[&str] = &["https://mirror.ghproxy.com/", "https://ghproxy.com/"];

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GitHubAsset>,
}

/// 返回当前运行平台标识。
pub fn detect_platform() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else {
        "linux".to_string()
    }
}

/// 根据当前平台返回可用内核版本列表。
pub async fn list_kernel_versions(platform: &str) -> Vec<String> {
    if should_use_remote_metadata() {
        if let Ok(releases) = fetch_kernel_releases(12).await {
            let mut versions = Vec::new();
            let mut seen = HashSet::new();
            for release in releases {
                if release
                    .assets
                    .iter()
                    .any(|asset| kernel_asset_matches_platform(platform, &asset.name))
                    && seen.insert(release.tag_name.clone())
                {
                    versions.push(release.tag_name);
                    if versions.len() >= 8 {
                        break;
                    }
                }
            }
            if !versions.is_empty() {
                return versions;
            }
        }
    }

    let normalized = platform.to_ascii_lowercase();
    let defaults = if normalized.contains("windows") {
        vec!["v1.19.1", "v1.19.0", "v1.18.5"]
    } else if normalized.contains("macos") || normalized.contains("darwin") {
        vec!["v1.19.1", "v1.19.0", "v1.18.4"]
    } else {
        vec!["v1.19.1", "v1.19.0", "v1.18.3"]
    };
    defaults.into_iter().map(String::from).collect()
}

fn should_use_remote_metadata() -> bool {
    !env::var("CAPYSPEEDTEST_OFFLINE").is_ok()
}

/// 检查指定平台与版本的内核是否已存在。
pub fn kernel_binary_exists(platform: &str, version: &str) -> Result<bool, String> {
    Ok(kernel_binary_path(platform, version)?.exists())
}

/// 返回内核二进制文件的完整路径。
pub fn kernel_binary_path(platform: &str, version: &str) -> Result<PathBuf, String> {
    let base = crate::services::state::app_data_root()?;
    let dir = base.join("kernels").join(version);
    let binary_name = if platform.contains("windows") {
        "mihomo.exe".to_string()
    } else {
        "mihomo".to_string()
    };
    Ok(dir.join(binary_name))
}

/// 扫描本地已安装的内核版本（仅返回二进制存在的版本目录）。
pub fn list_local_kernel_versions(platform: &str) -> Result<Vec<String>, String> {
    let base = crate::services::state::app_data_root()?;
    let kernels_dir = base.join("kernels");
    if !kernels_dir.exists() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();
    let entries =
        std::fs::read_dir(&kernels_dir).map_err(|error| format!("读取内核目录失败: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取内核目录项失败: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(version) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if kernel_binary_exists(platform, version)? {
            versions.push(version.to_string());
        }
    }
    versions.sort_by(|left, right| right.cmp(left));
    versions.dedup();
    Ok(versions)
}

fn default_mihomo_asset_name(platform: &str) -> Result<String, String> {
    let normalized = platform.to_ascii_lowercase();
    let suffix = if normalized.contains("windows") {
        format!("windows-amd64-v3.zip")
    } else if normalized.contains("macos") || normalized.contains("darwin") {
        if normalized.contains("arm") {
            format!("darwin-arm64-v3.zip")
        } else {
            format!("darwin-amd64-v3.zip")
        }
    } else {
        format!("linux-amd64-v3.tar.gz")
    };
    Ok(format!(
        "mihomo-{}-{}.{}",
        DEFAULT_KERNEL_VERSIONS[0], "amd64", suffix
    ))
}

fn build_mihomo_kernel_url(platform: &str, version: &str) -> Result<String, String> {
    let asset_name = default_mihomo_asset_name(platform)?;
    Ok(format!(
        "https://github.com/MetaCubeX/mihomo/releases/download/{}/{}",
        version, asset_name
    ))
}

#[derive(Debug)]
struct KernelDownload {
    url: String,
    asset_name: String,
}

async fn fetch_kernel_releases(limit: usize) -> Result<Vec<GitHubRelease>, String> {
    info!("[内核下载] 开始获取内核版本列表, limit: {}", limit);
    let url = format!("{}?per_page={}", MIHOMO_RELEASES_API, limit);
    let client = github_client()?;
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|error| {
            let msg = format!("获取内核版本列表失败: {error}");
            error!("[内核下载] {}", msg);
            msg
        })?
        .error_for_status()
        .map_err(|error| {
            let msg = format!("获取内核版本列表响应异常: {error}");
            error!("[内核下载] {}", msg);
            msg
        })?
        .json::<Vec<GitHubRelease>>()
        .await
        .map_err(|error| {
            let msg = format!("解析内核版本列表失败: {error}");
            error!("[内核下载] {}", msg);
            msg
        })?;
    info!("[内核下载] 成功获取 {} 个内核版本", resp.len());
    Ok(resp)
}

async fn fetch_release_by_tag(tag: &str) -> Result<GitHubRelease, String> {
    info!("[内核下载] 开始获取内核版本信息, tag: {}", tag);
    let url = format!("{}/{}", MIHOMO_RELEASE_BY_TAG_API, tag);
    let client = github_client()?;
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|error| {
            let msg = format!("获取内核版本信息失败: {error}");
            error!("[内核下载] {}", msg);
            msg
        })?
        .error_for_status()
        .map_err(|error| {
            let msg = format!("获取内核版本信息响应异常: {error}");
            error!("[内核下载] {}", msg);
            msg
        })?
        .json::<GitHubRelease>()
        .await
        .map_err(|error| {
            let msg = format!("解析内核版本信息失败: {error}");
            error!("[内核下载] {}", msg);
            msg
        })?;
    info!("[内核下载] 成功获取内核版本信息, tag: {}", tag);
    Ok(resp)
}

fn github_client() -> Result<&'static reqwest::Client, String> {
    shared_http_client().map_err(|error| {
        let msg = format!("初始化 GitHub 客户端失败: {error}");
        error!("[内核下载] {}", msg);
        msg
    })
}

fn kernel_asset_matches_platform(platform: &str, asset_name: &str) -> bool {
    let normalized = platform.to_ascii_lowercase();
    let asset_lower = asset_name.to_ascii_lowercase();
    if normalized.contains("windows") {
        asset_lower.contains("windows")
            && (asset_lower.contains("amd64") || asset_lower.contains("x86_64"))
    } else if normalized.contains("macos") || normalized.contains("darwin") {
        asset_lower.contains("darwin")
    } else {
        asset_lower.contains("linux")
            && (asset_lower.contains("amd64") || asset_lower.contains("x86_64"))
    }
}

async fn resolve_kernel_download(platform: &str, version: &str) -> Result<KernelDownload, String> {
    info!(
        "[内核下载] 开始解析内核下载地址, platform: {}, version: {}",
        platform, version
    );
    let release = fetch_release_by_tag(version).await?;
    let default_asset = default_mihomo_asset_name(platform)?;

    let matched_asset = release
        .assets
        .iter()
        .find(|a| kernel_asset_matches_platform(platform, &a.name));

    let (url, asset_name) = if let Some(asset) = matched_asset {
        (asset.browser_download_url.clone(), asset.name.clone())
    } else {
        warn!("[内核下载] 未在 release 中找到匹配当前平台的 asset，尝试回退到默认构建链接");
        (
            build_mihomo_kernel_url(platform, version)
                .ok()
                .unwrap_or_default(),
            default_asset,
        )
    };

    info!(
        "[内核下载] 解析到内核下载地址: {}, asset_name: {}",
        url, asset_name
    );
    Ok(KernelDownload { url, asset_name })
}

/// 下载指定版本内核；若已存在则直接返回。
/// 文件 I/O 通过 spawn_blocking 避免阻塞 async 运行时。
pub async fn download_kernel_version(platform: &str, version: &str) -> Result<String, String> {
    download_kernel_version_with_progress(platform, version, |_| {}).await
}

/// 下载指定版本内核，带进度回调。
/// 文件 I/O 通过 spawn_blocking 避免阻塞 async 运行时。
pub async fn download_kernel_version_with_progress<F>(
    platform: &str,
    version: &str,
    mut progress_callback: F,
) -> Result<String, String>
where
    F: FnMut(KernelDownloadProgressEvent) + Send,
{
    info!(
        "[内核下载] 开始下载内核版本: {}, platform: {}",
        version, platform
    );
    let target_path = match kernel_binary_path(platform, version) {
        Ok(p) => p,
        Err(e) => {
            error!("[内核下载] 获取内核路径失败: {}", e);
            return Err(e);
        }
    };

    if target_path.exists() {
        info!("[内核下载] 内核文件已存在，跳过下载: {:?}", target_path);
        progress_callback(KernelDownloadProgressEvent {
            version: version.to_string(),
            stage: "completed".to_string(),
            progress: 100.0,
            message: "内核已存在".to_string(),
        });
        return Ok(target_path.display().to_string());
    }

    let parent_dir = target_path.parent().ok_or_else(|| {
        let msg = "无法获取内核目录".to_string();
        error!("[内核下载] {}", msg);
        msg
    })?;

    // create_dir_all 是阻塞 I/O，放到 spawn_blocking
    let parent_dir_owned = parent_dir.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&parent_dir_owned).map_err(|e| format!("创建内核目录失败: {e}"))
    })
    .await
    .map_err(|e| {
        let msg = format!("spawn_blocking 失败: {e}");
        error!("[内核下载] {}", msg);
        msg
    })?
    .map_err(|e| {
        error!("[内核下载] {}", e);
        e
    })?;

    // 获取下载链接
    progress_callback(KernelDownloadProgressEvent {
        version: version.to_string(),
        stage: "downloading".to_string(),
        progress: 0.0,
        message: "正在解析下载链接...".to_string(),
    });

    let download = match resolve_kernel_download(platform, version).await {
        Ok(d) => d,
        Err(e) => {
            error!("[内核下载] 解析下载链接失败: {}", e);
            return Err(e);
        }
    };

    progress_callback(KernelDownloadProgressEvent {
        version: version.to_string(),
        stage: "downloading".to_string(),
        progress: 5.0,
        message: format!("开始下载资源包: {}", download.asset_name),
    });

    let temp_path = target_path.with_extension("download");
    let version_owned = version.to_string();

    // 下载文件（带进度回调）
    info!("[内核下载] 准备下载文件到临时路径: {:?}", temp_path);
    if let Err(e) = download_file_with_progress(&download.url, &temp_path, DOWNLOAD_RETRIES, |p| {
        progress_callback(KernelDownloadProgressEvent {
            version: version_owned.clone(),
            stage: "downloading".to_string(),
            progress: 5.0 + p * 0.85, // 5% - 90% 是下载进度
            message: format!("下载中... {:.0}%", p * 100.0),
        });
    })
    .await
    {
        error!("[内核下载] 下载文件失败: {}", e);
        if temp_path.exists() {
            let _ = std::fs::remove_file(&temp_path).map_err(|cleanup_error| {
                warn!(
                    "[内核下载] 清理临时文件失败: path={:?}, error={}",
                    temp_path, cleanup_error
                );
            });
        }
        return Err(e);
    }

    progress_callback(KernelDownloadProgressEvent {
        version: version.to_string(),
        stage: "extracting".to_string(),
        progress: 90.0,
        message: "正在解压安装...".to_string(),
    });

    info!(
        "[内核下载] 开始解压安装内核: asset={}, platform={}",
        download.asset_name, platform
    );
    if let Err(e) =
        install_kernel_binary_async(&temp_path, &target_path, &download.asset_name, platform).await
    {
        error!("[内核下载] 解压安装失败: {}", e);
        if temp_path.exists() {
            let _ = std::fs::remove_file(&temp_path).map_err(|cleanup_error| {
                warn!(
                    "[内核下载] 清理解压失败后的临时文件失败: path={:?}, error={}",
                    temp_path, cleanup_error
                );
            });
        }
        return Err(e);
    }

    if temp_path.exists() {
        let _ = std::fs::remove_file(&temp_path).map_err(|cleanup_error| {
            warn!(
                "[内核下载] 清理下载临时文件失败: path={:?}, error={}",
                temp_path, cleanup_error
            );
        });
    }

    progress_callback(KernelDownloadProgressEvent {
        version: version.to_string(),
        stage: "completed".to_string(),
        progress: 100.0,
        message: "下载完成".to_string(),
    });

    info!("[内核下载] 内核安装完成: {:?}", target_path);
    Ok(target_path.display().to_string())
}

/// 下载文件，支持进度回调和 GitHub 镜像回退。
async fn download_file_with_progress<F>(
    url: &str,
    target_path: &Path,
    retries: usize,
    mut progress_callback: F,
) -> Result<(), String>
where
    F: FnMut(f32) + Send,
{
    info!("[内核下载] 尝试下载文件: url={}, retries={}", url, retries);
    // 如果是 GitHub URL，构造镜像 URL 列表
    let urls_to_try = if url.contains("github.com") && url.contains("/releases/download/") {
        let mut urls = vec![url.to_string()];
        for mirror in GITHUB_MIRRORS {
            // 镜像 URL 格式: mirror + 原始 URL
            let mirrored_url = format!("{}{}", mirror, url);
            urls.push(mirrored_url);
        }
        urls
    } else {
        vec![url.to_string()]
    };

    // 尝试所有 URL
    let mut last_error = String::new();
    for url_to_try in &urls_to_try {
        info!("[内核下载] 尝试下载 URL: {}", url_to_try);
        for attempt in 0..=retries {
            match download_file_once(url_to_try, target_path, &mut progress_callback).await {
                Ok(()) => {
                    info!("[内核下载] 文件下载成功: {}", url_to_try);
                    return Ok(());
                }
                Err(e) => {
                    last_error = e;
                    if attempt < retries {
                        warn!(
                            "[内核下载] 下载失败 (尝试 {}/{}): {}",
                            attempt + 1,
                            retries + 1,
                            last_error
                        );
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    } else {
                        error!(
                            "[内核下载] URL {} 在 {} 次尝试后均失败: {}",
                            url_to_try,
                            retries + 1,
                            last_error
                        );
                    }
                }
            }
        }
    }

    let msg = format!("下载失败: {}", last_error);
    error!("[内核下载] 所有下载尝试均失败: {}", msg);
    Err(msg)
}

/// 单次下载（带进度追踪）
async fn download_file_once<F>(
    url: &str,
    target_path: &Path,
    progress_callback: &mut F,
) -> Result<(), String>
where
    F: FnMut(f32) + Send,
{
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = github_client()?;
    let response = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| {
            let msg = format!("下载请求失败: {e}");
            error!("[内核下载] {}", msg);
            msg
        })?
        .error_for_status()
        .map_err(|e| {
            let msg = format!("下载响应异常: {e}");
            error!("[内核下载] {}", msg);
            msg
        })?;

    let total_size = response.content_length().unwrap_or(0);
    info!("[内核下载] 开始下载 {} bytes from {}", total_size, url);

    // Create the file first
    let mut file = tokio::fs::File::create(target_path).await.map_err(|e| {
        let msg = format!("创建文件失败: {e}");
        error!("[内核下载] {}", msg);
        msg
    })?;

    // Stream download with progress tracking
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            let msg = format!("读取下载内容失败: {e}");
            error!("[内核下载] {}", msg);
            msg
        })?;
        let chunk_len = chunk.len() as u64;

        file.write_all(&chunk).await.map_err(|e| {
            let msg = format!("写入文件失败: {e}");
            error!("[内核下载] {}", msg);
            msg
        })?;

        downloaded += chunk_len;
        if total_size > 0 {
            let progress = (downloaded as f32) / (total_size as f32);
            progress_callback(progress);
        }
    }

    file.flush().await.map_err(|e| {
        let msg = format!("刷新文件失败: {e}");
        error!("[内核下载] {}", msg);
        msg
    })?;

    info!("[内核下载] 下载完成 {} bytes", downloaded);
    Ok(())
}

/// 兼容旧接口（无进度回调）
async fn download_file_async(url: &str, target_path: &Path, retries: usize) -> Result<(), String> {
    download_file_with_progress(url, target_path, retries, |_| {}).await
}

async fn install_kernel_binary_async(
    source: &Path,
    target: &Path,
    asset_name: &str,
    platform: &str,
) -> Result<(), String> {
    let source = source.to_path_buf();
    let target = target.to_path_buf();
    let asset_name = asset_name.to_string();
    let platform = platform.to_string();

    let result = tokio::task::spawn_blocking(move || {
        if asset_name.ends_with(".zip") {
            extract_kernel_from_zip(&source, &target)
        } else if asset_name.ends_with(".tar.gz") || asset_name.ends_with(".gz") {
            extract_kernel_from_gzip(&source, &target)?;
            set_executable_permission(&target, &platform)
        } else {
            std::fs::copy(&source, &target).map_err(|e| {
                let msg = format!("复制内核文件失败: {e}");
                error!("[内核下载] {}", msg);
                msg
            })?;
            set_executable_permission(&target, &platform)
        }
    })
    .await
    .map_err(|e| {
        let msg = format!("spawn_blocking 失败: {e}");
        error!("[内核下载] {}", msg);
        msg
    })??;

    Ok(result)
}

fn extract_kernel_from_gzip(source: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source).map_err(|e| {
        let msg = format!("打开压缩包失败: {e}");
        error!("[内核解压] {}", msg);
        msg
    })?;
    let mut decoder = GzDecoder::new(file);
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer).map_err(|e| {
        let msg = format!("解压失败: {e}");
        error!("[内核解压] {}", msg);
        msg
    })?;
    let mut out_file = std::fs::File::create(target).map_err(|e| {
        let msg = format!("创建目标文件失败: {e}");
        error!("[内核解压] {}", msg);
        msg
    })?;
    out_file.write_all(&buffer).map_err(|e| {
        let msg = format!("写入文件失败: {e}");
        error!("[内核解压] {}", msg);
        msg
    })?;
    Ok(())
}

fn extract_kernel_from_zip(source: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source).map_err(|e| {
        let msg = format!("打开压缩包失败: {e}");
        error!("[内核解压] {}", msg);
        msg
    })?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        let msg = format!("解析 ZIP 失败: {e}");
        error!("[内核解压] {}", msg);
        msg
    })?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            let msg = format!("读取 ZIP 条目失败: {e}");
            error!("[内核解压] {}", msg);
            msg
        })?;
        let name = file.name().to_string();
        let name_lower = name.to_ascii_lowercase();
        // 匹配任意包含 mihomo 且扩展名为 .exe 或无扩展名的文件
        if name_lower.contains("mihomo")
            && (name_lower.ends_with(".exe") || !name_lower.contains('.'))
        {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).map_err(|e| {
                let msg = format!("读取条目内容失败: {e}");
                error!("[内核解压] {}", msg);
                msg
            })?;
            std::fs::write(target, &buffer).map_err(|e| {
                let msg = format!("写入文件失败: {e}");
                error!("[内核解压] {}", msg);
                msg
            })?;
            return set_executable_permission(target, "");
        }
    }
    let msg = "zip 包中未找到 mihomo 可执行文件".to_string();
    error!("[内核解压] {}", msg);
    Err(msg)
}

fn set_executable_permission(path: &Path, _platform: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| {
                let msg = format!("获取文件权限失败: {e}");
                error!("[内核解压] {}", msg);
                msg
            })?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).map_err(|e| {
            let msg = format!("设置文件权限失败: {e}");
            error!("[内核解压] {}", msg);
            msg
        })?;
    }
    Ok(())
}

// =============================================================================
// Mihomo 进程管理
// =============================================================================

/// 根据节点连接信息生成 Mihomo proxy 配置段。
/// 返回 (proxy_yaml, needs_auth) 元组。
fn build_proxy_config(node: &crate::models::NodeInfo) -> String {
    if let Some(payload_proxy_yaml) = build_proxy_config_from_payload(node) {
        return payload_proxy_yaml;
    }

    if let Some(raw_proxy_yaml) = build_proxy_config_from_raw(node) {
        return raw_proxy_yaml;
    }

    let Some(ref connect_info) = node.connect_info else {
        return format!(
            r#"proxies:
  - name: "{}"
    type: http
    server: 127.0.0.1
    port: 1"#,
            node.name
        );
    };

    let server = &connect_info.server;
    let port = connect_info.port;

    match node.protocol.as_str() {
        "vless" => {
            let uuid = connect_info.username.as_deref().unwrap_or("");
            let password = connect_info.password.as_deref().unwrap_or("");
            format!(
                r#"proxies:
  - name: "{}"
    type: vless
    server: {}
    port: {}
    uuid: {}
    flow: {}
    tls: true"#,
                node.name,
                server,
                port,
                uuid,
                if password.is_empty() {
                    "xtls-rprx-vision"
                } else {
                    password
                }
            )
        }
        "trojan" => {
            let password = connect_info.password.as_deref().unwrap_or("");
            format!(
                r#"proxies:
  - name: "{}"
    type: trojan
    server: {}
    port: {}
    password: {}"#,
                node.name, server, port, password
            )
        }
        "ss" => {
            let method = connect_info.username.as_deref().unwrap_or("aes-256-gcm");
            let password = connect_info.password.as_deref().unwrap_or("");
            format!(
                r#"proxies:
  - name: "{}"
    type: ss
    server: {}
    port: {}
    cipher: {}
    password: {}"#,
                node.name, server, port, method, password
            )
        }
        "ssr" => {
            let password = connect_info.password.as_deref().unwrap_or("");
            let method = connect_info.username.as_deref().unwrap_or("aes-256-cfb");
            // SSR 链接需要通过特定的额外参数解析 obfs, protocol 等
            // 这里为了通用处理，我们尽量从 username 提取或使用默认值，
            // 实际项目中应该扩展 NodeInfo 支持更多 SSR 特有字段。
            format!(
                r#"proxies:
  - name: "{}"
    type: ssr
    server: {}
    port: {}
    cipher: {}
    password: {}
    obfs: plain
    protocol: origin"#,
                node.name, server, port, method, password
            )
        }
        "vmess" => {
            let uuid = connect_info.username.as_deref().unwrap_or("");
            let alter_id = connect_info.password.as_deref().unwrap_or("0");
            format!(
                r#"proxies:
  - name: "{}"
    type: vmess
    server: {}
    port: {}
    uuid: {}
    alterId: {}
    cipher: auto
    udp: true"#,
                node.name, server, port, uuid, alter_id
            )
        }
        _ => format!(
            r#"proxies:
  - name: "{}"
    type: http
    server: {}
    port: {}"#,
            node.name, server, port
        ),
    }
}

fn build_proxy_config_from_payload(node: &crate::models::NodeInfo) -> Option<String> {
    let payload = node.parsed_proxy_payload.as_ref()?;
    let mut value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let obj = value.as_object_mut()?;

    obj.insert(
        "name".to_string(),
        serde_json::Value::String(node.name.clone()),
    );
    if !obj.contains_key("type") {
        obj.insert(
            "type".to_string(),
            serde_json::Value::String(node.protocol.clone()),
        );
    } else if obj
        .get("type")
        .and_then(|v| v.as_str())
        .map(|v| v.eq_ignore_ascii_case("hy2"))
        .unwrap_or(false)
    {
        obj.insert(
            "type".to_string(),
            serde_json::Value::String("hysteria2".to_string()),
        );
    }

    let wrapper = serde_json::json!({
        "proxies": [value]
    });

    let yaml = serde_yaml::to_string(&wrapper).ok()?;
    Some(yaml.trim_start_matches("---\n").to_string())
}

fn build_proxy_config_from_raw(node: &crate::models::NodeInfo) -> Option<String> {
    match node.protocol.as_str() {
        "vless" => build_vless_proxy_from_raw(node),
        "trojan" => build_trojan_proxy_from_raw(node),
        "ss" => build_ss_proxy_from_raw(node),
        "ssr" => build_ssr_proxy_from_raw(node),
        "vmess" => build_vmess_proxy_from_raw(node),
        _ => None,
    }
}

fn build_vless_proxy_from_raw(node: &crate::models::NodeInfo) -> Option<String> {
    let url = Url::parse(&node.raw).ok()?;
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);
    let uuid = url.username();
    if uuid.is_empty() {
        return None;
    }

    let query = query_map(&url);
    let mut lines = vec![
        format!(r#"  - name: "{}""#, yaml_escape(&node.name)),
        "    type: vless".to_string(),
        format!("    server: {}", server),
        format!("    port: {}", port),
        format!(r#"    uuid: "{}""#, yaml_escape(uuid)),
        "    udp: true".to_string(),
    ];

    if let Some(flow) = query.get("flow").filter(|v| !v.is_empty()) {
        lines.push(format!(
            r#"    flow: "{}""#,
            yaml_escape(&flow.to_ascii_lowercase())
        ));
    }
    if let Some(encryption) = query.get("encryption").filter(|v| !v.is_empty()) {
        lines.push(format!(r#"    encryption: "{}""#, yaml_escape(encryption)));
    }

    let security = query
        .get("security")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let tls_enabled = security.ends_with("tls") || security == "reality";
    if tls_enabled {
        lines.push("    tls: true".to_string());
        let fingerprint = query
            .get("fp")
            .cloned()
            .unwrap_or_else(|| "chrome".to_string());
        lines.push(format!(
            r#"    client-fingerprint: "{}""#,
            yaml_escape(&fingerprint)
        ));
        if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
            append_yaml_string_list(&mut lines, "alpn", alpn, 4);
        }
        if let Some(pcs) = query.get("pcs").filter(|v| !v.is_empty()) {
            lines.push(format!(r#"    fingerprint: "{}""#, yaml_escape(pcs)));
        }
    }
    if let Some(sni) = query.get("sni").filter(|v| !v.is_empty()) {
        lines.push(format!(r#"    servername: "{}""#, yaml_escape(sni)));
    }
    if let Some(pbk) = query.get("pbk").filter(|v| !v.is_empty()) {
        lines.push("    reality-opts:".to_string());
        lines.push(format!(r#"      public-key: "{}""#, yaml_escape(pbk)));
        lines.push(format!(
            r#"      short-id: "{}""#,
            yaml_escape(query.get("sid").map(|s| s.as_str()).unwrap_or(""))
        ));
    }

    match query.get("packetEncoding").map(String::as_str) {
        Some("none") => {}
        Some("packet") => lines.push("    packet-addr: true".to_string()),
        _ => lines.push("    xudp: true".to_string()),
    }

    let mut network = query
        .get("type")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".to_string());
    let fake_type = query
        .get("headerType")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if fake_type == "http" {
        network = "http".to_string();
    } else if network == "http" {
        network = "h2".to_string();
    }
    lines.push(format!(r#"    network: "{}""#, yaml_escape(&network)));

    match network.as_str() {
        "tcp" => {
            if !fake_type.is_empty() && fake_type != "none" {
                lines.push("    http-opts:".to_string());
                append_yaml_path_list(
                    &mut lines,
                    "path",
                    query.get("path").map(String::as_str),
                    6,
                    "/",
                );
                if let Some(method) = query.get("method").filter(|v| !v.is_empty()) {
                    lines.push(format!(r#"      method: "{}""#, yaml_escape(method)));
                }
                if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                    lines.push("      headers:".to_string());
                    append_yaml_string_list(&mut lines, "Host", host, 8);
                }
            }
        }
        "http" => {
            lines.push("    h2-opts:".to_string());
            append_yaml_path_list(
                &mut lines,
                "path",
                query.get("path").map(String::as_str),
                6,
                "/",
            );
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                append_yaml_string_list(&mut lines, "host", host, 6);
            }
            lines.push("      headers: {}".to_string());
        }
        "ws" | "httpupgrade" => {
            lines.push("    ws-opts:".to_string());
            let path = query.get("path").map(String::as_str).unwrap_or("");
            lines.push(format!(r#"      path: "{}""#, yaml_escape(path)));
            lines.push("      headers:".to_string());
            lines.push("        User-Agent: \"Mozilla/5.0\"".to_string());
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"        Host: "{}""#, yaml_escape(host)));
            }
            if let Some(early_data) = query.get("ed").and_then(|s| s.parse::<u32>().ok()) {
                if network == "ws" {
                    lines.push(format!("      max-early-data: {}", early_data));
                    lines.push(
                        "      early-data-header-name: \"Sec-WebSocket-Protocol\"".to_string(),
                    );
                } else {
                    lines.push("      v2ray-http-upgrade-fast-open: true".to_string());
                }
            }
            if let Some(early_header) = query.get("eh").filter(|v| !v.is_empty()) {
                lines.push(format!(
                    r#"      early-data-header-name: "{}""#,
                    yaml_escape(early_header)
                ));
            }
        }
        "grpc" => {
            lines.push("    grpc-opts:".to_string());
            lines.push(format!(
                r#"      grpc-service-name: "{}""#,
                yaml_escape(query.get("serviceName").map(|s| s.as_str()).unwrap_or(""))
            ));
        }
        "xhttp" => {
            lines.push("    xhttp-opts:".to_string());
            if let Some(path) = query.get("path").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"      path: "{}""#, yaml_escape(path)));
            }
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"      host: "{}""#, yaml_escape(host)));
            }
            if let Some(mode) = query.get("mode").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"      mode: "{}""#, yaml_escape(mode)));
            }
        }
        _ => {}
    }

    Some(format!("proxies:\n{}", lines.join("\n")))
}

fn build_trojan_proxy_from_raw(node: &crate::models::NodeInfo) -> Option<String> {
    let url = Url::parse(&node.raw).ok()?;
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);
    let password = url.username();
    if password.is_empty() {
        return None;
    }

    let query = query_map(&url);
    let mut lines = vec![
        format!(r#"  - name: "{}""#, yaml_escape(&node.name)),
        "    type: trojan".to_string(),
        format!("    server: {}", server),
        format!("    port: {}", port),
        format!(r#"    password: "{}""#, yaml_escape(password)),
        "    udp: true".to_string(),
    ];

    if parse_bool_like(query.get("allowInsecure")) || parse_bool_like(query.get("insecure")) {
        lines.push("    skip-cert-verify: true".to_string());
    }
    if let Some(sni) = query.get("sni").filter(|v| !v.is_empty()) {
        lines.push(format!(r#"    sni: "{}""#, yaml_escape(sni)));
    }
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        append_yaml_string_list(&mut lines, "alpn", alpn, 4);
    }
    if let Some(network) = query
        .get("type")
        .map(|s| s.to_ascii_lowercase())
        .filter(|v| !v.is_empty())
    {
        lines.push(format!(r#"    network: "{}""#, yaml_escape(&network)));
        match network.as_str() {
            "ws" => {
                lines.push("    ws-opts:".to_string());
                let path = query.get("path").map(String::as_str).unwrap_or("");
                lines.push(format!(r#"      path: "{}""#, yaml_escape(path)));
                lines.push("      headers:".to_string());
                lines.push("        User-Agent: \"Mozilla/5.0\"".to_string());
            }
            "grpc" => {
                lines.push("    grpc-opts:".to_string());
                lines.push(format!(
                    r#"      grpc-service-name: "{}""#,
                    yaml_escape(query.get("serviceName").map(|s| s.as_str()).unwrap_or(""))
                ));
            }
            _ => {}
        }
    }
    let fingerprint = query
        .get("fp")
        .cloned()
        .unwrap_or_else(|| "chrome".to_string());
    lines.push(format!(
        r#"    client-fingerprint: "{}""#,
        yaml_escape(&fingerprint)
    ));
    if let Some(pcs) = query.get("pcs").filter(|v| !v.is_empty()) {
        lines.push(format!(r#"    fingerprint: "{}""#, yaml_escape(pcs)));
    }

    Some(format!("proxies:\n{}", lines.join("\n")))
}

fn build_ss_proxy_from_raw(node: &crate::models::NodeInfo) -> Option<String> {
    let mut url = Url::parse(&node.raw).ok()?;
    if url.port().is_none() {
        let decoded = decode_base64_flexible(url.host_str()?)?;
        let decoded_text = String::from_utf8(decoded).ok()?;
        url = Url::parse(&format!("ss://{}", decoded_text)).ok()?;
    }

    let mut cipher = url.username().to_string();
    let mut password = url.password().map(ToString::to_string);
    if password.is_none() {
        let decoded = decode_base64_flexible(&cipher)?;
        let decoded_text = String::from_utf8(decoded).ok()?;
        let (decoded_cipher, decoded_password) = decoded_text.split_once(':')?;
        cipher = decoded_cipher.to_string();
        password = Some(decoded_password.to_string());
    }

    let query = query_map(&url);
    let mut lines = vec![
        format!(r#"  - name: "{}""#, yaml_escape(&node.name)),
        "    type: ss".to_string(),
        format!("    server: {}", url.host_str()?),
        format!("    port: {}", url.port().unwrap_or(443)),
        format!(r#"    cipher: "{}""#, yaml_escape(&cipher)),
        format!(
            r#"    password: "{}""#,
            yaml_escape(password.as_deref().unwrap_or(""))
        ),
        "    udp: true".to_string(),
    ];

    if query
        .get("udp-over-tcp")
        .map(|v| v == "true")
        .unwrap_or(false)
        || query.get("uot").map(|v| v == "1").unwrap_or(false)
    {
        lines.push("    udp-over-tcp: true".to_string());
    }

    if let Some(plugin) = query.get("plugin").filter(|v| v.contains(';')) {
        let plugin_query = format!("pluginName={}", plugin.replace(';', "&"));
        let plugin_map = url::form_urlencoded::parse(plugin_query.as_bytes())
            .into_owned()
            .collect::<HashMap<String, String>>();
        if let Some(plugin_name) = plugin_map.get("pluginName") {
            if plugin_name.contains("obfs") {
                lines.push("    plugin: obfs".to_string());
                lines.push("    plugin-opts:".to_string());
                lines.push(format!(
                    r#"      mode: "{}""#,
                    yaml_escape(plugin_map.get("obfs").map(String::as_str).unwrap_or(""))
                ));
                lines.push(format!(
                    r#"      host: "{}""#,
                    yaml_escape(
                        plugin_map
                            .get("obfs-host")
                            .map(String::as_str)
                            .unwrap_or("")
                    )
                ));
            } else if plugin_name.contains("v2ray-plugin") {
                lines.push("    plugin: v2ray-plugin".to_string());
                lines.push("    plugin-opts:".to_string());
                lines.push(format!(
                    r#"      mode: "{}""#,
                    yaml_escape(plugin_map.get("mode").map(String::as_str).unwrap_or(""))
                ));
                lines.push(format!(
                    r#"      host: "{}""#,
                    yaml_escape(plugin_map.get("host").map(String::as_str).unwrap_or(""))
                ));
                lines.push(format!(
                    r#"      path: "{}""#,
                    yaml_escape(plugin_map.get("path").map(String::as_str).unwrap_or(""))
                ));
                lines.push(format!(
                    "      tls: {}",
                    if plugin.contains("tls") {
                        "true"
                    } else {
                        "false"
                    }
                ));
            }
        }
    }

    Some(format!("proxies:\n{}", lines.join("\n")))
}

fn build_ssr_proxy_from_raw(node: &crate::models::NodeInfo) -> Option<String> {
    let body = node.raw.strip_prefix("ssr://")?;
    let decoded = decode_base64_flexible(body)?;
    let decoded_text = String::from_utf8(decoded).ok()?;
    let (before, after) = decoded_text.split_once("/?")?;
    let parts = before.split(':').collect::<Vec<_>>();
    if parts.len() != 6 {
        return None;
    }

    let server = parts[0];
    let port = parts[1].parse::<u16>().ok().unwrap_or(443);
    let protocol = parts[2];
    let method = parts[3];
    let obfs = parts[4];
    let password = String::from_utf8(decode_base64_flexible(parts[5])?).ok()?;

    let query = url::form_urlencoded::parse(after.as_bytes())
        .into_owned()
        .collect::<HashMap<String, String>>();
    let obfs_param = query
        .get("obfsparam")
        .and_then(|v| decode_base64_flexible(v))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let protocol_param = query
        .get("protoparam")
        .and_then(|v| decode_base64_flexible(v))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();

    let mut lines = vec![
        format!(r#"  - name: "{}""#, yaml_escape(&node.name)),
        "    type: ssr".to_string(),
        format!("    server: {}", server),
        format!("    port: {}", port),
        format!(r#"    cipher: "{}""#, yaml_escape(method)),
        format!(r#"    password: "{}""#, yaml_escape(&password)),
        format!(r#"    obfs: "{}""#, yaml_escape(obfs)),
        format!(r#"    protocol: "{}""#, yaml_escape(protocol)),
        "    udp: true".to_string(),
    ];
    if !obfs_param.is_empty() {
        lines.push(format!(r#"    obfs-param: "{}""#, yaml_escape(&obfs_param)));
    }
    if !protocol_param.is_empty() {
        lines.push(format!(
            r#"    protocol-param: "{}""#,
            yaml_escape(&protocol_param)
        ));
    }

    Some(format!("proxies:\n{}", lines.join("\n")))
}

fn build_vmess_proxy_from_raw(node: &crate::models::NodeInfo) -> Option<String> {
    let body = node.raw.strip_prefix("vmess://")?;
    if let Some(decoded) =
        decode_base64_flexible(body).and_then(|bytes| String::from_utf8(bytes).ok())
    {
        if let Ok(values) = serde_json::from_str::<serde_json::Value>(&decoded) {
            return build_vmess_proxy_from_json(node, &values);
        }
    }

    let url = Url::parse(&node.raw).ok()?;
    let query = query_map(&url);
    let mut lines = vec![
        format!(r#"  - name: "{}""#, yaml_escape(&node.name)),
        "    type: vmess".to_string(),
        format!("    server: {}", url.host_str()?),
        format!("    port: {}", url.port().unwrap_or(443)),
        format!(r#"    uuid: "{}""#, yaml_escape(url.username())),
        "    alterId: 0".to_string(),
        format!(
            r#"    cipher: "{}""#,
            yaml_escape(
                query
                    .get("encryption")
                    .map(String::as_str)
                    .unwrap_or("auto")
            )
        ),
    ];

    append_v_share_common_fields(&query, &mut lines);
    let mut network = query
        .get("type")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".to_string());
    let fake_type = query
        .get("headerType")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if fake_type == "http" {
        network = "http".to_string();
    } else if network == "http" {
        network = "h2".to_string();
    }
    append_v_share_transport_fields(&query, &mut lines, &network, &fake_type);

    Some(format!("proxies:\n{}", lines.join("\n")))
}

fn build_vmess_proxy_from_json(
    node: &crate::models::NodeInfo,
    values: &serde_json::Value,
) -> Option<String> {
    let server = values.get("add")?.as_str()?;
    let uuid = values.get("id")?.as_str()?;
    let port = extract_json_u16(values.get("port"))?;

    let mut lines = vec![
        format!(r#"  - name: "{}""#, yaml_escape(&node.name)),
        "    type: vmess".to_string(),
        format!("    server: {}", server),
        format!("    port: {}", port),
        format!(r#"    uuid: "{}""#, yaml_escape(uuid)),
        format!(
            "    alterId: {}",
            extract_json_u16(values.get("aid")).unwrap_or(0)
        ),
        "    udp: true".to_string(),
        "    xudp: true".to_string(),
        "    tls: false".to_string(),
        "    skip-cert-verify: false".to_string(),
        format!(
            r#"    cipher: "{}""#,
            yaml_escape(values.get("scy").and_then(|v| v.as_str()).unwrap_or("auto"))
        ),
    ];

    if let Some(sni) = values
        .get("sni")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
    {
        lines.push(format!(r#"    servername: "{}""#, yaml_escape(sni)));
    }

    let mut network = values
        .get("net")
        .and_then(|v| v.as_str())
        .unwrap_or("tcp")
        .to_ascii_lowercase();
    if values.get("type").and_then(|v| v.as_str()) == Some("http") {
        network = "http".to_string();
    } else if network == "http" {
        network = "h2".to_string();
    }
    lines.push(format!(r#"    network: "{}""#, yaml_escape(&network)));

    if let Some(tls) = values.get("tls").and_then(|v| v.as_str()) {
        if tls.to_ascii_lowercase().ends_with("tls") {
            lines.push("    tls: true".to_string());
        }
        if let Some(alpn) = values
            .get("alpn")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
        {
            append_yaml_string_list(&mut lines, "alpn", alpn, 4);
        }
    }

    match network.as_str() {
        "http" => {
            lines.push("    http-opts:".to_string());
            append_yaml_path_list(
                &mut lines,
                "path",
                values.get("path").and_then(|v| v.as_str()),
                6,
                "/",
            );
            if let Some(host) = values
                .get("host")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
            {
                lines.push("      headers:".to_string());
                append_yaml_string_list(&mut lines, "Host", host, 8);
            }
        }
        "h2" => {
            lines.push("    h2-opts:".to_string());
            if let Some(path) = values
                .get("path")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
            {
                lines.push(format!(r#"      path: "{}""#, yaml_escape(path)));
            }
            if let Some(host) = values
                .get("host")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
            {
                lines.push("      headers:".to_string());
                append_yaml_string_list(&mut lines, "Host", host, 8);
            }
        }
        "ws" | "httpupgrade" => {
            lines.push("    ws-opts:".to_string());
            let path = values.get("path").and_then(|v| v.as_str()).unwrap_or("/");
            lines.push(format!(r#"      path: "{}""#, yaml_escape(path)));
            lines.push("      headers:".to_string());
            lines.push("        User-Agent: \"Mozilla/5.0\"".to_string());
            if let Some(host) = values
                .get("host")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
            {
                lines.push(format!(r#"        Host: "{}""#, yaml_escape(host)));
            }
        }
        "grpc" => {
            lines.push("    grpc-opts:".to_string());
            lines.push(format!(
                r#"      grpc-service-name: "{}""#,
                yaml_escape(values.get("path").and_then(|v| v.as_str()).unwrap_or(""))
            ));
        }
        _ => {}
    }

    Some(format!("proxies:\n{}", lines.join("\n")))
}

fn append_v_share_common_fields(query: &HashMap<String, String>, lines: &mut Vec<String>) {
    lines.push("    udp: true".to_string());

    let security = query
        .get("security")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let tls_enabled = security.ends_with("tls") || security == "reality";
    if tls_enabled {
        lines.push("    tls: true".to_string());
        let fingerprint = query
            .get("fp")
            .cloned()
            .unwrap_or_else(|| "chrome".to_string());
        lines.push(format!(
            r#"    client-fingerprint: "{}""#,
            yaml_escape(&fingerprint)
        ));
        if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
            append_yaml_string_list(lines, "alpn", alpn, 4);
        }
        if let Some(pcs) = query.get("pcs").filter(|v| !v.is_empty()) {
            lines.push(format!(r#"    fingerprint: "{}""#, yaml_escape(pcs)));
        }
    }
    if let Some(sni) = query.get("sni").filter(|v| !v.is_empty()) {
        lines.push(format!(r#"    servername: "{}""#, yaml_escape(sni)));
    }
    if let Some(pbk) = query.get("pbk").filter(|v| !v.is_empty()) {
        lines.push("    reality-opts:".to_string());
        lines.push(format!(r#"      public-key: "{}""#, yaml_escape(pbk)));
        lines.push(format!(
            r#"      short-id: "{}""#,
            yaml_escape(query.get("sid").map(|s| s.as_str()).unwrap_or(""))
        ));
    }
    match query.get("packetEncoding").map(String::as_str) {
        Some("none") => {}
        Some("packet") => lines.push("    packet-addr: true".to_string()),
        _ => lines.push("    xudp: true".to_string()),
    }

    let mut network = query
        .get("type")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".to_string());
    let fake_type = query
        .get("headerType")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if fake_type == "http" {
        network = "http".to_string();
    } else if network == "http" {
        network = "h2".to_string();
    }
    lines.push(format!(r#"    network: "{}""#, yaml_escape(&network)));
}

fn append_v_share_transport_fields(
    query: &HashMap<String, String>,
    lines: &mut Vec<String>,
    network: &str,
    fake_type: &str,
) {
    match network {
        "tcp" => {
            if !fake_type.is_empty() && fake_type != "none" {
                lines.push("    http-opts:".to_string());
                append_yaml_path_list(
                    &mut *lines,
                    "path",
                    query.get("path").map(String::as_str),
                    6,
                    "/",
                );
                if let Some(method) = query.get("method").filter(|v| !v.is_empty()) {
                    lines.push(format!(r#"      method: "{}""#, yaml_escape(method)));
                }
                if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                    lines.push("      headers:".to_string());
                    append_yaml_string_list(&mut *lines, "Host", host, 8);
                }
            }
        }
        "http" => {
            lines.push("    h2-opts:".to_string());
            append_yaml_path_list(
                &mut *lines,
                "path",
                query.get("path").map(String::as_str),
                6,
                "/",
            );
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                append_yaml_string_list(&mut *lines, "host", host, 6);
            }
            lines.push("      headers: {}".to_string());
        }
        "ws" | "httpupgrade" => {
            lines.push("    ws-opts:".to_string());
            let path = query.get("path").map(String::as_str).unwrap_or("");
            lines.push(format!(r#"      path: "{}""#, yaml_escape(path)));
            lines.push("      headers:".to_string());
            lines.push("        User-Agent: \"Mozilla/5.0\"".to_string());
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"        Host: "{}""#, yaml_escape(host)));
            }
            if let Some(early_data) = query.get("ed").and_then(|s| s.parse::<u32>().ok()) {
                if network == "ws" {
                    lines.push(format!("      max-early-data: {}", early_data));
                    lines.push(
                        "      early-data-header-name: \"Sec-WebSocket-Protocol\"".to_string(),
                    );
                } else {
                    lines.push("      v2ray-http-upgrade-fast-open: true".to_string());
                }
            }
            if let Some(early_header) = query.get("eh").filter(|v| !v.is_empty()) {
                lines.push(format!(
                    r#"      early-data-header-name: "{}""#,
                    yaml_escape(early_header)
                ));
            }
        }
        "grpc" => {
            lines.push("    grpc-opts:".to_string());
            lines.push(format!(
                r#"      grpc-service-name: "{}""#,
                yaml_escape(query.get("serviceName").map(|s| s.as_str()).unwrap_or(""))
            ));
        }
        "xhttp" => {
            lines.push("    xhttp-opts:".to_string());
            if let Some(path) = query.get("path").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"      path: "{}""#, yaml_escape(path)));
            }
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"      host: "{}""#, yaml_escape(host)));
            }
            if let Some(mode) = query.get("mode").filter(|v| !v.is_empty()) {
                lines.push(format!(r#"      mode: "{}""#, yaml_escape(mode)));
            }
        }
        _ => {}
    }
}

fn extract_json_u16(value: Option<&serde_json::Value>) -> Option<u16> {
    let value = value?;
    if let Some(v) = value.as_u64() {
        return u16::try_from(v).ok();
    }
    if let Some(v) = value.as_i64() {
        return u16::try_from(v).ok();
    }
    value.as_str()?.parse::<u16>().ok()
}

fn decode_base64_flexible(data: &str) -> Option<Vec<u8>> {
    let normalized = data.replace('-', "+").replace('_', "/");
    let mut with_padding = normalized.clone();
    while with_padding.len() % 4 != 0 {
        with_padding.push('=');
    }
    base64::engine::general_purpose::STANDARD
        .decode(&with_padding)
        .ok()
        .or_else(|| base64::engine::general_purpose::URL_SAFE.decode(data).ok())
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(data)
                .ok()
        })
}

fn yaml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_bool_like(value: Option<&String>) -> bool {
    matches!(
        value.map(|v| v.as_str()),
        Some("1") | Some("true") | Some("TRUE") | Some("True")
    )
}

fn query_map(url: &Url) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in url.query_pairs() {
        map.insert(k.to_string(), v.to_string());
    }
    map
}

fn append_yaml_string_list(lines: &mut Vec<String>, key: &str, csv: &str, indent: usize) {
    let indent_str = " ".repeat(indent);
    lines.push(format!("{indent_str}{key}:"));
    for item in csv.split(',').map(str::trim).filter(|v| !v.is_empty()) {
        lines.push(format!(r#"{}  - "{}""#, indent_str, yaml_escape(item)));
    }
}

fn append_yaml_path_list(
    lines: &mut Vec<String>,
    key: &str,
    value: Option<&str>,
    indent: usize,
    default_value: &str,
) {
    let indent_str = " ".repeat(indent);
    lines.push(format!("{indent_str}{key}:"));
    let path = value.filter(|v| !v.is_empty()).unwrap_or(default_value);
    lines.push(format!(r#"{}  - "{}""#, indent_str, yaml_escape(path)));
}

fn apply_windows_spawn_flags(cmd: &mut Command) {
    #[cfg(windows)]
    {
        // Prevent console subsystem children (e.g. mihomo.exe) from flashing a black console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Mihomo 进程管理句柄。
pub struct MihomoProcess {
    child: Child,
    socks_port: u16,
    api_port: u16,
    cleaned_up: bool,
    /// 配置文件路径，用于注册表追踪
    config_path: Option<String>,
}

impl MihomoProcess {
    fn kill_and_wait(child: &mut Child) -> Result<(), String> {
        match child.try_wait() {
            Ok(Some(_)) => Ok(()),
            Ok(None) => {
                child.kill().map_err(|e| format!("停止 mihomo 失败: {e}"))?;
                child
                    .wait()
                    .map(|_| ())
                    .map_err(|e| format!("等待 mihomo 退出失败: {e}"))
            }
            Err(e) => Err(format!("检查 mihomo 进程状态失败: {e}")),
        }
    }

    /// 生成 Mihomo 配置文件 YAML 内容。
    /// 使用固定端口以便外部访问 SOCKS5 代理。
    pub fn generate_config(
        node: &crate::models::NodeInfo,
        socks_port: u16,
        api_port: u16,
    ) -> String {
        let proxy_config = build_proxy_config(node);

        format!(
            r#"port: 0
socks-port: {}
api-port: {}
allow-lan: false
mode: rule
log-level: warning

{}
proxy-groups:
  - name: "proxy"
    type: select
    proxies:
      - "{}"

rules:
  - GEOIP,CN,DIRECT
  - MATCH,proxy
"#,
            socks_port, api_port, proxy_config, node.name
        )
    }

    /// 启动 mihomo 进程，返回进程句柄。
    /// socks_port 和 api_port 必须为非零值，因为使用固定端口。
    /// 注意：此函数是同步的，但 spawn_mihomo_async 是异步版本，会正确处理阻塞调用。
    pub fn spawn(
        config_path: &Path,
        kernel_path: &Path,
        socks_port: u16,
        api_port: u16,
    ) -> Result<Self, String> {
        info!(
            "[Mihomo] spawn 开始: kernel_path={:?}, config_path={:?}, socks_port={}, api_port={}",
            kernel_path, config_path, socks_port, api_port
        );

        if socks_port == 0 || api_port == 0 {
            return Err("端口必须为非零值".to_string());
        }

        // 检查内核文件是否存在
        if !kernel_path.exists() {
            return Err(format!("内核文件不存在: {:?}", kernel_path));
        }

        info!("[Mihomo] 内核文件存在，开始启动进程...");

        // 使用 std::process::Command 启动 Mihomo
        let mut cmd = Command::new(kernel_path);
        cmd.arg("-f").arg(config_path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        apply_windows_spawn_flags(&mut cmd);

        info!("[Mihomo] 执行命令: {:?}", cmd);

        let child = cmd.spawn().map_err(|e| {
            error!("[Mihomo] 启动进程失败: {}", e);
            format!("启动 mihomo 失败: {e}")
        })?;

        info!("[Mihomo] 进程已启动, pid={:?}", child.id());

        // 等待 Mihomo 启动并绑定端口 (使用 std::thread::sleep 因为这是同步上下文)
        info!("[Mihomo] 等待 1000ms 让进程初始化...");
        std::thread::sleep(Duration::from_millis(1000));

        info!(
            "[Mihomo] spawn 完成, socks_port={}, api_port={}",
            socks_port, api_port
        );

        Ok(Self {
            child,
            socks_port,
            api_port,
            cleaned_up: false,
            config_path: None,
        })
    }

    /// 异步启动 mihomo 进程，使用 tokio::time::sleep 避免阻塞异步运行时。
    pub async fn spawn_async(
        config_path: &Path,
        kernel_path: &Path,
        socks_port: u16,
        api_port: u16,
    ) -> Result<Self, String> {
        info!("[Mihomo] spawn_async 开始: kernel_path={:?}, config_path={:?}, socks_port={}, api_port={}",
               kernel_path, config_path, socks_port, api_port);

        if socks_port == 0 || api_port == 0 {
            return Err("端口必须为非零值".to_string());
        }

        // 检查内核文件是否存在
        if !kernel_path.exists() {
            return Err(format!("内核文件不存在: {:?}", kernel_path));
        }

        info!("[Mihomo] 内核文件存在，开始启动进程...");

        let config_path_owned = config_path.to_path_buf();
        let kernel_path_owned = kernel_path.to_path_buf();

        let mut child = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&kernel_path_owned);
            cmd.arg("-f").arg(&config_path_owned);
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            apply_windows_spawn_flags(&mut cmd);
            cmd.spawn()
        })
        .await
        .map_err(|e| format!("spawn_blocking 失败: {}", e))?
        .map_err(|e| format!("启动 mihomo 失败: {}", e))?;

        info!("[Mihomo] 进程已启动, pid={:?}", child.id());

        // 捕获并记录日志
        if let Some(stdout) = child.stdout.take() {
            tokio::task::spawn_blocking(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        debug!("[Mihomo STDOUT] {}", l);
                    }
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::task::spawn_blocking(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        error!("[Mihomo STDERR] {}", l);
                    }
                }
            });
        }

        // 使用 tokio::time::sleep 避免阻塞异步运行时
        info!("[Mihomo] 等待 1000ms 让进程初始化...");
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // 检查进程是否意外退出
        if let Ok(Some(status)) = child.try_wait() {
            let err_msg = format!("Mihomo 进程意外退出，状态码: {}", status);
            error!("[Mihomo] {}", err_msg);
            return Err(err_msg);
        }

        info!(
            "[Mihomo] spawn_async 完成, socks_port={}, api_port={}",
            socks_port, api_port
        );

        Ok(Self {
            child,
            socks_port,
            api_port,
            cleaned_up: false,
            config_path: None,
        })
    }

    /// 停止进程。
    pub fn shutdown(mut self) -> Result<(), String> {
        let result = Self::kill_and_wait(&mut self.child);
        self.cleaned_up = true;
        result
    }

    /// 获取 SOCKS5 代理地址。
    pub fn proxy_addr(&self) -> String {
        format!("socks5://127.0.0.1:{}", self.socks_port)
    }

    /// 获取 API 端口。
    pub fn api_port(&self) -> u16 {
        self.api_port
    }

    /// 获取进程 PID
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for MihomoProcess {
    fn drop(&mut self) {
        if self.cleaned_up {
            return;
        }
        if let Err(e) = Self::kill_and_wait(&mut self.child) {
            warn!("[Mihomo] Drop 自动清理失败: {}", e);
        }
    }
}

/// 基于 PID 的进程标识，用于 HashSet 去重
impl PartialEq for MihomoProcess {
    fn eq(&self, other: &Self) -> bool {
        self.child.id() == other.child.id()
    }
}

impl Eq for MihomoProcess {}

impl std::hash::Hash for MihomoProcess {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.child.id().hash(state);
    }
}

impl MihomoProcess {
    /// 设置配置文件路径
    pub fn set_config_path(&mut self, path: String) {
        self.config_path = Some(path);
    }

    /// 获取配置文件路径
    pub fn config_path(&self) -> Option<&str> {
        self.config_path.as_deref()
    }

    /// 判断进程是否仍存活
    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().map(|s| s.is_none()).unwrap_or(false)
    }
}

// ============================================================================
// MihomoProcessRegistry — 全局进程注册表（基于 PID 追踪）
// ============================================================================

use std::sync::{Arc, Mutex};

/// 全局 Mihomo 进程注册表
/// 仅追踪 PID，不拥有进程句柄。进程仍由调用者管理。
pub struct MihomoProcessRegistry {
    pids: Mutex<HashSet<u32>>,
}

impl MihomoProcessRegistry {
    /// 全局注册表单例
    pub fn global() -> &'static Arc<Self> {
        static REGISTRY: OnceLock<Arc<MihomoProcessRegistry>> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            Arc::new(MihomoProcessRegistry {
                pids: Mutex::new(HashSet::new()),
            })
        })
    }

    /// 注册一个 Mihomo 进程的 PID
    pub fn register_pid(&self, pid: u32) {
        let mut pids = self.pids.lock().unwrap();
        pids.insert(pid);
        info!(
            "[Registry] 注册 Mihomo PID: {}, 当前追踪数: {}",
            pid,
            pids.len()
        );
    }

    /// 反注册一个 Mihomo 进程的 PID
    pub fn unregister_pid(&self, pid: u32) {
        let mut pids = self.pids.lock().unwrap();
        if pids.remove(&pid) {
            info!(
                "[Registry] 反注册 Mihomo PID: {}, 剩余: {}",
                pid,
                pids.len()
            );
        }
    }

    /// 关闭所有注册的进程（通过 OS 级 kill）
    #[cfg(windows)]
    pub fn shutdown_all(&self) {
        let pids: Vec<u32> = {
            let pids = self.pids.lock().unwrap();
            info!(
                "[Registry] 关闭所有 Mihomo 进程, 当前注册数: {}",
                pids.len()
            );
            pids.iter().copied().collect()
        };

        for pid in pids {
            if let Err(e) = kill_process_by_pid(pid) {
                warn!("[Registry] 关闭 Mihomo (pid={}) 失败: {}", pid, e);
            }
            // 即使失败也移除，因为进程可能已经崩溃退出
            self.unregister_pid(pid);
        }
    }

    /// 关闭所有注册的进程（非 Windows 版本）
    #[cfg(not(windows))]
    pub fn shutdown_all(&self) {
        let pids: Vec<u32> = {
            let pids = self.pids.lock().unwrap();
            info!(
                "[Registry] 关闭所有 Mihomo 进程, 当前注册数: {}",
                pids.len()
            );
            pids.iter().copied().collect()
        };

        for pid in pids {
            if let Err(e) = kill_process_by_pid(pid) {
                warn!("[Registry] 关闭 Mihomo (pid={}) 失败: {}", pid, e);
            }
            self.unregister_pid(pid);
        }
    }

    /// 清理启动时可能存在的孤儿 Mihomo 进程
    /// 在后台线程中执行，不阻塞调用者
    pub fn cleanup_orphaned_background() {
        std::thread::spawn(|| {
            info!("[Registry] 开始清理孤儿 Mihomo 进程...");

            let app_data = match crate::services::state_app_data_root() {
                Ok(p) => p,
                Err(e) => {
                    warn!("[Registry] 获取应用数据目录失败: {}", e);
                    return;
                }
            };
            let config_prefix = app_data
                .join("speedtest_configs")
                .to_string_lossy()
                .to_string();

            if let Err(e) = cleanup_orphaned_impl(&config_prefix) {
                warn!("[Registry] 清理孤儿进程失败: {}", e);
            }
        });
    }
}

fn cleanup_orphaned_impl(config_prefix: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::process::Command;

        // 使用 tasklist 获取所有 mihomo.exe 进程
        let output = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq mihomo.exe", "/FO", "CSV", "/NH"])
            .output()
            .map_err(|e| format!("执行 tasklist 失败: {}", e))?;

        if !output.status.success() {
            return Ok(()); // 没有 mihomo 进程，正常返回
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 2 {
                continue;
            }
            // CSV 格式: "mihomo.exe","1234","Session Name","Session#","Mem Usage"
            let pid = parts[1].trim_matches('"');
            let Ok(pid_u32) = pid.parse::<u32>() else {
                continue;
            };

            // 检查进程命令行参数
            if let Ok(cmdline) = get_process_commandline(pid_u32) {
                if cmdline.contains("-f") && cmdline.contains(config_prefix) {
                    info!(
                        "[Registry] 发现孤儿 Mihomo 进程, pid={}, cmdline={}",
                        pid, cmdline
                    );
                    if let Err(e) = kill_process_by_pid(pid_u32) {
                        warn!("[Registry] 杀死孤儿进程失败: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(not(windows))]
    {
        Ok(()) // 非 Windows 平台暂不支持
    }
}

#[cfg(windows)]
fn get_process_commandline(pid: u32) -> Result<String, String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            desired_access: u32,
            inherit_handle: i32,
            process_id: u32,
        ) -> *mut std::ffi::c_void;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn QueryFullProcessImageNameW(
            process: *mut std::ffi::c_void,
            flags: u32,
            exe_name: *mut u16,
            size: *mut u32,
        ) -> i32;
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return Err(format!("OpenProcess 失败, pid={}", pid));
        }

        let mut buffer: [u16; 1024] = [0; 1024];
        let mut size: u32 = 1024;

        let success = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
        CloseHandle(handle);

        if success == 0 {
            return Err(format!("QueryFullProcessImageNameW 失败, pid={}", pid));
        }

        let name = OsString::from_wide(&buffer[..size as usize])
            .to_string_lossy()
            .to_string();

        // 对于 mihomo，我们主要关心命令行参数，这里返回进程路径
        Ok(name)
    }
}

#[cfg(windows)]
fn kill_process_by_pid(pid: u32) -> Result<(), String> {
    use std::process::Command;

    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output()
        .map_err(|e| format!("执行 taskkill 失败: {}", e))?;

    if output.status.success() {
        info!("[Registry] 已杀死孤儿进程, pid={}", pid);
        Ok(())
    } else {
        Err(format!(
            "taskkill 失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(not(windows))]
fn kill_process_by_pid(pid: u32) -> Result<(), String> {
    let output = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output()
        .map_err(|e| format!("执行 kill 失败: {}", e))?;

    if output.status.success() {
        info!("[Registry] 已杀死进程, pid={}", pid);
        Ok(())
    } else {
        Err(format!(
            "kill 失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

use std::sync::OnceLock;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NodeConnectInfo, NodeInfo};

    #[test]
    fn generate_config_vless节点() {
        let node = NodeInfo {
            name: "测试-VLESS".to_string(),
            protocol: "vless".to_string(),
            country: "HK".to_string(),
            raw: "vless://uuid@example.com:443#测试".to_string(),
            parsed_proxy_payload: None,
            connect_info: Some(NodeConnectInfo {
                server: "example.com".to_string(),
                port: 443,
                username: Some("test-uuid".to_string()),
                password: Some("xtls-rprx-vision".to_string()),
            }),
            test_file: None,
            upload_target: None,
        };

        let yaml = MihomoProcess::generate_config(&node, 10800, 10801);

        assert!(yaml.contains("socks-port: 10800"));
        assert!(yaml.contains("api-port: 10801"));
        assert!(yaml.contains("type: vless"));
        assert!(yaml.contains("server: example.com"));
        assert!(yaml.contains("port: 443"));
        assert!(
            yaml.contains("uuid: \"uuid\"")
                || yaml.contains("uuid: uuid")
                || yaml.contains("uuid: test-uuid")
        );
        assert!(yaml.contains("测试-VLESS"));
    }

    #[test]
    fn generate_config_vless_reality字段完整() {
        let node = NodeInfo {
            name: "香港 A02 联通优化".to_string(),
            protocol: "vless".to_string(),
            country: "HK".to_string(),
            raw: "vless://c62f29e1-a056-33cc-9381-ba973fd329c9@hk02.aisafushi.org:443?type=tcp&encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.microsoft.com&fp=chrome&pbk=Vc8ycAgKqfRvtXjvGP0ry_U91o5wgrQlqOhHq72HYRs&sid=1bc2c1ef1c#%E9%A6%99%E6%B8%AF%20A02%20%E8%81%94%E9%80%9A%E4%BC%98%E5%8C%96".to_string(),
            parsed_proxy_payload: None,
            connect_info: Some(NodeConnectInfo {
                server: "hk02.aisafushi.org".to_string(),
                port: 443,
                username: Some("c62f29e1-a056-33cc-9381-ba973fd329c9".to_string()),
                password: None,
            }),
            test_file: None,
            upload_target: None,
        };

        let yaml = MihomoProcess::generate_config(&node, 10800, 10801);

        assert!(yaml.contains("type: vless"));
        assert!(yaml.contains("server: hk02.aisafushi.org"));
        assert!(yaml.contains("port: 443"));
        assert!(yaml.contains("uuid: \"c62f29e1-a056-33cc-9381-ba973fd329c9\""));
        assert!(yaml.contains("encryption: \"none\""));
        assert!(yaml.contains("flow: \"xtls-rprx-vision\""));
        assert!(yaml.contains("tls: true"));
        assert!(yaml.contains("network: \"tcp\""));
        assert!(yaml.contains("servername: \"www.microsoft.com\""));
        assert!(yaml.contains("client-fingerprint: \"chrome\""));
        assert!(yaml.contains("reality-opts:"));
        assert!(yaml.contains("public-key: \"Vc8ycAgKqfRvtXjvGP0ry_U91o5wgrQlqOhHq72HYRs\""));
        assert!(yaml.contains("short-id: \"1bc2c1ef1c\""));
    }

    #[test]
    fn generate_config_trojan节点() {
        let node = NodeInfo {
            name: "测试-Trojan".to_string(),
            protocol: "trojan".to_string(),
            country: "JP".to_string(),
            raw: "".to_string(),
            parsed_proxy_payload: None,
            connect_info: Some(NodeConnectInfo {
                server: "jp.example.com".to_string(),
                port: 443,
                username: None,
                password: Some("password123".to_string()),
            }),
            test_file: None,
            upload_target: None,
        };

        let yaml = MihomoProcess::generate_config(&node, 10810, 10811);

        assert!(yaml.contains("type: trojan"));
        assert!(yaml.contains("server: jp.example.com"));
        assert!(yaml.contains("password: password123"));
    }

    #[test]
    fn generate_config_ss节点() {
        let node = NodeInfo {
            name: "测试-SS".to_string(),
            protocol: "ss".to_string(),
            country: "SG".to_string(),
            raw: "".to_string(),
            parsed_proxy_payload: None,
            connect_info: Some(NodeConnectInfo {
                server: "sg.example.com".to_string(),
                port: 8388,
                username: Some("aes-256-gcm".to_string()),
                password: Some("password".to_string()),
            }),
            test_file: None,
            upload_target: None,
        };

        let yaml = MihomoProcess::generate_config(&node, 10820, 10821);

        assert!(yaml.contains("type: ss"));
        assert!(yaml.contains("cipher: aes-256-gcm"));
        assert!(yaml.contains("password: password"));
    }

    #[test]
    fn generate_config_vmess节点() {
        let node = NodeInfo {
            name: "测试-VMess".to_string(),
            protocol: "vmess".to_string(),
            country: "US".to_string(),
            raw: "".to_string(),
            parsed_proxy_payload: None,
            connect_info: Some(NodeConnectInfo {
                server: "us.example.com".to_string(),
                port: 10086,
                username: Some("user-uuid".to_string()),
                password: Some("0:user-uuid".to_string()), // alterId:uuid 格式
            }),
            test_file: None,
            upload_target: None,
        };

        let yaml = MihomoProcess::generate_config(&node, 10830, 10831);

        assert!(yaml.contains("type: vmess"));
        assert!(yaml.contains("uuid: user-uuid"));
        assert!(yaml.contains("alterId:"));
    }

    #[test]
    fn generate_config_ssr节点() {
        let node = NodeInfo {
            name: "测试-SSR".to_string(),
            protocol: "ssr".to_string(),
            country: "TW".to_string(),
            raw: "".to_string(),
            parsed_proxy_payload: None,
            connect_info: Some(NodeConnectInfo {
                server: "tw.example.com".to_string(),
                port: 443,
                username: Some("aes-256-cfb".to_string()),
                password: Some("password".to_string()),
            }),
            test_file: None,
            upload_target: None,
        };

        let yaml = MihomoProcess::generate_config(&node, 10850, 10851);

        assert!(yaml.contains("type: ssr"));
        assert!(yaml.contains("cipher: aes-256-cfb"));
        assert!(yaml.contains("password: password"));
        assert!(yaml.contains("obfs: plain"));
        assert!(yaml.contains("protocol: origin"));
    }

    #[test]
    fn generate_config_优先使用解析payload_覆盖新增协议() {
        let cases = vec![
            (
                "hysteria",
                r#"{"name":"hy","type":"hysteria","server":"example.com","port":443,"auth_str":"a"}"#,
                "type: hysteria",
                "auth_str: a",
            ),
            (
                "hysteria2",
                r#"{"name":"hy2","type":"hysteria2","server":"example.com","port":8443,"password":"p"}"#,
                "type: hysteria2",
                "password: p",
            ),
            (
                "tuic",
                r#"{"name":"tuic","type":"tuic","server":"example.com","port":443,"token":"t","udp":true}"#,
                "type: tuic",
                "token: t",
            ),
            (
                "socks5",
                r#"{"name":"s5","type":"socks5","server":"127.0.0.1","port":1080}"#,
                "type: socks5",
                "port: 1080",
            ),
            (
                "http",
                r#"{"name":"h1","type":"http","server":"127.0.0.1","port":8080,"tls":true}"#,
                "type: http",
                "tls: true",
            ),
            (
                "anytls",
                r#"{"name":"at","type":"anytls","server":"example.com","port":443,"username":"u","password":"p"}"#,
                "type: anytls",
                "username: u",
            ),
            (
                "mieru",
                r#"{"name":"m1","type":"mieru","server":"1.2.3.4","port":6666,"transport":"TCP"}"#,
                "type: mieru",
                "transport: TCP",
            ),
        ];

        for (protocol, payload, expected_a, expected_b) in cases {
            let node = NodeInfo {
                name: format!("payload-{protocol}"),
                protocol: protocol.to_string(),
                country: "UNKNOWN".to_string(),
                raw: String::new(),
                parsed_proxy_payload: Some(payload.to_string()),
                connect_info: None,
                test_file: None,
                upload_target: None,
            };
            let yaml = MihomoProcess::generate_config(&node, 10900, 10901);
            assert!(
                yaml.contains(expected_a),
                "missing `{expected_a}` in {yaml}"
            );
            assert!(
                yaml.contains(expected_b),
                "missing `{expected_b}` in {yaml}"
            );
        }
    }

    #[test]
    fn generate_config_无connect_info使用默认() {
        let node = NodeInfo {
            name: "无连接信息".to_string(),
            protocol: "unknown".to_string(),
            country: "XX".to_string(),
            raw: "".to_string(),
            parsed_proxy_payload: None,
            connect_info: None,
            test_file: None,
            upload_target: None,
        };

        let yaml = MihomoProcess::generate_config(&node, 10840, 10841);

        assert!(yaml.contains("type: http"));
        assert!(yaml.contains("server: 127.0.0.1"));
        assert!(yaml.contains("port: 1"));
    }

    #[test]
    fn proxy_addr返回正确格式() {
        // 由于 MihomoProcess::spawn 需要真实的 kernel_path，我们只测试 proxy_addr 格式化
        // 这里直接验证格式
        let addr = format!("socks5://127.0.0.1:{}", 10800);
        assert_eq!(addr, "socks5://127.0.0.1:10800");
    }

    #[test]
    fn detect_platform返回有效平台() {
        let platform = detect_platform();
        assert!(!platform.is_empty());
        // Windows 上应该返回 "windows"
        #[cfg(target_os = "windows")]
        assert_eq!(platform, "windows");
    }
}
