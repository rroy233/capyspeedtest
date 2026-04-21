//! 批次表操作模块

use crate::database::get_db;
use crate::models::{BatchSummary, SpeedTestResult, SpeedTestTaskConfig};
use chrono::Timelike;
use rusqlite::params;
use tracing::info;

/// 保存测速批次，返回新创建的 batch_id
pub fn save_batch(
    created_at: i64,
    subscription_text: &str,
    config: &SpeedTestTaskConfig,
    results: &[SpeedTestResult],
) -> Result<i64, String> {
    let config_json =
        serde_json::to_string(config).map_err(|e| format!("序列化配置失败: {}", e))?;

    let conn = get_db()?;

    // 插入批次记录
    conn.execute(
        "INSERT INTO speedtest_batches (created_at, subscription_text, config_json) VALUES (?1, ?2, ?3)",
        params![created_at, subscription_text, config_json],
    )
    .map_err(|e| format!("插入批次失败: {}", e))?;

    let batch_id = conn.last_insert_rowid();

    // 批量插入结果
    for result in results {
        let country_code = result.node.country.to_uppercase();
        conn.execute(
            r#"
            INSERT INTO speedtest_results (
                batch_id, node_name, protocol, country_code,
                tcp_ping_ms, site_ping_ms, packet_loss_rate,
                avg_download_mbps, max_download_mbps, avg_upload_mbps, max_upload_mbps,
                nat_type,
                ingress_ip, ingress_country, ingress_country_name, ingress_isp,
                egress_ip, egress_country, egress_country_name, egress_isp,
                finished_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
            "#,
            params![
                batch_id,
                result.node.name,
                result.node.protocol,
                country_code,
                result.tcp_ping_ms as i64,
                result.site_ping_ms as i64,
                result.packet_loss_rate as f64,
                result.avg_download_mbps as f64,
                result.max_download_mbps as f64,
                result.avg_upload_mbps.map(|v| v as f64),
                result.max_upload_mbps.map(|v| v as f64),
                result.nat_type,
                result.ingress_geoip.ip,
                result.ingress_geoip.country_code,
                result.ingress_geoip.country_name,
                result.ingress_geoip.isp,
                result.egress_geoip.ip,
                result.egress_geoip.country_code,
                result.egress_geoip.country_name,
                result.egress_geoip.isp,
                parse_finished_at(&result.finished_at),
            ],
        )
        .map_err(|e| format!("插入结果失败: {}", e))?;
    }

    info!(
        "[数据库] 保存批次 batch_id={}, 节点数={}",
        batch_id,
        results.len()
    );
    Ok(batch_id)
}

/// 解析 finished_at 时间戳（秒）
fn parse_finished_at(finished_at: &str) -> i64 {
    finished_at.parse::<i64>().unwrap_or_else(|_| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    })
}

/// 获取批次列表（分页）
pub fn get_batches(
    from_timestamp: Option<i64>,
    to_timestamp: Option<i64>,
    limit: usize,
    offset: usize,
) -> Result<Vec<BatchSummary>, String> {
    let conn = get_db()?;

    let mut sql = String::from(
        r#"
        SELECT b.batch_id, b.created_at, b.config_json,
               (SELECT COUNT(*) FROM speedtest_results WHERE batch_id = b.batch_id) as node_count
        FROM speedtest_batches b
        WHERE 1=1
        "#,
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(from) = from_timestamp {
        sql.push_str(" AND b.created_at >= ?");
        params_vec.push(Box::new(from));
    }
    if let Some(to) = to_timestamp {
        sql.push_str(" AND b.created_at <= ?");
        params_vec.push(Box::new(to));
    }

    sql.push_str(" ORDER BY b.created_at DESC");
    sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("准备查询失败: {}", e))?;

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let batches = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok(BatchSummary {
                batch_id: row.get(0)?,
                created_at: row.get(1)?,
                config_json: row.get(2)?,
                node_count: row.get::<_, i64>(3)? as usize,
            })
        })
        .map_err(|e| format!("查询批次失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(batches)
}

/// 获取单个批次的详细信息
pub fn get_batch_results(batch_id: i64) -> Result<Vec<SpeedTestResult>, String> {
    let conn = get_db()?;

    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                node_name, protocol, country_code,
                tcp_ping_ms, site_ping_ms, packet_loss_rate,
                avg_download_mbps, max_download_mbps, avg_upload_mbps, max_upload_mbps,
                nat_type,
                ingress_ip, ingress_country, ingress_country_name, ingress_isp,
                egress_ip, egress_country, egress_country_name, egress_isp,
                finished_at
            FROM speedtest_results
            WHERE batch_id = ?1
            ORDER BY avg_download_mbps DESC
            "#,
        )
        .map_err(|e| format!("准备查询失败: {}", e))?;

    let results = stmt
        .query_map([batch_id], |row| {
            let country_code: String = row.get(2)?;
            Ok(SpeedTestResult {
                node: crate::models::NodeInfo {
                    name: row.get(0)?,
                    protocol: row.get(1)?,
                    country: country_code.clone(),
                    raw: String::new(),
                    parsed_proxy_payload: None,
                    connect_info: None,
                    test_file: None,
                    upload_target: None,
                },
                tcp_ping_ms: row.get::<_, i64>(3)? as u32,
                site_ping_ms: row.get::<_, i64>(4)? as u32,
                packet_loss_rate: row.get::<_, f64>(5)? as f32,
                avg_download_mbps: row.get::<_, f64>(6)? as f32,
                max_download_mbps: row.get::<_, f64>(7)? as f32,
                avg_upload_mbps: row.get::<_, Option<f64>>(8)?.map(|v| v as f32),
                max_upload_mbps: row.get::<_, Option<f64>>(9)?.map(|v| v as f32),
                ingress_geoip: crate::models::GeoIpInfo {
                    ip: row.get(11)?,
                    country_code: row.get::<_, String>(12)?,
                    country_name: row.get::<_, String>(13)?,
                    isp: row.get::<_, String>(14)?,
                },
                egress_geoip: crate::models::GeoIpInfo {
                    ip: row.get(15)?,
                    country_code: row.get::<_, String>(16)?,
                    country_name: row.get::<_, String>(17)?,
                    isp: row.get::<_, String>(18)?,
                },
                nat_type: row.get(10)?,
                finished_at: row.get::<_, i64>(19)?.to_string(),
            })
        })
        .map_err(|e| format!("查询结果失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// 删除指定批次
pub fn delete_batches(batch_ids: &[i64]) -> Result<usize, String> {
    if batch_ids.is_empty() {
        return Ok(0);
    }

    let conn = get_db()?;
    let placeholders: Vec<&str> = batch_ids.iter().map(|_| "?").collect();
    let sql = format!(
        "DELETE FROM speedtest_batches WHERE batch_id IN ({})",
        placeholders.join(",")
    );

    let params: Vec<&dyn rusqlite::ToSql> = batch_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();

    let deleted = conn
        .execute(&sql, rusqlite::params_from_iter(params))
        .map_err(|e| format!("删除批次失败: {}", e))?;

    info!("[数据库] 删除 {} 个批次", deleted);
    Ok(deleted)
}

/// 删除指定时间之前的所有批次
pub fn delete_batches_older_than(timestamp: i64) -> Result<usize, String> {
    let conn = get_db()?;

    let deleted = conn
        .execute(
            "DELETE FROM speedtest_batches WHERE created_at < ?",
            [timestamp],
        )
        .map_err(|e| format!("删除旧批次失败: {}", e))?;

    info!("[数据库] 删除 {} 个旧批次 (早于 {})", deleted, timestamp);
    Ok(deleted)
}

/// 清空所有历史记录
pub fn clear_all_batches() -> Result<usize, String> {
    let conn = get_db()?;

    // 外键级联删除会自动清理 results 表
    let deleted = conn
        .execute("DELETE FROM speedtest_batches", [])
        .map_err(|e| format!("清空历史失败: {}", e))?;

    info!("[数据库] 清空所有历史记录，删除了 {} 个批次", deleted);
    Ok(deleted)
}

/// 获取散点图数据
pub fn get_scatter_data(
    from_timestamp: Option<i64>,
    to_timestamp: Option<i64>,
) -> Result<Vec<crate::models::ScatterPoint>, String> {
    let conn = get_db()?;

    let mut sql = String::from(
        r#"
        SELECT
            batch_id, finished_at, country_code,
            avg_download_mbps, avg_upload_mbps, node_name
        FROM speedtest_results
        WHERE 1=1
        "#,
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(from) = from_timestamp {
        sql.push_str(" AND finished_at >= ?");
        params_vec.push(Box::new(from));
    }
    if let Some(to) = to_timestamp {
        sql.push_str(" AND finished_at <= ?");
        params_vec.push(Box::new(to));
    }

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("准备查询失败: {}", e))?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let points = stmt
        .query_map(params_refs.as_slice(), |row| {
            let finished_at: i64 = row.get(1)?;
            // 按本地时区提取 hour (0-23.99)
            let hour = chrono::DateTime::from_timestamp(finished_at, 0)
                .map(|utc| {
                    let local = utc.with_timezone(&chrono::Local);
                    local.hour() as f64
                        + local.minute() as f64 / 60.0
                        + local.second() as f64 / 3600.0
                })
                .unwrap_or(0.0);
            Ok(crate::models::ScatterPoint {
                batch_id: row.get(0)?,
                finished_at,
                hour,
                country_code: row.get(2)?,
                avg_download_mbps: row.get::<_, f64>(3)?,
                avg_upload_mbps: row.get::<_, Option<f64>>(4)?,
                node_name: row.get(5)?,
            })
        })
        .map_err(|e| format!("查询散点数据失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(points)
}

/// 获取所有出现过的地区代码
pub fn get_all_countries() -> Result<Vec<String>, String> {
    let conn = get_db()?;

    let mut stmt = conn
        .prepare("SELECT DISTINCT country_code FROM speedtest_results ORDER BY country_code")
        .map_err(|e| format!("准备查询失败: {}", e))?;

    let countries = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("查询地区失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(countries)
}
