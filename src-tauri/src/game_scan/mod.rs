//! 自动扫描游戏存档模块
//!
//! 该模块负责：
//! - 集成 PCGamingWiki 等数据源的游戏信息索引
//! - 检测本机已安装的游戏（按平台）
//! - 根据规则匹配游戏存档路径并解析变量
//! - 通过事件向前端报告扫描进度与结果
//!
//! 当前步骤仅提供类型与函数存根，后续步骤将逐步完善实现与命令注册。

mod db;
mod resolver;
pub mod types;
mod ipc;
mod platform;

// 仅在 Windows 平台编译 Windows 检测逻辑
#[cfg(target_os = "windows")]
mod windows;

// 仅在 Linux 平台编译 Linux 检测逻辑（存根）
#[cfg(target_os = "linux")]
mod linux;

// 仅在 macOS 平台编译 macOS 检测逻辑（存根）
#[cfg(target_os = "macos")]
mod macos;

// 对外导出常用类型
pub use ipc::*;
