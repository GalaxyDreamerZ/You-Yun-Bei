#![cfg(target_os = "windows")]

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::{env, fs};
use log::{info, warn};

use super::types::{DetectedGame, GameInfo, SaveMatchResult, ScanOptions};
use super::types::DetectionSource;
use crate::game_scan::resolver::{default_env, resolve_save_rule};
use crate::backup::{SaveUnit, SaveUnitType};
use crate::device::get_current_device_id;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;
use regex::Regex;
use serde_json::Value;

/// 在 Windows 平台检测已安装的游戏
///
/// - 输入：`ScanOptions` 控制是否扫描 Steam / Epic / 注册表 / 常见目录 / 进程
/// - 输出：返回可能的 `DetectedGame` 列表，后续将用于存档匹配
///
/// 当前为函数存根：返回空列表，后续步骤将逐步实现具体检测逻辑。
/// 检测 Windows 平台已安装的游戏（基础版）
///
/// - 输入：`ScanOptions` 控制不同来源的扫描开关
/// - 输出：`DetectedGame` 列表（使用目录名作为游戏名的基础猜测）
/// - 实现：扫描常见的 Steam/Epic 默认库目录，枚举其子目录
/// 检测 Windows 平台已安装的游戏（包含 Steam 扫描与兜底目录枚举）
///
/// - 输入：`ScanOptions` 控制不同来源的扫描开关
/// - 输出：`DetectedGame` 列表
/// - 行为：
///   - 当启用 `search_steam` 时读取注册表与 `libraryfolders.vdf` 解析所有库并枚举游戏目录
///   - 当启用 `search_common_dirs` 时枚举默认的 Steam/Epic 常见目录作为兜底
/// 综合检测 Windows 平台已安装的游戏（Steam/Epic/Origin + 常见目录兜底）
///
/// - 输入：`ScanOptions` 控制不同来源的扫描开关
/// - 输出：`DetectedGame` 列表
/// - 合并策略：优先保留来源更可信的条目（平台特定 > 常见目录），按安装路径进行去重
pub async fn detect_installed_games(options: &ScanOptions) -> Result<Vec<DetectedGame>> {
    let mut detected = Vec::new();

    // 优先进行 Steam 深度扫描（注册表 + VDF）
    if options.search_steam {
        let steam_games = scan_steam_games(options).await?;
        detected.extend(steam_games);
    }

    // Epic（Manifest 解析）
    if options.search_epic {
        let epic_games = scan_epic_games(options).await?;
        detected.extend(epic_games);
    }

    // Origin/EA（installedGames.json / 目录兜底）
    if options.search_origin {
        let origin_games = scan_origin_games(options).await?;
        detected.extend(origin_games);
    }

    // 常见目录兜底扫描（统一标注为 CommonDir）
    if options.search_common_dirs {
        let common = scan_common_game_directories(options).await?;
        detected.extend(common);
    }

    // 对结果进行去重，优先按安装路径唯一性，其次按名称+来源
    Ok(dedup_detected(detected))
}

/// 扫描常见游戏安装目录（兜底策略）
///
/// - 目录来源：`PROGRAMFILES` 与 `PROGRAMFILES(X86)` 下的常见位置
/// - 当前覆盖：Steam/Epic/Origin/GOG/Ubisoft 的常见安装根目录
/// - 检测策略：枚举一级子目录，作为安装目录候选；来源标注为 `CommonDir`
/// - 返回：尽可能多的候选列表，后续由去重逻辑与规则匹配进一步筛选
pub async fn scan_common_game_directories(_options: &ScanOptions) -> Result<Vec<DetectedGame>> {
    let mut detected = Vec::new();

    // 读取 Program Files 根路径（支持覆盖）
    let pf = env::var("PROGRAMFILES").unwrap_or_else(|_| String::from("C\\\\Program Files"));
    let pfx86 = env::var("PROGRAMFILES(X86)").unwrap_or_else(|_| String::from("C\\\\Program Files (x86)"));

    // 常见目录集合
    let candidates: Vec<PathBuf> = vec![
        // Steam（兜底，若主库未识别）
        PathBuf::from(format!("{}\\Steam\\steamapps\\common", pf)),
        PathBuf::from(format!("{}\\Steam\\steamapps\\common", pfx86)),
        // Epic
        PathBuf::from(format!("{}\\Epic Games", pf)),
        PathBuf::from(format!("{}\\Epic Games", pfx86)),
        // Origin
        PathBuf::from(format!("{}\\Origin Games", pf)),
        PathBuf::from(format!("{}\\Origin Games", pfx86)),
        // GOG Galaxy
        PathBuf::from(format!("{}\\GOG Galaxy\\Games", pf)),
        PathBuf::from(format!("{}\\GOG Galaxy\\Games", pfx86)),
        // Ubisoft
        PathBuf::from(format!("{}\\Ubisoft\\Ubisoft Game Launcher\\games", pf)),
        PathBuf::from(format!("{}\\Ubisoft\\Ubisoft Game Launcher\\games", pfx86)),
    ];

    // 遍历一级子目录作为候选游戏安装目录
    for root in candidates.into_iter() {
        if let Ok(rd) = fs::read_dir(&root) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        let info = GameInfo {
                            name: name.to_string(),
                            aliases: Vec::new(),
                            pcgw_id: None,
                            install_rules: Vec::new(),
                            save_rules: Vec::new(),
                        };
                        detected.push(DetectedGame {
                            info,
                            install_path: Some(path),
                            source: DetectionSource::CommonDir,
                        });
                    }
                }
            }
        }
    }

    Ok(detected)
}

/// 从注册表与环境变量解析 Steam 安装路径
///
/// - 优先读取 `HKCU\Software\Valve\Steam` 的 `SteamPath`
/// - 若失败，尝试 `HKLM\Software\WOW6432Node\Valve\Steam` 的 `InstallPath`/`SteamPath`
/// - 提供环境变量 `RGSM_STEAM_PATH_OVERRIDE` 作为覆盖，便于测试与异常场景
fn get_steam_path_from_registry() -> Result<PathBuf> {
    // 环境变量覆盖（用于测试或自定义路径）
    if let Ok(override_path) = env::var("RGSM_STEAM_PATH_OVERRIDE") {
        let p = PathBuf::from(override_path);
        if p.exists() {
            return Ok(p);
        }
    }

    // HKCU
    let hcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hcu.open_subkey("Software\\Valve\\Steam") {
        if let Ok(val) = key.get_value::<String, _>("SteamPath") {
            let p = PathBuf::from(val);
            if p.exists() { return Ok(p); }
        }
        if let Ok(val) = key.get_value::<String, _>("InstallPath") {
            let p = PathBuf::from(val);
            if p.exists() { return Ok(p); }
        }
    }

    // HKLM - WOW6432Node（32 位路径）
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey("Software\\WOW6432Node\\Valve\\Steam") {
        if let Ok(val) = key.get_value::<String, _>("SteamPath") {
            let p = PathBuf::from(val);
            if p.exists() { return Ok(p); }
        }
        if let Ok(val) = key.get_value::<String, _>("InstallPath") {
            let p = PathBuf::from(val);
            if p.exists() { return Ok(p); }
        }
    }

    // 兜底：常见默认位置
    let pf = env::var("PROGRAMFILES").unwrap_or_else(|_| String::from("C\\\\Program Files"));
    let pfx86 = env::var("PROGRAMFILES(X86)").unwrap_or_else(|_| String::from("C\\\\Program Files (x86)"));
    let candidates = [
        PathBuf::from(format!("{}\\Steam", pf)),
        PathBuf::from(format!("{}\\Steam", pfx86)),
    ];
    for c in candidates { if c.exists() { return Ok(c); } }

    Err(anyhow::anyhow!("Steam path not found via registry or defaults"))
}

/// 解析 Steam 库文件 `libraryfolders.vdf` 并返回所有库路径
///
/// - 文件位置：`<steam_path>/steamapps/libraryfolders.vdf`
/// - 解析策略：容错地提取所有 `"path"` 字段的值
fn read_steam_library_folders(steam_path: &Path) -> Result<Vec<PathBuf>> {
    let vdf_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    let content = fs::read_to_string(&vdf_path)
        .with_context(|| format!("Failed to read libraryfolders.vdf: {}", vdf_path.display()))?;
    let paths = parse_libraryfolders_vdf(&content);
    let mut out = Vec::new();
    for p in paths {
        let pb = PathBuf::from(p);
        if pb.exists() { out.push(pb); }
    }
    // 将主库也加入（SteamPath）
    if steam_path.exists() { out.push(steam_path.to_path_buf()); }
    Ok(out)
}

/// 简易解析 `libraryfolders.vdf` 内容，收集所有 `path` 值
///
/// - 适配新版/旧版 KeyValues 格式，尽可能宽松地匹配
/// - 返回原始字符串路径列表（不判断存在性）
fn parse_libraryfolders_vdf(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let re = Regex::new(r#"path"\s*"([^"]+)"#).unwrap();
    for cap in re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let raw = m.as_str().trim();
            if !raw.is_empty() {
                // 规范化双反斜杠为单反斜杠，便于后续 Path 处理
                let normalized = raw.replace("\\\\", "\\");
                paths.push(normalized);
            }
        }
    }
    paths
}

/// 对检测到的游戏结果进行去重
///
/// - 主键：`install_path` 字符串（小写规范化）
/// - 备选键：`name + source`，当路径缺失时使用
/// 对检测到的游戏结果进行去重（Windows 路径规范化）
///
/// - 主键：规范化后的 `install_path` 字符串（统一分隔符、去除末尾分隔、转小写、尽量 canonicalize）
/// - 备选键：`name + source`，当路径缺失时使用
fn dedup_detected(items: Vec<DetectedGame>) -> Vec<DetectedGame> {
    use std::collections::HashSet;
    use std::path::Path;

    /// 规范化 Windows 路径为稳定的字符串键
    ///
    /// - 优先使用 `canonicalize` 获取真实路径；失败时回退原始路径
    /// - 统一分隔符为反斜杠，移除末尾反斜杠，最后转为小写
    fn normalize_win_path_key(p: &Path) -> String {
        let pb = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let s = pb.to_string_lossy().to_string();
        let s = s.replace('/', "\\");
        let s = s.trim_end_matches('\u{5c}').to_string();
        s.to_ascii_lowercase()
    }

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in items.into_iter() {
        let key = if let Some(ref p) = d.install_path {
            normalize_win_path_key(p)
        } else {
            format!("{}::{:?}", d.info.name.to_lowercase(), d.source)
        };
        if seen.insert(key) {
            out.push(d);
        }
    }
    out
}

/// 扫描 Steam 库目录中的已安装游戏
///
/// - 解析库列表后，遍历 `<library>/steamapps/common` 子目录，将每个子目录视为一个候选游戏
/// - 将来源标注为 `DetectionSource::Steam`
pub async fn scan_steam_games(_options: &ScanOptions) -> Result<Vec<DetectedGame>> {
    let mut detected = Vec::new();

    let steam_path = match get_steam_path_from_registry() {
        Ok(p) => p,
        Err(e) => {
            warn!(target:"rgsm::game_scan::windows", "Steam path not found: {e}");
            return Ok(detected); // 无法定位 Steam，返回空集合
        }
    };

    info!(target:"rgsm::game_scan::windows", "Steam path: {}", steam_path.display());

    let libraries = match read_steam_library_folders(&steam_path) {
        Ok(libs) => libs,
        Err(e) => {
            warn!(target:"rgsm::game_scan::windows", "Failed to read library folders: {e}");
            vec![steam_path.clone()]
        }
    };

    for lib in libraries {
        let common_dir = lib.join("steamapps").join("common");
        if let Ok(rd) = fs::read_dir(&common_dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        let info = GameInfo {
                            name: name.to_string(),
                            aliases: Vec::new(),
                            pcgw_id: None,
                            install_rules: Vec::new(),
                            save_rules: Vec::new(),
                        };
                        detected.push(DetectedGame {
                            info,
                            install_path: Some(path),
                            source: DetectionSource::Steam,
                        });
                    }
                }
            }
        }
    }

    Ok(detected)
}

/// 获取 ProgramData 根目录，支持环境变量覆盖（用于测试）
///
/// - 优先读取 `RGSM_PROGRAMDATA_OVERRIDE`
/// - 其次读取系统 `PROGRAMDATA`
/// - 失败时回退到 `C\ProgramData`
fn program_data_root() -> PathBuf {
    if let Ok(override_path) = env::var("RGSM_PROGRAMDATA_OVERRIDE") {
        let p = PathBuf::from(override_path);
        if p.exists() { return p; }
    }
    if let Ok(pd) = env::var("PROGRAMDATA") {
        let p = PathBuf::from(pd);
        if p.exists() { return p; }
    }
    PathBuf::from("C\\ProgramData")
}

/// 解析 Epic Manifests 下的单个清单文件，提取名称与安装路径
///
/// - 典型文件位于：`<ProgramData>/Epic/EpicGamesLauncher/Data/Manifests/*.item`
/// - 关键字段：`DisplayName` 或 `AppName`，`InstallLocation`
fn parse_epic_manifest_file(path: &Path) -> Option<(String, PathBuf)> {
    let content = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&content).ok()?;
    let name = v.get("DisplayName")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| v.get("AppName").and_then(|x| x.as_str()).map(|s| s.to_string()))?;
    let install_str = v.get("InstallLocation")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("installLocation").and_then(|x| x.as_str()))?;
    let install_path = PathBuf::from(install_str);
    if install_path.exists() { Some((name, install_path)) } else { None }
}

/// 扫描 Epic 已安装游戏（通过 ProgramData Manifests）
///
/// - 读取 Manifests 目录中 `.item`/`.manifest` 文件，解析安装路径
/// - 为每个有效条目创建 `DetectedGame`，来源标注为 `Epic`
pub async fn scan_epic_games(_options: &ScanOptions) -> Result<Vec<DetectedGame>> {
    let mut detected = Vec::new();
    let pd = program_data_root();

    let candidates = [
        pd.join("Epic").join("EpicGamesLauncher").join("Data").join("Manifests"),
        pd.join("EpicGamesLauncher").join("Data").join("Manifests"),
    ];

    let mut seen_paths = std::collections::HashSet::new();

    for dir in candidates.iter() {
        if let Ok(rd) = fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                // 只处理常见扩展
                let ext_ok = p.extension()
                    .and_then(|s| s.to_str())
                    .map(|e| matches!(e.to_ascii_lowercase().as_str(), "item" | "manifest"))
                    .unwrap_or(false);
                if !ext_ok { continue; }

                if let Some((name, install_path)) = parse_epic_manifest_file(&p) {
                    // 去重（按安装路径）
                    let key = install_path.to_string_lossy().to_string();
                    if seen_paths.insert(key) {
                        let info = GameInfo {
                            name,
                            aliases: Vec::new(),
                            pcgw_id: None,
                            install_rules: Vec::new(),
                            save_rules: Vec::new(),
                        };
                        detected.push(DetectedGame {
                            info,
                            install_path: Some(install_path),
                            source: DetectionSource::Epic,
                        });
                    }
                }
            }
        }
    }

    Ok(detected)
}

/// 解析 EA Desktop 的 `installedGames.json`，返回名称与安装路径列表
///
/// - 典型位置：`<ProgramData>/Electronic Arts/EA Desktop/installedGames.json`
/// - 解析策略：兼容对象或数组两种结构，优先读取 `displayName` 与 `installLocation`
fn parse_ea_installed_games_json(file: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let content = match fs::read_to_string(file) { Ok(s) => s, Err(_) => return out };
    let root: Value = match serde_json::from_str(&content) { Ok(v) => v, Err(_) => return out };

    fn extract_from_value(v: &Value, out: &mut Vec<(String, PathBuf)>) {
        match v {
            Value::Array(arr) => {
                for item in arr { extract_from_value(item, out); }
            }
            Value::Object(map) => {
                // 常见字段
                let name = map.get("displayName").and_then(|x| x.as_str())
                    .or_else(|| map.get("productName").and_then(|x| x.as_str()))
                    .or_else(|| map.get("title").and_then(|x| x.as_str()));
                let install = map.get("installLocation").and_then(|x| x.as_str())
                    .or_else(|| map.get("installationPath").and_then(|x| x.as_str()))
                    .or_else(|| map.get("path").and_then(|x| x.as_str()));
                if let (Some(n), Some(p)) = (name, install) {
                    let pb = PathBuf::from(p);
                    out.push((n.to_string(), pb));
                    return;
                }
                // 深度遍历
                for (_, vv) in map.iter() { extract_from_value(vv, out); }
            }
            _ => {}
        }
    }

    extract_from_value(&root, &mut out);
    out
}

/// 扫描 Origin/EA 已安装游戏
///
/// - 优先读取 EA Desktop 的 `installedGames.json`
/// - 若失败，回退枚举 `Origin Games` 目录
pub async fn scan_origin_games(_options: &ScanOptions) -> Result<Vec<DetectedGame>> {
    let mut detected = Vec::new();
    let pd = program_data_root();

    let ea_json = pd.join("Electronic Arts").join("EA Desktop").join("installedGames.json");
    if ea_json.exists() {
        for (name, install_path) in parse_ea_installed_games_json(&ea_json) {
            let info = GameInfo {
                name,
                aliases: Vec::new(),
                pcgw_id: None,
                install_rules: Vec::new(),
                save_rules: Vec::new(),
            };
            detected.push(DetectedGame {
                info,
                install_path: Some(install_path),
                source: DetectionSource::Origin,
            });
        }
    }

    // 兜底：枚举常见的 Origin 安装目录
    let pf = env::var("PROGRAMFILES").unwrap_or_else(|_| String::from("C\\Program Files"));
    let pfx86 = env::var("PROGRAMFILES(X86)")
        .unwrap_or_else(|_| String::from("C\\Program Files (x86)"));
    let origin_dirs = [
        PathBuf::from(format!("{}\\Origin Games", pf)),
        PathBuf::from(format!("{}\\Origin Games", pfx86)),
    ];
    for d in origin_dirs.iter() {
        if let Ok(rd) = fs::read_dir(d) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        let info = GameInfo {
                            name: name.to_string(),
                            aliases: Vec::new(),
                            pcgw_id: None,
                            install_rules: Vec::new(),
                            save_rules: Vec::new(),
                        };
                        detected.push(DetectedGame {
                            info,
                            install_path: Some(path),
                            source: DetectionSource::Origin,
                        });
                    }
                }
            }
        }
    }

    Ok(detected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_scan::types;
    use std::fs::create_dir_all;
    use std::io::Write;
    use std::sync::Mutex;

    // 测试环境串行锁，避免环境变量被并发修改导致不稳定
    static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// 测试：解析 libraryfolders.vdf 内容提取路径
    #[test]
    fn test_parse_libraryfolders_vdf() {
        let sample = r#"
        "libraryfolders"
        {
            "TimeNextStatsReport"    "12345"
            "contentstatsid" "-1234567890"
            "1"
            {
                "path"    "D:\\SteamLibrary"
                "label"   "Secondary"
                "mounted"   "1"
            }
            "2"
            {
                "path"    "E:\\Games\\SteamLib"
                "mounted"   "1"
            }
        }
        "#;
        let paths = parse_libraryfolders_vdf(sample);
        println!("paths = {:?}", paths);
        println!("eq? {}", paths.iter().any(|p| p == "D\\\\SteamLibrary"));
        println!("bytes0 = {:?}", paths.get(0).unwrap().as_bytes());
        assert!(paths.contains(&"D:\\SteamLibrary".to_string()));
        assert!(paths.contains(&"E:\\Games\\SteamLib".to_string()));
    }

    /// 测试：读取 libraryfolders.vdf 返回存在的库目录
    #[test]
    fn test_read_steam_library_folders() {
        let base = temp_dir::TempDir::new().unwrap();
        let steam_path = base.path().join("Steam");
        let steamapps = steam_path.join("steamapps");
        create_dir_all(&steamapps).unwrap();

        // 写入 vdf，路径指向 base/Steam
        let vdf_path = steamapps.join("libraryfolders.vdf");
        let mut f = std::fs::File::create(&vdf_path).unwrap();
        write!(
            f,
            "\n\"libraryfolders\"\n{{\n\"1\"\n{{\n\"path\"\t\"{}\"\n}}\n}}\n",
            steam_path.display()
        )
        .unwrap();

        let libs = read_steam_library_folders(&steam_path).unwrap();
        assert!(libs.iter().any(|p| p == &steam_path));
    }

    /// 测试：覆盖环境变量并完整扫描 common 目录枚举一个游戏
    #[test]
    fn test_scan_steam_games_with_override() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let base = temp_dir::TempDir::new().unwrap();
        let steam_path = base.path().join("Steam");
        let common_dir = steam_path.join("steamapps").join("common");
        create_dir_all(&common_dir).unwrap();
        // 写入 vdf 指向 steam_path
        let vdf_path = steam_path.join("steamapps").join("libraryfolders.vdf");
        let mut f = std::fs::File::create(&vdf_path).unwrap();
        write!(
            f,
            "\n\"libraryfolders\"\n{{\n\"1\"\n{{\n\"path\"\t\"{}\"\n}}\n}}\n",
            steam_path.display()
        )
        .unwrap();

        // 创建一个游戏目录
        let game_dir = common_dir.join("MyTestGame");
        create_dir_all(&game_dir).unwrap();

        // 设置覆盖变量并运行扫描
        // 设置环境变量覆盖（测试用）
        unsafe {
            std::env::set_var("RGSM_STEAM_PATH_OVERRIDE", &steam_path);
        }
        let opts = ScanOptions {
            platform: "windows".into(),
            search_steam: true,
            search_epic: false,
            search_origin: false,
            search_registry: true,
            search_common_dirs: false,
            search_processes: false,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let res = rt.block_on(scan_steam_games(&opts)).unwrap();
        assert!(res.iter().any(|d| d.info.name == "MyTestGame"));
    }

    /// 测试：Epic Manifests 解析（使用 ProgramData 覆盖）
    #[test]
    fn test_scan_epic_games_with_override() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        // 使用系统临时目录构造唯一目录，避免依赖外部 crate
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let pd = std::env::temp_dir().join(format!("rgsm_pd_epic_{}", millis));
        create_dir_all(&pd).expect("mkdir pd");
        let pd_str = pd.to_string_lossy().to_string();
        unsafe {
            std::env::set_var("RGSM_PROGRAMDATA_OVERRIDE", &pd_str);
            std::env::set_var("PROGRAMDATA", &pd_str);
        }

        let manifests = pd
            .join("Epic").join("EpicGamesLauncher").join("Data").join("Manifests");
        create_dir_all(&manifests).expect("mkdir manifests");

        let install_dir = pd.join("Games").join("MyEpicGame");
        create_dir_all(&install_dir).expect("mkdir install");

        let item_path = manifests.join("mygame.item");
        let install_str = install_dir.display().to_string().replace("\\", "\\\\");
        let sample = format!(r#"{{
            "DisplayName": "My Epic Game",
            "AppName": "MyEpicGame",
            "InstallLocation": "{}"
        }}"#, install_str);
        std::fs::write(&item_path, sample).expect("write item");

        // 直接验证清单解析是否生效
        assert!(parse_epic_manifest_file(&item_path).is_some());

        let opts = ScanOptions {
            platform: "windows".into(),
            search_steam: false,
            search_epic: true,
            search_origin: false,
            search_registry: false,
            search_common_dirs: false,
            search_processes: false,
        };

        let rt = tokio::runtime::Runtime::new().expect("rt");
        let res = rt.block_on(scan_epic_games(&opts)).expect("scan epic");
        assert!(!res.is_empty());
        assert_eq!(res[0].source, DetectionSource::Epic);
        assert_eq!(res[0].info.name, "My Epic Game");
        assert!(res[0].install_path.as_ref().unwrap().exists());
    }

    /// 测试：Origin/EA JSON 解析（使用 ProgramData 覆盖）
    #[test]
    fn test_scan_origin_games_with_override() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let pd = std::env::temp_dir().join(format!("rgsm_pd_ea_{}", millis));
        create_dir_all(&pd).expect("mkdir pd");
        let pd_str = pd.to_string_lossy().to_string();
        unsafe {
            std::env::set_var("RGSM_PROGRAMDATA_OVERRIDE", &pd_str);
            std::env::set_var("PROGRAMDATA", &pd_str);
        }

        let ea_dir = pd.join("Electronic Arts").join("EA Desktop");
        create_dir_all(&ea_dir).expect("mkdir ea");

        let install_dir = pd.join("Games").join("MyEASeed");
        create_dir_all(&install_dir).expect("mkdir install");

        let json = ea_dir.join("installedGames.json");
        let install_str = install_dir.display().to_string().replace("\\", "\\\\");
        let sample = format!(r#"[
            {{
                "displayName": "My EA Game",
                "installLocation": "{}"
            }}
        ]"#, install_str);
        std::fs::write(&json, sample).expect("write json");

        // 直接验证 JSON 解析是否生效
        let parsed = parse_ea_installed_games_json(&json);
        assert!(!parsed.is_empty());

        let opts = ScanOptions {
            platform: "windows".into(),
            search_steam: false,
            search_epic: false,
            search_origin: true,
            search_registry: false,
            search_common_dirs: false,
            search_processes: false,
        };

        let rt = tokio::runtime::Runtime::new().expect("rt");
        let res = rt.block_on(scan_origin_games(&opts)).expect("scan origin");
        assert!(!res.is_empty());
        assert_eq!(res[0].source, DetectionSource::Origin);
        assert_eq!(res[0].info.name, "My EA Game");
        assert!(res[0].install_path.as_ref().unwrap().exists());
    }

    /// 测试：常见目录扫描（覆盖 PROGRAMFILES 指向临时目录）
    #[test]
    fn test_scan_common_dirs_with_override() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!("rgsm_pf_common_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
        create_dir_all(&base).expect("mkdir base");
        let pf_str = base.to_string_lossy().to_string();
        unsafe {
            std::env::set_var("PROGRAMFILES", &pf_str);
        }

        // 构造 GOG Galaxy 常见路径与一个游戏目录
        let gog_games = base.join("GOG Galaxy").join("Games");
        let my_game = gog_games.join("MyCommonGame");
        create_dir_all(&my_game).expect("mkdir gog game");

        let opts = ScanOptions {
            platform: "windows".into(),
            search_steam: false,
            search_epic: false,
            search_origin: false,
            search_registry: false,
            search_common_dirs: true,
            search_processes: false,
        };

        let rt = tokio::runtime::Runtime::new().expect("rt");
        let res = rt
            .block_on(super::scan_common_game_directories(&opts))
            .expect("scan common");
        assert!(res.iter().any(|d| d.source == DetectionSource::CommonDir && d.info.name == "MyCommonGame"));
    }

    /// 验证 SaveUnit 生成逻辑（基于存在路径与当前设备映射）
    #[test]
    fn test_generate_save_units_from_matches() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();

        // 创建一个临时目录作为匹配目标
        let base = std::env::temp_dir().join("rgsm_unit_test");
        let _ = std::fs::remove_dir_all(&base);
        create_dir_all(&base).expect("mkdir base");
        let folder = base.join("Saves");
        create_dir_all(&folder).expect("mkdir folder");

        // 构造一个 GameInfo 与保存规则，直接指向上面的临时目录
        let rule = types::SavePathRule {
            id: "r1".into(),
            description: None,
            path_template: folder.to_string_lossy().to_string(),
            requires: None,
            platforms: vec!["windows".into()],
            confidence: 1.0,
        };

        let game = GameInfo {
            name: "UnitGame".into(),
            aliases: Vec::new(),
            pcgw_id: None,
            install_rules: Vec::new(),
            save_rules: vec![rule],
        };

        let rt = tokio::runtime::Runtime::new().expect("rt");
        let units = rt
            .block_on(super::generate_save_units(&game, &base))
            .expect("generate units");

        assert!(!units.is_empty(), "should generate at least one save unit");
        let device_id = get_current_device_id().clone();
        let has_mapping = units.iter().any(|u| u.paths.get(&device_id).is_some());
        assert!(has_mapping, "save unit should contain path mapping for current device");
    }
}

/// 在 Windows 平台为指定游戏尝试匹配存档路径
///
/// - 输入：`GameInfo` 与可选的安装路径，用于解析规则模板
/// - 输出：`SaveMatchResult` 列表，包含解析出的路径与存在性、可信度
///
/// 当前为函数存根：返回空列表，后续步骤将逐步实现解析与校验逻辑。
/// 按规则匹配 Windows 平台的存档路径（基础版）
///
/// - 输入：`GameInfo` 与安装路径
/// - 输出：`SaveMatchResult` 列表，包含路径存在性与可信度
pub async fn match_save_paths(
    game: &GameInfo,
    install_path: &Path,
) -> Result<Vec<SaveMatchResult>> {
    // 针对指定游戏匹配存档路径（Windows）
    // - 基于 PCGW 规则解析 `<...>` 与环境变量，生成候选路径
    // - 额外为特殊游戏提供兜底匹配（如 Black Myth: Wukong 存档在安装目录下）
    // - 返回包含存在性标记与可信度的匹配结果列表
    // 测试环境避免读取真实配置文件，使用默认配置构建解析环境
    let env = default_env(&crate::config::Config::default());

    let mut results = Vec::new();

    // 遍历规则，解析模板并进行存在性校验
    for rule in &game.save_rules {
        let paths = resolve_save_rule(rule, &env)?;
        for p in paths {
            let exists = p.exists();
            let confidence = if exists { rule.confidence.min(1.0) } else { rule.confidence * 0.5 };
            results.push(SaveMatchResult {
                rule_id: rule.id.clone(),
                resolved_path: p,
                exists,
                confidence,
            });
        }
    }

    // 预留：可利用安装路径提升匹配质量（如通过占位符替换）
    let _install_path = install_path.to_path_buf();

    // 特例兜底：Black Myth: Wukong（黑神话：悟空）——优先匹配安装目录下的 SaveGames
    // 路径形式：<install>/b1/Saved/SaveGames[/<SteamId>]
    // 若存在 .sav 文件的子目录，则返回该子目录；否则返回 SaveGames 目录本身。
    let normalized = |s: &str| s.to_ascii_lowercase().replace([' ', ':', '_'], "");
    let is_bmw = normalized(&game.name).contains("blackmythwukong")
        || game.aliases.iter().any(|a| normalized(a).contains("blackmythwukong"));
    if is_bmw {
        let base = install_path.join("b1").join("Saved").join("SaveGames");
        if base.is_dir() {
            let mut picked: Option<PathBuf> = None;
            if let Ok(rd) = std::fs::read_dir(&base) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        if let Ok(sub) = std::fs::read_dir(&p) {
                            let has_sav = sub.flatten().any(|e| {
                                e.path().is_file()
                                    && e.path()
                                        .extension()
                                        .and_then(|x| x.to_str())
                                        .map(|ext| ext.eq_ignore_ascii_case("sav"))
                                        .unwrap_or(false)
                            });
                            if has_sav {
                                picked = Some(p);
                                break;
                            }
                        }
                    }
                }
            }

            let target = picked.unwrap_or(base);
            results.push(SaveMatchResult {
                rule_id: "bmw-install-savegames".into(),
                resolved_path: target,
                exists: true,
                confidence: 0.99,
            });
        }
    }

    // 通用兜底：在常见用户目录中尝试按游戏名/别名匹配存档根目录
    for p in search_common_save_roots(game)? {
        results.push(SaveMatchResult {
            rule_id: "common-roots-name-match".into(),
            resolved_path: p,
            exists: true,
            confidence: 0.90,
        });
    }

    Ok(results)
}

/// 生成 SaveUnit（含设备路径映射）
///
/// - 输入：`GameInfo` 与安装路径，用于路径解析与存在性检查
/// - 输出：`SaveUnit` 列表，仅包含存在的路径，并映射到当前设备 ID
pub async fn generate_save_units(
    game: &GameInfo,
    install_path: &Path,
) -> Result<Vec<SaveUnit>> {
    let matches = match_save_paths(game, install_path).await?;
    let device_id = get_current_device_id().clone();

    // 去重并优先保留更“像存档”的路径（含典型扩展或命名）
    let mut units = Vec::new();
    let mut best_by_path: std::collections::HashMap<String, (f32, SaveMatchResult)> =
        std::collections::HashMap::new();
    for m in matches.into_iter().filter(|m| m.exists) {
        let key = m.resolved_path.to_string_lossy().to_string();
        let score_bonus = if is_plausible_save_dir(&m.resolved_path) { 0.1 } else { 0.0 };
        let score = m.confidence + score_bonus;
        match best_by_path.get(&key) {
            Some((prev, _)) if *prev >= score => {}
            _ => {
                best_by_path.insert(key, (score, m));
            }
        }
    }

    for (_, (_, m)) in best_by_path.into_iter() {
        let unit_type = if m.resolved_path.is_file() {
            SaveUnitType::File
        } else {
            SaveUnitType::Folder
        };
        let mut paths = std::collections::HashMap::new();
        paths.insert(device_id.clone(), m.resolved_path.to_string_lossy().to_string());
        units.push(SaveUnit { unit_type, paths, delete_before_apply: false });
    }

    Ok(units)
}

/// 判断目录是否“像”存档目录
///
/// - 规则：包含常见扩展的文件（如 `.sav`, `.save`, `.slot`, `.dat`）或名称包含 `save` 的子目录
/// - 目的：提高候选路径质量评分，减少错误目录被加入配置
fn is_plausible_save_dir(path: &Path) -> bool {
    if path.is_file() {
        return path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "sav" | "save" | "slot" | "dat"))
            .unwrap_or(false);
    }

    if !path.is_dir() {
        return false;
    }

    let mut has_save_file = false;
    let mut has_save_named_dir = false;
    if let Ok(rd) = std::fs::read_dir(path) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_file() {
                if p.extension()
                    .and_then(|e| e.to_str())
                    .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "sav" | "save" | "slot" | "dat"))
                    .unwrap_or(false) {
                    has_save_file = true;
                }
            } else if p.is_dir() {
                if p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|n| n.to_ascii_lowercase().contains("save"))
                    .unwrap_or(false) {
                    has_save_named_dir = true;
                }
            }
            if has_save_file || has_save_named_dir { break; }
        }
    }

    has_save_file || has_save_named_dir
}

/// 在常见用户目录中按游戏名/别名匹配潜在的存档根目录
///
/// - 搜索范围：`Documents`、`Saved Games`、`LocalAppData`、`AppData/Roaming`
/// - 规则：目录名包含游戏名或别名的规范化形式，并且目录下包含存档特征
/// 在常见用户目录中按游戏名/别名匹配潜在的存档根目录
///
/// - 搜索范围：`Documents`、`Saved Games`、`LocalAppData`、`AppData/Roaming`
/// - 规则：
///   1) 目录名包含游戏名或别名的规范化形式，并且目录下包含存档特征
///   2) 支持厂商目录的二级匹配（例如 `Saved Games/Quantic Dream/Detroit Become Human`）
fn search_common_save_roots(game: &GameInfo) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    if let Ok(user) = std::env::var("USERPROFILE") {
        roots.push(Path::new(&user).join("Documents"));
        roots.push(Path::new(&user).join("Saved Games"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(Path::new(&local).to_path_buf());
    }
    if let Ok(roam) = std::env::var("APPDATA") {
        roots.push(Path::new(&roam).to_path_buf());
    }

    let mut candidates = Vec::new();
    let tokens: Vec<String> = std::iter::once(game.name.clone())
        .chain(game.aliases.clone())
        .map(|s| s.to_ascii_lowercase().replace([' ', ':', '_'], ""))
        .collect();

    for root in roots {
        if !root.is_dir() { continue; }
        if let Ok(rd) = std::fs::read_dir(&root) {
            for entry in rd.flatten() {
                let p = entry.path();
                if !p.is_dir() { continue; }
                let name = p.file_name().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase());

                // 1) 直接匹配：目录名包含游戏名/别名
                let mut matched_dirs: Vec<PathBuf> = Vec::new();
                if let Some(n) = name.clone() {
                    let norm = n.replace([' ', ':', '_'], "");
                    if tokens.iter().any(|t| norm.contains(t)) {
                        matched_dirs.push(p.clone());
                    }
                }

                // 2) 厂商目录二级匹配：在当前目录下查找名称命中游戏名/别名的子目录
                if matched_dirs.is_empty() {
                    if let Ok(sub) = std::fs::read_dir(&p) {
                        for s in sub.flatten() {
                            let q = s.path();
                            if !q.is_dir() { continue; }
                            let qn = q.file_name().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase());
                            if let Some(qn) = qn {
                                let qnorm = qn.replace([' ', ':', '_'], "");
                                if tokens.iter().any(|t| qnorm.contains(t)) {
                                    matched_dirs.push(q.clone());
                                    break; // 命中一个即可
                                }
                            }
                        }
                    }
                }

                // 对命中的目录进行“像存档”检查，必要时尝试常见子目录
                for mdir in matched_dirs {
                    let mut candidate = None;
                    if is_plausible_save_dir(&mdir) {
                        candidate = Some(mdir.clone());
                    } else {
                        let sub_names = ["SaveGames", "SaveData", "Saves", "Profiles"];
                        for sub in &sub_names {
                            let subdir = mdir.join(sub);
                            if is_plausible_save_dir(&subdir) {
                                candidate = Some(subdir);
                                break;
                            }
                        }
                    }
                    if let Some(c) = candidate { candidates.push(c); }
                }
            }
        }
    }

    Ok(candidates)
}
