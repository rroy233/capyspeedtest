//! 文件系统工具函数：目录统计、zip打包。

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

/// 递归统计目录的总大小和文件数量。
pub fn collect_dir_stats(dir: &Path) -> Result<(u64, u64), String> {
    if !dir.exists() {
        return Ok((0, 0));
    }
    let mut total_bytes: u64 = 0;
    let mut file_count: u64 = 0;
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败({dir:?}): {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败({dir:?}): {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            let (child_bytes, child_files) = collect_dir_stats(&path)?;
            total_bytes = total_bytes.saturating_add(child_bytes);
            file_count = file_count.saturating_add(child_files);
        } else {
            let size = entry
                .metadata()
                .map_err(|e| format!("读取文件元数据失败({path:?}): {e}"))?
                .len();
            total_bytes = total_bytes.saturating_add(size);
            file_count = file_count.saturating_add(1);
        }
    }
    Ok((total_bytes, file_count))
}

/// 递归将目录添加到 ZipWriter。
pub fn add_dir_to_zip(
    writer: &mut zip::ZipWriter<fs::File>,
    root_dir: &Path,
    current_dir: &Path,
    excluded_prefix: &Path,
) -> Result<(), String> {
    let entries =
        fs::read_dir(current_dir).map_err(|e| format!("读取目录失败({current_dir:?}): {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败({current_dir:?}): {e}"))?;
        let path = entry.path();
        if path.starts_with(excluded_prefix) {
            continue;
        }

        let relative = path
            .strip_prefix(root_dir)
            .map_err(|e| format!("构建压缩相对路径失败({path:?}): {e}"))?;
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        if path.is_dir() {
            if !relative_text.is_empty() {
                writer
                    .add_directory(format!("{relative_text}/"), options)
                    .map_err(|e| format!("压缩目录失败({path:?}): {e}"))?;
            }
            add_dir_to_zip(writer, root_dir, &path, excluded_prefix)?;
        } else {
            writer
                .start_file(relative_text, options)
                .map_err(|e| format!("创建压缩文件项失败({path:?}): {e}"))?;
            let mut f =
                fs::File::open(&path).map_err(|e| format!("打开文件失败({path:?}): {e}"))?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .map_err(|e| format!("读取文件失败({path:?}): {e}"))?;
            writer
                .write_all(&buf)
                .map_err(|e| format!("写入压缩文件失败({path:?}): {e}"))?;
        }
    }

    Ok(())
}
