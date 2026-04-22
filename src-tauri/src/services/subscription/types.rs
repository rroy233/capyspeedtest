//! 订阅解析模块 - 公共类型和常量

use serde_json::{Map as JsonMap, Value as JsonValue};

/// 默认下载测速服务器（不经过代理，直连测速用）。
pub const DEFAULT_TEST_FILE: &str = "http://speedtest.tele2.net/10MB.zip";
/// 默认上传测速服务器。
pub const DEFAULT_UPLOAD_TARGET: &str = "http://httpbin.org/post";
/// 内部行格式前缀：用于把 YAML 节点重新编码成单行，兼容前端"按 raw 回填再解析"流程。
pub const INTERNAL_PROXY_PREFIX: &str = "proxycfg://";

/// ProxyPayload 是解析过程中使用的中间数据结构
pub type ProxyPayload = JsonMap<String, JsonValue>;
