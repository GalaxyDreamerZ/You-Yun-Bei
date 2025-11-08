//! 跨平台平台层（Platform Layer）
//!
//! 提供统一的接口以屏蔽不同操作系统的实现差异。
//! - Windows：复用已实现的 `windows` 模块。
//! - macOS/Linux：当前为安全存根，返回空结果并记录日志，逐步迭代完善。

use anyhow::Result;
use std::path::Path;
// 移除未使用的导入，保持编译无警告

use crate::backup::SaveUnit;
use super::types::{DetectedGame, GameInfo, SaveMatchResult, ScanOptions};

#[cfg(target_os = "windows")]
use crate::game_scan::windows;

/// 检测已安装的游戏（跨平台入口）
///
/// - Windows：调用 `windows::detect_installed_games`
/// - 非 Windows：返回空列表并输出 Beta/受限提示日志
pub async fn detect_installed_games(options: &ScanOptions) -> Result<Vec<DetectedGame>> {
    #[cfg(target_os = "windows")]
    {
        return windows::detect_installed_games(options).await;
    }

    #[cfg(not(target_os = "windows"))]
    {
        log::info!(target: "rgsm::scan", "{}", rust_i18n::t!("scan.platform_beta"));
        Ok(Vec::new())
    }
}

/// 匹配存档路径（跨平台入口）
///
/// - Windows：调用 `windows::match_save_paths`
/// - 非 Windows：返回空匹配并记录提示日志
pub async fn match_save_paths(game: &GameInfo, install_path: &Path) -> Result<Vec<SaveMatchResult>> {
    #[cfg(target_os = "windows")]
    {
        return windows::match_save_paths(game, install_path).await;
    }

    #[cfg(not(target_os = "windows"))]
    {
        log::info!(target: "rgsm::scan", "{}", rust_i18n::t!("scan.platform_beta"));
        Ok(Vec::new())
    }
}

/// 生成保存单元（跨平台入口）
///
/// - Windows：调用 `windows::generate_save_units`
/// - 非 Windows：返回空并记录提示日志
pub async fn generate_save_units(game: &GameInfo, install_path: &Path) -> Result<Vec<SaveUnit>> {
    #[cfg(target_os = "windows")]
    {
        return windows::generate_save_units(game, install_path).await;
    }

    #[cfg(not(target_os = "windows"))]
    {
        log::info!(target: "rgsm::scan", "{}", rust_i18n::t!("scan.platform_beta"));
        Ok(Vec::new())
    }
}