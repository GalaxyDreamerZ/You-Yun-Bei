use crate::cloud_sync::upload_game_snapshots;
use crate::config::{get_config, set_config, Config};
use crate::preclude::*;

use log::{error, info};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

use super::{Game, GameSnapshots};

/// 对 Windows 路径组件进行安全化处理
///
/// - 规则：移除/替换非法字符 `< > : " / \ | ? *` 为下划线 `_`
/// - 处理：去除结尾的空格与点；若结果为空则使用 `Game`
/// - 保留：大小写与非 ASCII 字符不做额外转换
pub fn sanitize_windows_path_component(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect();
    // 去除结尾的空格与点
    while s.ends_with(' ') || s.ends_with('.') { s.pop(); }
    if s.trim().is_empty() { return String::from("Game"); }
    s
}

/// 组合本地备份目录：`config.backup_path` + 安全化后的游戏名
pub fn join_backup_dir(config: &Config, name: &str) -> PathBuf {
    let safe = sanitize_windows_path_component(name);
    PathBuf::from(&config.backup_path).join(safe)
}

async fn create_backup_folder(name: &str) -> Result<(), BackupError> {
    let config = get_config()?;
    let backup_path = join_backup_dir(&config, name);
    let info: GameSnapshots = if !backup_path.exists() {
        fs::create_dir_all(&backup_path)?;
        GameSnapshots {
            name: name.to_string(),
            backups: Vec::new(),
        }
    } else {
        // 如果已经存在，info从原来的文件中读取
        let bytes = fs::read(backup_path.join("Backups.json"));
        serde_json::from_slice(&bytes?)?
    };
    fs::write(
        backup_path.join("Backups.json"),
        serde_json::to_string_pretty(&info)?,
    )?;

    // 处理云同步
    if config.settings.cloud_settings.always_sync {
        let op = config.settings.cloud_settings.backend.get_op()?;
        // 上传存档记录信息
        upload_game_snapshots(&op, info).await?;
    }

    Ok(())
}

pub async fn create_game_backup(game: &Game) -> Result<(), BackupError> {
    let mut config = get_config()?;
    create_backup_folder(&game.name).await?;

    // 查找是否存在与新游戏中的 `name` 字段相同的游戏
    let pos = config.games.iter().position(|g| g.name == game.name);
    match pos {
        Some(index) => {
            // 如果找到了，就用新的游戏覆盖它
            config.games[index] = game.clone();
        }
        None => {
            // 如果没有找到，就将新的游戏添加到 `games` 数组中
            config.games.push(game.clone());
        }
    }
    set_config(&config).await?;
    Ok(())
}

pub async fn backup_all() -> Result<(), BackupError> {
    let config = get_config()?;
    for game in &config.games {
        if let Err(e) = game.create_snapshot("Backup all").await {
            error!(target: "rgsm::backup", "Backup all failed for game {:#?}", game);
            return Err(e);
        } else {
            info!(target: "rgsm::backup", "Backup all succeeded for game {:#?}", game.name);
        }
    }
    Ok(())
}

pub async fn apply_all(app_handle: Option<&AppHandle>) -> Result<(), BackupError> {
    let config = get_config()?;
    for game in &config.games {
        let date = game
            .get_game_snapshots_info()?
            .backups
            .last()
            .ok_or(BackupError::NoBackupAvailable)?
            .date
            .clone();
        if let Err(e) = game.restore_snapshot(&date, app_handle) {
            error!(target: "rgsm::backup", "Apply all failed for game {:#?} with date {}", game, date);
            return Err(e);
        } else {
            info!(target: "rgsm::backup", "Apply all succeeded for game {:#?} with date {}", game.name, date);
        }
    }
    Ok(())
}
