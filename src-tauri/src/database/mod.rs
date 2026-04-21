//! 数据库模块：SQLite 数据库初始化和连接管理。
//!
//! 数据库文件位于应用数据目录下的 `speedtest.db`。

pub mod batch;
pub mod result;

use rusqlite::{Connection, Result};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{error, info};

/// 全局数据库连接
pub static DB: std::sync::OnceLock<Mutex<Connection>> = std::sync::OnceLock::new();

fn database_path() -> Result<PathBuf, String> {
    let dir = crate::services::state::app_data_root()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
    Ok(dir.join("speedtest.db"))
}

/// 初始化数据库，创建表结构
pub fn init_database() -> Result<Connection, String> {
    let db_path = database_path()?;
    info!("[数据库] 初始化数据库: {:?}", db_path);

    let conn = Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;

    // 启用外键约束
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("启用外键失败: {}", e))?;

    // 创建批次表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS speedtest_batches (
            batch_id INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at INTEGER NOT NULL,
            subscription_text TEXT NOT NULL,
            config_json TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_batches_created_at ON speedtest_batches(created_at);
        "#,
    )
    .map_err(|e| format!("创建批次表失败: {}", e))?;

    // 创建结果表
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS speedtest_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id INTEGER NOT NULL,
            node_name TEXT NOT NULL,
            protocol TEXT NOT NULL,
            country_code TEXT NOT NULL,
            tcp_ping_ms INTEGER NOT NULL,
            site_ping_ms INTEGER NOT NULL,
            packet_loss_rate REAL NOT NULL,
            avg_download_mbps REAL NOT NULL,
            max_download_mbps REAL NOT NULL,
            avg_upload_mbps REAL,
            max_upload_mbps REAL,
            nat_type TEXT NOT NULL,
            ingress_ip TEXT NOT NULL,
            ingress_country TEXT NOT NULL,
            ingress_country_name TEXT NOT NULL DEFAULT '',
            ingress_isp TEXT NOT NULL DEFAULT '',
            egress_ip TEXT NOT NULL,
            egress_country TEXT NOT NULL,
            egress_country_name TEXT NOT NULL DEFAULT '',
            egress_isp TEXT NOT NULL DEFAULT '',
            finished_at INTEGER NOT NULL,
            FOREIGN KEY (batch_id) REFERENCES speedtest_batches(batch_id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_results_batch_id ON speedtest_results(batch_id);
        CREATE INDEX IF NOT EXISTS idx_results_country_code ON speedtest_results(country_code);
        CREATE INDEX IF NOT EXISTS idx_results_finished_at ON speedtest_results(finished_at);
        "#,
    )
    .map_err(|e| format!("创建结果表失败: {}", e))?;

    // 迁移旧表结构：添加 ingress_country_name, ingress_isp, egress_country_name, egress_isp 列
    let migrator = || -> Result<(), String> {
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(speedtest_results)")
            .map_err(|e| format!("查询表结构失败: {}", e))?
            .query_map([], |row| Ok(row.get::<_, String>(1).unwrap_or_default()))
            .map_err(|e| format!("查询表结构失败: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        if !cols.contains(&"ingress_country_name".to_string()) {
            conn.execute(
                "ALTER TABLE speedtest_results ADD COLUMN ingress_country_name TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| format!("迁移添加 ingress_country_name 失败: {}", e))?;
        }
        if !cols.contains(&"ingress_isp".to_string()) {
            conn.execute(
                "ALTER TABLE speedtest_results ADD COLUMN ingress_isp TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| format!("迁移添加 ingress_isp 失败: {}", e))?;
        }
        if !cols.contains(&"egress_country_name".to_string()) {
            conn.execute(
                "ALTER TABLE speedtest_results ADD COLUMN egress_country_name TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| format!("迁移添加 egress_country_name 失败: {}", e))?;
        }
        if !cols.contains(&"egress_isp".to_string()) {
            conn.execute(
                "ALTER TABLE speedtest_results ADD COLUMN egress_isp TEXT NOT NULL DEFAULT ''",
                [],
            )
            .map_err(|e| format!("迁移添加 egress_isp 失败: {}", e))?;
        }
        Ok(())
    };
    migrator()?;

    info!("[数据库] 初始化完成");
    Ok(conn)
}

/// 获取数据库连接（线程安全）
pub fn get_db() -> Result<std::sync::MutexGuard<'static, Connection>, String> {
    let db = DB.get_or_init(|| match init_database() {
        Ok(conn) => Mutex::new(conn),
        Err(e) => {
            error!("[数据库] 初始化失败: {}", e);
            panic!("数据库初始化失败: {}", e);
        }
    });
    db.lock().map_err(|e| format!("获取数据库锁失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_init() {
        // 使用临时目录测试
        let temp_dir =
            std::env::temp_dir().join(format!("capyspeedtest-db-test-{}", current_timestamp()));
        std::env::set_var("CAPYSPEEDTEST_DATA_DIR", &temp_dir);

        let result = init_database();
        assert!(result.is_ok(), "数据库初始化应该成功: {:?}", result.err());

        std::env::remove_var("CAPYSPEEDTEST_DATA_DIR");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    fn current_timestamp() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }
}
