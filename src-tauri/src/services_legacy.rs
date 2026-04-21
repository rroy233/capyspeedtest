//! services 模块重导出：保持向后兼容，实际实现委托给子模块。
//!
//! 新代码应优先使用 `crate::services::<子模块>` 的方式直接访问。

pub mod services;

pub use services::*;
