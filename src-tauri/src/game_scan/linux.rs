#![cfg(target_os = "linux")]

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::backup::SaveUnit;
use super::types::{DetectedGame, GameInfo, SaveMatchResult, ScanOptions};

/// 在 Linux 平台检测已安装的游戏（存根实现）
///
/// - 输入：`ScanOptions` 控制不同来源的扫描开关
/// - 输出：返回空列表；后续将实现 Steam/Epic/Flatpak 等来源解析
pub async fn detect_installed_games(_options: &ScanOptions) -> Result<Vec<DetectedGame>> {
    log::info!(target: "rgsm::scan", "{}", rust_i18n::t!("scan.platform_beta"));
    Ok(Vec::new())
}

/// 在 Linux 平台匹配存档路径（存根实现）
///
/// - 输入：游戏信息与安装路径
/// - 输出：返回空匹配；后续将结合 XDG 目录规则/PCGW 索引实现
pub async fn match_save_paths(_game: &GameInfo, _install_path: &Path) -> Result<Vec<SaveMatchResult>> {
    log::info!(target: "rgsm::scan", "{}", rust_i18n::t!("scan.platform_beta"));
    Ok(Vec::new())
}

/// 在 Linux 平台生成保存单元（存根实现）
///
/// - 输入：游戏信息与安装路径
/// - 输出：返回空；后续将把匹配结果转换为 `SaveUnit`
pub async fn generate_save_units(_game: &GameInfo, _install_path: &Path) -> Result<Vec<SaveUnit>> {
    log::info!(target: "rgsm::scan", "{}", rust_i18n::t!("scan.platform_beta"));
    Ok(Vec::new())
}