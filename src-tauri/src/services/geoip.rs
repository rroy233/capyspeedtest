//! GeoIP 模块：负责 MMDB 数据库读取、真实 IP→地理位置查询，以及节点名→国家代码推断。
//!
//! 使用 MaxMind 的 .mmdb 格式数据库进行离线 IP 地理位置查询。

use crate::models::GeoIpInfo;
use maxminddb::Reader;
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::{debug, error, info, warn};

// ============================================================================
// 节点名 → 国家/地区代码 推断
// ============================================================================

/// 单个国家/地区的匹配规则条目。
struct CountryEntry {
    /// ISO alpha-2 大写代码
    code: &'static str,
    /// Unicode emoji 国旗（如 "🇭🇰"）
    emoji: Option<&'static str>,
    /// CJK 关键词（中文繁体/简体等），原始字符串 contains 匹配
    cjk_keywords: &'static [&'static str],
    /// 长英文词（4+ 字符），大写文本 contains 匹配，无误判
    long_tokens: &'static [&'static str],
    /// 短代码（2-3 字符），必须词边界匹配，避免 "TWITCH"→TW 等误判
    short_tokens: &'static [&'static str],
    /// 正则匹配模式（低优先级），匹配"中转地+节点地区"组合
    regex_patterns: &'static [&'static str],
}

/// 按优先级排列的国家/地区规则表。
/// 高冲突风险的条目（如 IN、ES、IT）靠后，长词/CJK 在前阶段截获。
static COUNTRY_TABLE: &[CountryEntry] = &[
    CountryEntry {
        code: "HK",
        emoji: Some("\u{1F1ED}\u{1F1F0}"),
        cjk_keywords: &["香港"],
        long_tokens: &["HONGKONG", "HONG KONG"],
        short_tokens: &["HK"],
        regex_patterns: &[
            "北京.*港",
            "上海.*港",
            "广州.*港",
            "深圳.*港",
            "珠海.*港",
            "福州.*港",
            "厦门.*港",
            "杭州.*港",
            "南京.*港",
            "成都.*港",
            "武汉.*港",
            "西安.*港",
            "重庆.*港",
            "天津.*港",
            "大连.*港",
            "青岛.*港",
            "宁波.*港",
            "郑州.*港",
            "长沙.*港",
            "沈阳.*港",
            "济南.*港",
            "昆明.*港",
            "贵阳.*港",
            "南宁.*港",
            "哈尔滨.*港",
            "长春.*港",
            "石家庄.*港",
            "太原.*港",
            "兰州.*港",
            "乌鲁木齐.*港",
            "中港",
            "深港",
            "沪港",
            "京港",
            "广港",
            "珠港",
            "福港",
            "厦港",
        ],
    },
    CountryEntry {
        code: "TW",
        emoji: Some("\u{1F1F9}\u{1F1FC}"),
        cjk_keywords: &["台湾", "台灣"],
        long_tokens: &["TAIWAN"],
        short_tokens: &["TW"],
        regex_patterns: &[
            "北京.*台",
            "上海.*台",
            "广州.*台",
            "深圳.*台",
            "珠海.*台",
            "福州.*台",
            "厦门.*台",
            "杭州.*台",
            "南京.*台",
            "成都.*台",
            "武汉.*台",
            "西安.*台",
            "重庆.*台",
            "天津.*台",
            "大连.*台",
            "青岛.*台",
            "宁波.*台",
            "郑州.*台",
            "长沙.*台",
            "沈阳.*台",
            "济南.*台",
            "昆明.*台",
            "贵阳.*台",
            "南宁.*台",
            "哈尔滨.*台",
            "长春.*台",
            "石家庄.*台",
            "太原.*台",
            "兰州.*台",
            "乌鲁木齐.*台",
            "中台",
            "深台",
            "沪台",
            "京台",
            "广台",
        ],
    },
    CountryEntry {
        code: "JP",
        emoji: Some("\u{1F1EF}\u{1F1F5}"),
        cjk_keywords: &["日本"],
        long_tokens: &["JAPAN", "NIPPON"],
        short_tokens: &["JP"],
        regex_patterns: &[
            "北京.*日",
            "上海.*日",
            "广州.*日",
            "深圳.*日",
            "珠海.*日",
            "福州.*日",
            "厦门.*日",
            "杭州.*日",
            "南京.*日",
            "成都.*日",
            "武汉.*日",
            "西安.*日",
            "重庆.*日",
            "天津.*日",
            "大连.*日",
            "青岛.*日",
            "宁波.*日",
            "郑州.*日",
            "长沙.*日",
            "沈阳.*日",
            "济南.*日",
            "昆明.*日",
            "贵阳.*日",
            "南宁.*日",
            "哈尔滨.*日",
            "长春.*日",
            "石家庄.*日",
            "太原.*日",
            "兰州.*日",
            "乌鲁木齐.*日",
            "中日",
            "深日",
            "沪日",
            "京日",
            "广日",
        ],
    },
    CountryEntry {
        code: "KR",
        emoji: Some("\u{1F1F0}\u{1F1F7}"),
        cjk_keywords: &["韩国", "韓國"],
        long_tokens: &["KOREA", "SOUTH KOREA", "SOUTHKOREA"],
        short_tokens: &["KR"],
        regex_patterns: &[
            "北京.*韩",
            "上海.*韩",
            "广州.*韩",
            "深圳.*韩",
            "珠海.*韩",
            "福州.*韩",
            "厦门.*韩",
            "杭州.*韩",
            "南京.*韩",
            "成都.*韩",
            "武汉.*韩",
            "西安.*韩",
            "重庆.*韩",
            "天津.*韩",
            "大连.*韩",
            "青岛.*韩",
            "宁波.*韩",
            "郑州.*韩",
            "长沙.*韩",
            "沈阳.*韩",
            "济南.*韩",
            "昆明.*韩",
            "贵阳.*韩",
            "南宁.*韩",
            "哈尔滨.*韩",
            "长春.*韩",
            "石家庄.*韩",
            "太原.*韩",
            "兰州.*韩",
            "乌鲁木齐.*韩",
            "中韩",
            "深韩",
            "沪韩",
            "京韩",
            "广韩",
        ],
    },
    CountryEntry {
        code: "SG",
        emoji: Some("\u{1F1F8}\u{1F1EC}"),
        cjk_keywords: &["新加坡"],
        long_tokens: &["SINGAPORE"],
        short_tokens: &["SG"],
        regex_patterns: &[
            "北京.*新",
            "上海.*新",
            "广州.*新",
            "深圳.*新",
            "珠海.*新",
            "福州.*新",
            "厦门.*新",
            "杭州.*新",
            "南京.*新",
            "成都.*新",
            "武汉.*新",
            "西安.*新",
            "重庆.*新",
            "天津.*新",
            "大连.*新",
            "青岛.*新",
            "宁波.*新",
            "郑州.*新",
            "长沙.*新",
            "沈阳.*新",
            "济南.*新",
            "昆明.*新",
            "贵阳.*新",
            "南宁.*新",
            "哈尔滨.*新",
            "长春.*新",
            "石家庄.*新",
            "太原.*新",
            "兰州.*新",
            "乌鲁木齐.*新",
            "中新",
            "深新",
            "沪新",
            "京新",
            "广新",
        ],
    },
    CountryEntry {
        code: "MY",
        emoji: Some("\u{1F1F2}\u{1F1FE}"),
        cjk_keywords: &["马来西亚", "馬來西亞"],
        long_tokens: &["MALAYSIA"],
        short_tokens: &["MY"],
        regex_patterns: &[
            "北京.*马",
            "上海.*马",
            "广州.*马",
            "深圳.*马",
            "中马",
            "深马",
            "沪马",
            "京马",
            "广马",
        ],
    },
    CountryEntry {
        code: "TH",
        emoji: Some("\u{1F1F9}\u{1F1ED}"),
        cjk_keywords: &["泰国", "泰國"],
        long_tokens: &["THAILAND"],
        short_tokens: &["TH"],
        regex_patterns: &[
            "北京.*泰",
            "上海.*泰",
            "广州.*泰",
            "深圳.*泰",
            "中泰",
            "深泰",
            "沪泰",
            "京泰",
            "广泰",
        ],
    },
    CountryEntry {
        code: "VN",
        emoji: Some("\u{1F1FB}\u{1F1F3}"),
        cjk_keywords: &["越南"],
        long_tokens: &["VIETNAM", "VIET NAM"],
        short_tokens: &["VN"],
        regex_patterns: &[
            "北京.*越",
            "上海.*越",
            "广州.*越",
            "深圳.*越",
            "中越",
            "深越",
            "沪越",
            "京越",
            "广越",
        ],
    },
    CountryEntry {
        code: "ID",
        emoji: Some("\u{1F1EE}\u{1F1E9}"),
        cjk_keywords: &["印尼", "印度尼西亚"],
        long_tokens: &["INDONESIA"],
        short_tokens: &["ID"],
        regex_patterns: &[
            "北京.*尼",
            "上海.*尼",
            "广州.*尼",
            "深圳.*尼",
            "中尼",
            "深尼",
            "沪尼",
            "京尼",
            "广尼",
        ],
    },
    CountryEntry {
        code: "PH",
        emoji: Some("\u{1F1F5}\u{1F1ED}"),
        cjk_keywords: &["菲律宾", "菲律賓"],
        long_tokens: &["PHILIPPINES"],
        short_tokens: &["PH"],
        regex_patterns: &[
            "北京.*菲",
            "上海.*菲",
            "广州.*菲",
            "深圳.*菲",
            "中菲",
            "深菲",
            "沪菲",
            "京菲",
            "广菲",
        ],
    },
    CountryEntry {
        code: "AU",
        emoji: Some("\u{1F1E6}\u{1F1FA}"),
        cjk_keywords: &["澳大利亚", "澳洲"],
        long_tokens: &["AUSTRALIA"],
        short_tokens: &["AU"],
        regex_patterns: &[
            "北京.*澳",
            "上海.*澳",
            "广州.*澳",
            "深圳.*澳",
            "中澳",
            "深澳",
            "沪澳",
            "京澳",
            "广澳",
        ],
    },
    CountryEntry {
        code: "NZ",
        emoji: Some("\u{1F1F3}\u{1F1FF}"),
        cjk_keywords: &["新西兰", "紐西蘭"],
        long_tokens: &["NEW ZEALAND", "NEWZEALAND"],
        short_tokens: &["NZ"],
        regex_patterns: &[
            "北京.*新西",
            "上海.*新西",
            "广州.*新西",
            "深圳.*新西",
            "中新西",
            "深新西",
            "沪新西",
            "京新西",
            "广新西",
        ],
    },
    CountryEntry {
        code: "US",
        emoji: Some("\u{1F1FA}\u{1F1F8}"),
        cjk_keywords: &["美国", "美國"],
        long_tokens: &["UNITED STATES", "UNITEDSTATES", "AMERICA"],
        short_tokens: &["US", "USA"],
        regex_patterns: &[
            "北京.*美",
            "上海.*美",
            "广州.*美",
            "深圳.*美",
            "珠海.*美",
            "福州.*美",
            "厦门.*美",
            "杭州.*美",
            "南京.*美",
            "成都.*美",
            "武汉.*美",
            "西安.*美",
            "重庆.*美",
            "天津.*美",
            "大连.*美",
            "青岛.*美",
            "宁波.*美",
            "郑州.*美",
            "长沙.*美",
            "沈阳.*美",
            "济南.*美",
            "昆明.*美",
            "贵阳.*美",
            "南宁.*美",
            "哈尔滨.*美",
            "长春.*美",
            "石家庄.*美",
            "太原.*美",
            "兰州.*美",
            "乌鲁木齐.*美",
            "中美",
            "深美",
            "沪美",
            "京美",
            "广美",
            "珠美",
            "福美",
            "厦美",
        ],
    },
    CountryEntry {
        code: "CA",
        emoji: Some("\u{1F1E8}\u{1F1E6}"),
        cjk_keywords: &["加拿大"],
        long_tokens: &["CANADA"],
        short_tokens: &["CA"],
        regex_patterns: &[
            "北京.*加",
            "上海.*加",
            "广州.*加",
            "深圳.*加",
            "中加",
            "深加",
            "沪加",
            "京加",
            "广加",
        ],
    },
    CountryEntry {
        code: "BR",
        emoji: Some("\u{1F1E7}\u{1F1F7}"),
        cjk_keywords: &["巴西"],
        long_tokens: &["BRAZIL"],
        short_tokens: &["BR"],
        regex_patterns: &[
            "北京.*巴",
            "上海.*巴",
            "广州.*巴",
            "深圳.*巴",
            "中巴",
            "深巴",
            "沪巴",
            "京巴",
            "广巴",
        ],
    },
    CountryEntry {
        code: "MX",
        emoji: Some("\u{1F1F2}\u{1F1FD}"),
        cjk_keywords: &["墨西哥"],
        long_tokens: &["MEXICO"],
        short_tokens: &["MX"],
        regex_patterns: &[
            "北京.*墨",
            "上海.*墨",
            "广州.*墨",
            "深圳.*墨",
            "中墨",
            "深墨",
            "沪墨",
            "京墨",
            "广墨",
        ],
    },
    CountryEntry {
        code: "GB-UKM",
        emoji: Some("\u{1F1EC}\u{1F1E7}"),
        cjk_keywords: &["英国", "英國"],
        long_tokens: &["UNITED KINGDOM", "UNITEDKINGDOM", "BRITAIN"],
        short_tokens: &["GB", "UK"],
        regex_patterns: &[
            "北京.*英",
            "上海.*英",
            "广州.*英",
            "深圳.*英",
            "中英",
            "深英",
            "沪英",
            "京英",
            "广英",
        ],
    },
    CountryEntry {
        code: "DE",
        emoji: Some("\u{1F1E9}\u{1F1EA}"),
        cjk_keywords: &["德国", "德國"],
        long_tokens: &["GERMANY", "DEUTSCH"],
        short_tokens: &["DE"],
        regex_patterns: &[
            "北京.*德",
            "上海.*德",
            "广州.*德",
            "深圳.*德",
            "中德",
            "深德",
            "沪德",
            "京德",
            "广德",
        ],
    },
    CountryEntry {
        code: "FR",
        emoji: Some("\u{1F1EB}\u{1F1F7}"),
        cjk_keywords: &["法国", "法國"],
        long_tokens: &["FRANCE"],
        short_tokens: &["FR"],
        regex_patterns: &[
            "北京.*法",
            "上海.*法",
            "广州.*法",
            "深圳.*法",
            "中法",
            "深法",
            "沪法",
            "京法",
            "广法",
        ],
    },
    CountryEntry {
        code: "NL",
        emoji: Some("\u{1F1F3}\u{1F1F1}"),
        cjk_keywords: &["荷兰", "荷蘭"],
        long_tokens: &["NETHERLANDS", "HOLLAND"],
        short_tokens: &["NL"],
        regex_patterns: &[
            "北京.*荷",
            "上海.*荷",
            "广州.*荷",
            "深圳.*荷",
            "中荷",
            "深荷",
            "沪荷",
            "京荷",
            "广荷",
        ],
    },
    CountryEntry {
        code: "CH",
        emoji: Some("\u{1F1E8}\u{1F1ED}"),
        cjk_keywords: &["瑞士"],
        long_tokens: &["SWITZERLAND"],
        short_tokens: &["CH"],
        regex_patterns: &[
            "北京.*瑞士",
            "上海.*瑞士",
            "广州.*瑞士",
            "深圳.*瑞士",
            "中瑞士",
            "深瑞士",
            "沪瑞士",
            "京瑞士",
            "广瑞士",
        ],
    },
    CountryEntry {
        code: "SE",
        emoji: Some("\u{1F1F8}\u{1F1EA}"),
        cjk_keywords: &["瑞典"],
        long_tokens: &["SWEDEN"],
        short_tokens: &["SE"],
        regex_patterns: &[
            "北京.*瑞",
            "上海.*瑞",
            "广州.*瑞",
            "深圳.*瑞",
            "中瑞",
            "深瑞",
            "沪瑞",
            "京瑞",
            "广瑞",
        ],
    },
    CountryEntry {
        code: "NO",
        emoji: Some("\u{1F1F3}\u{1F1F4}"),
        cjk_keywords: &["挪威"],
        long_tokens: &["NORWAY"],
        short_tokens: &["NO"],
        regex_patterns: &[
            "北京.*挪",
            "上海.*挪",
            "广州.*挪",
            "深圳.*挪",
            "中挪",
            "深挪",
            "沪挪",
            "京挪",
            "广挪",
        ],
    },
    CountryEntry {
        code: "DK",
        emoji: Some("\u{1F1E9}\u{1F1F0}"),
        cjk_keywords: &["丹麦", "丹麥"],
        long_tokens: &["DENMARK"],
        short_tokens: &["DK"],
        regex_patterns: &[
            "北京.*丹",
            "上海.*丹",
            "广州.*丹",
            "深圳.*丹",
            "中丹",
            "深丹",
            "沪丹",
            "京丹",
            "广丹",
        ],
    },
    CountryEntry {
        code: "PL",
        emoji: Some("\u{1F1F5}\u{1F1F1}"),
        cjk_keywords: &["波兰", "波蘭"],
        long_tokens: &["POLAND"],
        short_tokens: &["PL"],
        regex_patterns: &[
            "北京.*波",
            "上海.*波",
            "广州.*波",
            "深圳.*波",
            "中波",
            "深波",
            "沪波",
            "京波",
            "广波",
        ],
    },
    CountryEntry {
        code: "RU",
        emoji: Some("\u{1F1F7}\u{1F1FA}"),
        cjk_keywords: &["俄罗斯", "俄羅斯"],
        long_tokens: &["RUSSIA"],
        short_tokens: &["RU"],
        regex_patterns: &[
            "北京.*俄",
            "上海.*俄",
            "广州.*俄",
            "深圳.*俄",
            "中俄",
            "深俄",
            "沪俄",
            "京俄",
            "广俄",
        ],
    },
    CountryEntry {
        code: "IN",
        emoji: Some("\u{1F1EE}\u{1F1F3}"),
        cjk_keywords: &["印度"],
        long_tokens: &["INDIA"],
        short_tokens: &["IN"],
        regex_patterns: &[
            "北京.*印",
            "上海.*印",
            "广州.*印",
            "深圳.*印",
            "中印",
            "深印",
            "沪印",
            "京印",
            "广印",
        ],
    },
    CountryEntry {
        code: "TR",
        emoji: Some("\u{1F1F9}\u{1F1F7}"),
        cjk_keywords: &["土耳其"],
        long_tokens: &["TURKEY", "TURKIYE"],
        short_tokens: &["TR"],
        regex_patterns: &[
            "北京.*土",
            "上海.*土",
            "广州.*土",
            "深圳.*土",
            "中土",
            "深土",
            "沪土",
            "京土",
            "广土",
        ],
    },
];

/// 预编译的 regex 缓存，与 COUNTRY_TABLE 一一对应。
static COMPILED_REGEXES: Lazy<Vec<Vec<Regex>>> = Lazy::new(|| {
    COUNTRY_TABLE
        .iter()
        .map(|entry| {
            entry
                .regex_patterns
                .iter()
                .filter_map(|p| Regex::new(p).ok())
                .collect()
        })
        .collect()
});

/// 根据节点名称推断 ISO alpha-2 国家/地区代码。
pub fn infer_country_from_name(name: &str) -> String {
    // 阶段 1：Emoji 精确匹配
    for entry in COUNTRY_TABLE {
        if let Some(emoji) = entry.emoji {
            if name.contains(emoji) {
                return entry.code.to_string();
            }
        }
    }

    let upper = name.to_ascii_uppercase();

    // 阶段 2：长英文词（4+ 字符）
    for entry in COUNTRY_TABLE {
        for &token in entry.long_tokens {
            if upper.contains(token) {
                return entry.code.to_string();
            }
        }
    }

    // 阶段 3：CJK 关键词
    for entry in COUNTRY_TABLE {
        for &kw in entry.cjk_keywords {
            if name.contains(kw) {
                return entry.code.to_string();
            }
        }
    }

    // 阶段 4：短代码词边界匹配
    for entry in COUNTRY_TABLE {
        for &token in entry.short_tokens {
            if word_boundary_contains(&upper, token) {
                return entry.code.to_string();
            }
        }
    }

    // 阶段 5：正则匹配（中转地+节点地区，低优先级）
    for (i, entry) in COUNTRY_TABLE.iter().enumerate() {
        if !entry.regex_patterns.is_empty() {
            for re in &COMPILED_REGEXES[i] {
                if re.is_match(name) {
                    return entry.code.to_string();
                }
            }
        }
    }

    "UNKNOWN".to_string()
}

/// 词边界匹配：检查 token 是否作为独立词出现在 text 中。
/// 要求 token 前后不能紧跟 ASCII 字母或数字。
/// 对 UTF-8 混合字符串安全（token 必须是纯 ASCII）。
fn word_boundary_contains(text: &str, token: &str) -> bool {
    let tbytes = token.as_bytes();
    let tlen = tbytes.len();
    if tlen == 0 || text.len() < tlen {
        return false;
    }
    let sbytes = text.as_bytes();
    for i in 0..=(sbytes.len() - tlen) {
        if &sbytes[i..i + tlen] == tbytes {
            let before_ok = i == 0 || !sbytes[i - 1].is_ascii_alphanumeric();
            let after_ok = i + tlen == sbytes.len() || !sbytes[i + tlen].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

/// 默认 IP 数据库版本。
pub const DEFAULT_IP_DATABASE_VERSION: &str = "2026.04.15";

/// 返回默认 IP 数据库版本。
pub fn default_ip_database_version() -> String {
    DEFAULT_IP_DATABASE_VERSION.to_string()
}

/// 返回当前 GeoIP 数据库文件路径。
pub fn geoip_database_path() -> Result<std::path::PathBuf, String> {
    let base = super::state::app_data_root()?;
    Ok(base.join("geoip").join("Country.mmdb"))
}

/// 检查本地 GeoIP 库文件是否存在。
pub fn geoip_database_exists() -> Result<bool, String> {
    Ok(geoip_database_path()?.exists())
}

/// 通过 API 服务查询出口 IP 的地理位置。
///
/// 如果 `proxy_url` 为 `Some(...)`，则通过指定代理发送请求；
/// 如果为 `None`，则直连。API 查询失败时 fallback 到本地 MMDB。
pub async fn fetch_egress_ip_via_proxy(proxy_url: Option<&str>) -> Result<GeoIpInfo, String> {
    debug!("[GeoIP] 查询出口 IP, proxy_url={:?}", proxy_url);
    let mut client_builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10));

    if let Some(url) = proxy_url {
        debug!("[GeoIP] 使用代理: {}", url);
        let proxy = reqwest::Proxy::all(url).map_err(|e| format!("创建代理失败: {e}"))?;
        client_builder = client_builder.proxy(proxy);
    }

    let client = client_builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let response = match client.get("https://api.ip.sb/geoip").send().await {
        Ok(r) => r,
        Err(e) => {
            error!("[GeoIP] 查询出口 IP 网络请求失败: {}", e);
            return Err(format!("查询出口 IP 失败: {}", e));
        }
    };

    let json: serde_json::Value = match response.json().await {
        Ok(j) => j,
        Err(e) => {
            error!("[GeoIP] 解析出口 IP 响应 JSON 失败: {}", e);
            return Err(format!("解析 GeoIP 响应失败: {}", e));
        }
    };

    let ip = json["ip"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    if ip == "Unknown" {
        error!(
            "[GeoIP] API 响应缺少 IP 字段，请求 URL: https://api.ip.sb/geoip, 响应内容: {}",
            json
        );
        return Err("响应中缺少 IP 字段".to_string());
    }

    let country_code = json["country_code"].as_str().unwrap_or("UN").to_string();
    let country_name = json["country"].as_str().unwrap_or("Unknown").to_string();
    let isp = json["isp"].as_str().unwrap_or("Unknown ISP").to_string();

    info!(
        "[GeoIP] 出口 IP: {} {}, ISP: {}",
        country_name, country_code, isp
    );

    Ok(GeoIpInfo {
        ip,
        country_code,
        country_name,
        isp,
    })
}

/// 直接查询 IP 的地理位置（无代理）。API 失败时 fallback 到本地 MMDB。
pub async fn fetch_geoip_direct(ip: &str) -> Result<GeoIpInfo, String> {
    debug!("[GeoIP] 直接查询 IP: {}", ip);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let response = match client
        .get(format!("https://api.ip.sb/geoip/{}", ip))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("[GeoIP] 直接查询 IP {} 网络请求失败: {}", ip, e);
            if let Some(local) = lookup_ip_local(ip) {
                warn!(
                    "[GeoIP] 使用本地 MMDB fallback: {} {} {}",
                    local.country_name, local.country_code, local.isp
                );
                return Ok(local);
            }
            return Err(format!("查询 IP {} 失败: {}", ip, e));
        }
    };

    let json: serde_json::Value = match response.json().await {
        Ok(j) => j,
        Err(e) => {
            error!("[GeoIP] 解析 IP {} 响应 JSON 失败: {}", ip, e);
            if let Some(local) = lookup_ip_local(ip) {
                warn!(
                    "[GeoIP] 使用本地 MMDB fallback: {} {} {}",
                    local.country_name, local.country_code, local.isp
                );
                return Ok(local);
            }
            return Err(format!("解析 GeoIP 响应失败: {}", e));
        }
    };

    let resolved_ip = json["ip"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| ip.to_string());

    if resolved_ip == "Unknown" || resolved_ip.is_empty() {
        error!(
            "[GeoIP] API 响应缺少 IP 字段，请求 IP: {}, 响应内容: {}",
            ip, json
        );
        if let Some(local) = lookup_ip_local(ip) {
            warn!(
                "[GeoIP] 使用本地 MMDB fallback: {} {} {}",
                local.country_name, local.country_code, local.isp
            );
            return Ok(local);
        }
        return Err("响应中缺少 IP 字段".to_string());
    }

    let country_code = json["country_code"].as_str().unwrap_or("UN").to_string();
    let country_name = json["country"].as_str().unwrap_or("Unknown").to_string();
    let isp = json["isp"].as_str().unwrap_or("Unknown ISP").to_string();

    info!(
        "[GeoIP] 直接查询 IP: {} {}, ISP: {}",
        country_name, country_code, isp
    );

    Ok(GeoIpInfo {
        ip: resolved_ip,
        country_code,
        country_name,
        isp,
    })
}

/// 使用本地 MMDB 数据库查询 IP 的地理位置（离线备选）。
pub fn lookup_ip_local(ip: &str) -> Option<GeoIpInfo> {
    let db_path = match geoip_database_path() {
        Ok(p) => p,
        Err(e) => {
            warn!("[GeoIP] 获取数据库路径失败: {}", e);
            return None;
        }
    };

    let reader: Reader<Vec<u8>> = match Reader::open_readfile(&db_path) {
        Ok(r) => r,
        Err(e) => {
            warn!("[GeoIP] 打开本地数据库失败: {}", e);
            return None;
        }
    };

    let ip_addr: std::net::IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => {
            warn!("[GeoIP] 无效 IP 格式: {}", ip);
            return None;
        }
    };

    let result = reader.lookup(ip_addr).ok()?;
    let country: maxminddb::geoip2::Country = match result.decode() {
        Ok(Some(c)) => c,
        Ok(None) => {
            warn!("[GeoIP] 本地数据库中未找到 IP: {}", ip);
            return None;
        }
        Err(e) => {
            warn!("[GeoIP] 解码本地数据库失败: {}", e);
            return None;
        }
    };

    let country_code = country
        .country
        .iso_code
        .map(|s| s.to_string())
        .unwrap_or_else(|| "UN".to_string());

    let country_name = country
        .country
        .names
        .english
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let isp = "Local MMDB".to_string();

    debug!(
        "[GeoIP] 本地查询 IP: {} -> {} {}",
        ip, country_name, country_code
    );

    Some(GeoIpInfo {
        ip: ip.to_string(),
        country_code,
        country_name,
        isp,
    })
}

/// 下载并替换 GeoIP 库，返回更新后的版本号。
pub async fn download_geoip_database() -> Result<String, String> {
    let target_path = geoip_database_path()?;
    let parent_dir = target_path
        .parent()
        .ok_or_else(|| "无法获取 GeoIP 目录".to_string())?;

    let parent_dir_owned = parent_dir.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&parent_dir_owned).map_err(|e| format!("创建 GeoIP 目录失败: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))??;

    let temp_path = target_path.with_extension("download");
    download_file_async(GEOIP_DOWNLOAD_URL, &temp_path, 3).await?;
    replace_file_with_backup_async(&temp_path, &target_path).await?;

    Ok(latest_ip_database_version())
}

const GEOIP_DOWNLOAD_URL: &str =
    "https://github.com/Loyalsoldier/geoip/releases/latest/download/Country.mmdb";

pub fn latest_ip_database_version() -> String {
    DEFAULT_IP_DATABASE_VERSION.to_string()
}

async fn download_file_async(
    url: &str,
    target_path: &std::path::Path,
    retries: usize,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("capyspeedtest/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    for attempt in 0..=retries {
        match download_file_once_async(&client, url, target_path).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < retries => {
                eprintln!("下载失败 (尝试 {}/{}): {}", attempt + 1, retries + 1, e);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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
    let result = tokio::task::spawn_blocking(move || {
        std::fs::write(&target, &bytes).map_err(|e| format!("写入文件失败: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))??;

    Ok(result)
}

async fn replace_file_with_backup_async(
    temp_path: &std::path::Path,
    target_path: &std::path::Path,
) -> Result<(), String> {
    let temp = temp_path.to_path_buf();
    let target = target_path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        std::fs::rename(&temp, &target).map_err(|e| format!("文件替换失败: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {e}"))??;

    Ok(result)
}
