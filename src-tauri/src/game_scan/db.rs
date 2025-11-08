use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::path::Path;
use tauri::AppHandle;
use tauri::path::BaseDirectory;
use tauri::Manager;
use log::info;
use rusqlite::Connection;
// 注意：索引加载已固定使用默认 SQLite 路径，不再读取配置文件

/// 远端 PCGW 索引候选地址（优先顺序）
///
/// - 默认尝试从 GitHub Releases 的最新版本获取完整索引
/// - 若失败，回退到仓库主分支的原始文件路径
#[allow(dead_code)]
const REMOTE_INDEX_URLS: &[&str] = &[
    // Releases 最新版本的可下载资源（更可能包含完整数据集）
    "https://github.com/dyang886/Game-Save-Manager/releases/latest/download/pcgw_index.json",
    // 回退：主分支中的示例索引（若无发布资源时）
    "https://raw.githubusercontent.com/dyang886/Game-Save-Manager/main/src-tauri/gen/pcgw_index.json",
];

use super::types::GameInfo;
use super::types::PcgwIndexMeta;

/// PCGamingWiki 索引文件结构（最小子集）
#[derive(Debug, Serialize, Deserialize)]
struct PcgwIndex {
    /// 版本号，用于缓存更新与兼容性管理
    version: String,
    /// 游戏条目集合
    games: Vec<GameInfo>,
}

/// 加载 PCGW 索引（固定为程序资源目录下的 SQLite 路径）
///
/// - 输入：`app` 应用句柄（用于解析程序资源目录）
/// - 行为：使用 `AppHandle.path().resolve("database/database.db", BaseDirectory::Resource)`
/// - 返回：成功返回 `GameInfo` 列表，失败返回错误
pub async fn load_pcgw_index(app: &AppHandle) -> Result<Vec<GameInfo>> {
    let sqlite_path: PathBuf = app
        .path()
        .resolve("database/database.db", BaseDirectory::Resource)
        .context("Failed to resolve program resource path for database/database.db")?;

    if !sqlite_path.exists() {
        return Err(anyhow::anyhow!(format!(
            "PCGW sqlite not found at {}",
            sqlite_path.display()
        )));
    }

    let list = load_pcgw_index_from_sqlite_direct(&sqlite_path)
        .with_context(|| format!("Failed to load sqlite index at {}", sqlite_path.display()))?;
    info!(target:"rgsm::pcgw", "Loaded PCGW index from sqlite: {}", sqlite_path.display());
    Ok(list)
}

/// 加载 PCGW 索引的元信息（版本与条目数量，固定使用程序资源目录下的 SQLite）
///
/// - 输入：`app` 应用句柄（用于解析资源目录）
/// - 输出：`PcgwIndexMeta`（版本固定为 "sqlite"，数量为条目数）
pub async fn load_pcgw_index_meta(app: &AppHandle) -> Result<PcgwIndexMeta> {
    let sqlite_path: PathBuf = app
        .path()
        .resolve("database/database.db", BaseDirectory::Resource)
        .context("Failed to resolve program resource path for database/database.db")?;

    let games = load_pcgw_index_from_sqlite_direct(&sqlite_path)
        .with_context(|| format!("Failed to load sqlite index at {}", sqlite_path.display()))?;
    Ok(PcgwIndexMeta { version: Some("sqlite".into()), count: games.len() })
}

/// 远端下载并缓存 PCGW 索引到 AppData
///
/// - 行为：尝试从候选 URL 拉取 JSON；校验结构后写入缓存
/// - 缓存路径：`AppData/RGSM/pcgw_index.json`
/// - 返回：索引元信息（版本与条目数量），便于前端显示
/// 远端下载与 JSON 缓存更新机制已废弃；为兼容 IPC，此函数直接返回本地 SQLite 索引的元信息
pub async fn update_pcgw_index_remote(app: &AppHandle) -> Result<PcgwIndexMeta> {
    load_pcgw_index_meta(app).await
}

/// 从指定文件导入 PCGW 索引并写入缓存
///
/// - 输入：`src_path` 本地 JSON 文件路径
/// - 行为：读取并校验结构后写入 `AppData/RGSM/pcgw_index.json`
/// - 返回：索引元信息
pub async fn import_pcgw_index_from_file(app: &AppHandle, src_path: &Path) -> Result<PcgwIndexMeta> {
    let text = fs::read_to_string(src_path)
        .with_context(|| format!("Failed to read source file at {}", src_path.display()))?;
    let index: PcgwIndex = serde_json::from_str(&text)
        .context("Failed to parse provided PCGW index json")?;

    let cache_dir = app
        .path()
        .resolve("RGSM", BaseDirectory::AppData)
        .context("Failed to resolve AppData/RGSM directory")?;
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache dir at {}", cache_dir.display()))?;
    }
    let cache_path = cache_dir.join("pcgw_index.json");
    fs::write(&cache_path, &text)
        .with_context(|| format!("Failed to write index at {}", cache_path.display()))?;

    Ok(PcgwIndexMeta { version: Some(index.version), count: index.games.len() })
}

/// 从SQLite数据库（如 Game-Save-Manager 的 `database.db`）导入并转换为PCGW索引
///
/// - 输入：`sqlite_path` SQLite文件路径
/// - 行为：尽可能智能地探测表和列，提取 `name`、`aliases`、`pcgw_id` 以及可能的保存路径字段，生成最小可用的索引
/// - 输出：索引元信息（版本与条目数量）并写入缓存 `AppData/RGSM/pcgw_index.json`
pub async fn import_pcgw_index_from_sqlite(app: &AppHandle, sqlite_path: &Path) -> Result<PcgwIndexMeta> {
    let conn = Connection::open(sqlite_path)
        .with_context(|| format!("Failed to open sqlite at {}", sqlite_path.display()))?;

    // 列出所有表
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
    let table_names: Result<Vec<String>> = stmt
        .query_map([], |row| row.get::<usize, String>(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!(e))
        .with_context(|| "Failed to list tables");
    let table_names = table_names?;

    // 选择可能的游戏主表：包含 name 列的第一个表
    let mut game_table: Option<String> = None;
    let mut game_columns: Vec<String> = Vec::new();
    for t in table_names.iter() {
        let pragma = format!("PRAGMA table_info({})", t);
        let mut st = conn.prepare(&pragma)?;
        let cols: Vec<String> = st
            .query_map([], |row| row.get::<usize, String>(1))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!(e))?;
        if cols.iter().any(|c| c.eq_ignore_ascii_case("name") || c.eq_ignore_ascii_case("title")) {
            game_table = Some(t.clone());
            game_columns = cols;
            break;
        }
    }

    let Some(game_table) = game_table else {
        return Err(anyhow::anyhow!("No suitable game table found (requires a 'name' column)").into());
    };

    // 判断可能的列名
    let name_col = game_columns
        .iter()
        .find(|c| c.eq_ignore_ascii_case("name") || c.eq_ignore_ascii_case("title"))
        .cloned()
        .unwrap();
    let alias_col = game_columns
        .iter()
        .find(|c| c.eq_ignore_ascii_case("aliases") || c.eq_ignore_ascii_case("alias") || c.eq_ignore_ascii_case("aka"))
        .cloned();
    // 增强：识别本地化列（如 zh_CN、zh-cn、name_zh_cn 等）并作为别名来源
    let zh_like_col = game_columns
        .iter()
        .find(|c| {
            let lc = c.to_lowercase();
            lc == "zh_cn" || lc == "zh-cn" || lc == "zh" || lc.contains("name_zh") || lc.contains("chinese")
        })
        .cloned();
    let pcgw_col = game_columns
        .iter()
        .find(|c| c.eq_ignore_ascii_case("pcgw_id") || c.eq_ignore_ascii_case("slug") || c.eq_ignore_ascii_case("wiki_id") || c.eq_ignore_ascii_case("pcgw"))
        .cloned();

    // 提取行并转换为 GameInfo（基于列索引以保证稳定性）
    let mut games: Vec<GameInfo> = Vec::new();
    let sql = format!("SELECT * FROM {}", game_table);
    let mut s = conn.prepare(&sql)?;
    let col_names: Vec<String> = s
        .column_names()
        .iter()
        .map(|c| c.to_string())
        .collect();

    // 计算列索引
    let name_idx = col_names
        .iter()
        .position(|c| c.eq_ignore_ascii_case(&name_col))
        .ok_or_else(|| anyhow::anyhow!("name column index not found"))?;
    let alias_idx = alias_col
        .as_ref()
        .and_then(|ac| col_names.iter().position(|c| c.eq_ignore_ascii_case(ac)));
    let zh_like_idx = zh_like_col
        .as_ref()
        .and_then(|zc| col_names.iter().position(|c| c.eq_ignore_ascii_case(zc)));
    let pcgw_idx = pcgw_col
        .as_ref()
        .and_then(|pc| col_names.iter().position(|c| c.eq_ignore_ascii_case(pc)));

    // 识别可能的路径列索引
    let path_like_idxs: Vec<usize> = col_names
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let lc = c.to_lowercase();
            if lc.contains("path") || lc.contains("save") || lc.contains("location") || lc.contains("documents") {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    let mut rows = s.query([])?;
    while let Some(row) = rows.next()? {
        // 读取基础字段
        let name: String = row
            .get::<usize, String>(name_idx)
            .unwrap_or_default();
        if name.trim().is_empty() {
            continue;
        }

        let mut aliases: Vec<String> = if let Some(ai) = alias_idx {
            let sopt: Option<String> = row
                .get::<usize, Option<String>>(ai)
                .unwrap_or(None);
            sopt.map(|s| split_aliases(&s)).unwrap_or_default()
        } else {
            Vec::new()
        };
        // 将本地化中文字段并入别名，提高中文命中率
        if let Some(zi) = zh_like_idx {
            if let Some(zv) = row.get::<usize, Option<String>>(zi).unwrap_or(None) {
                let z = zv.trim().to_string();
                if !z.is_empty() && !aliases.iter().any(|a| a.eq_ignore_ascii_case(&z)) {
                    aliases.push(z);
                }
            }
        }

        let pcgw_id: Option<String> = if let Some(pi) = pcgw_idx {
            row.get::<usize, Option<String>>(pi).unwrap_or(None)
        } else { None };

        let mut gi = GameInfo {
            name,
            aliases,
            pcgw_id,
            install_rules: Vec::new(),
            save_rules: Vec::new(),
        };

        // 读取可能的路径列
        for idx in &path_like_idxs {
            let val_opt: Option<String> = row
                .get::<usize, Option<String>>(*idx)
                .unwrap_or(None);
            if let Some(val) = val_opt {
                if !val.trim().is_empty() {
                    gi.save_rules.push(super::types::SavePathRule {
                        id: format!("{}-{}", gi.name.replace(' ', "_"), col_names[*idx].as_str()),
                        description: Some(format!("Imported from {}.{}", game_table, col_names[*idx])),
                        path_template: normalize_path_template(&val),
                        requires: None,
                        platforms: vec!["windows".into()],
                        confidence: 0.6,
                    });
                }
            }
        }

        games.push(gi);
    }

    // 写入缓存
    let index = PcgwIndex { version: "db-import".into(), games };
    let cache_dir = app
        .path()
        .resolve("RGSM", BaseDirectory::AppData)
        .context("Failed to resolve AppData/RGSM directory")?;
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create cache dir at {}", cache_dir.display()))?;
    }
    let cache_path = cache_dir.join("pcgw_index.json");
    let text = serde_json::to_string(&index).context("Failed to serialize imported index")?;
    fs::write(&cache_path, &text)
        .with_context(|| format!("Failed to write index at {}", cache_path.display()))?;

    Ok(PcgwIndexMeta { version: Some(index.version), count: index.games.len() })
}

/// 直接从指定 SQLite 数据库加载 PCGW 索引（无需写入缓存）
///
/// - 输入：`sqlite_path` 为 SQLite 文件路径
/// - 输出：返回转换后的 `GameInfo` 列表；若失败则返回错误
/// - 行为：与导入逻辑一致，智能探测主表与列，并对可能的路径列进行模板规范化
fn load_pcgw_index_from_sqlite_direct(sqlite_path: &Path) -> Result<Vec<GameInfo>> {
    let conn = Connection::open(sqlite_path)
        .with_context(|| format!("Failed to open sqlite at {}", sqlite_path.display()))?;

    // 列出所有表
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
    let table_names: Result<Vec<String>> = stmt
        .query_map([], |row| row.get::<usize, String>(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!(e))
        .with_context(|| "Failed to list tables");
    let table_names = table_names?;

    // 选择可能的游戏主表：包含 name 列的第一个表
    let mut game_table: Option<String> = None;
    let mut game_columns: Vec<String> = Vec::new();
    for t in table_names.iter() {
        let pragma = format!("PRAGMA table_info({})", t);
        let mut st = conn.prepare(&pragma)?;
        let cols: Vec<String> = st
            .query_map([], |row| row.get::<usize, String>(1))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!(e))?;
        if cols.iter().any(|c| c.eq_ignore_ascii_case("name") || c.eq_ignore_ascii_case("title")) {
            game_table = Some(t.clone());
            game_columns = cols;
            break;
        }
    }

    let Some(game_table) = game_table else {
        return Err(anyhow::anyhow!("No suitable game table found (requires a 'name' column)").into());
    };

    // 判断可能的列名
    let name_col = game_columns
        .iter()
        .find(|c| c.eq_ignore_ascii_case("name") || c.eq_ignore_ascii_case("title"))
        .cloned()
        .unwrap();
    let alias_col = game_columns
        .iter()
        .find(|c| c.eq_ignore_ascii_case("aliases") || c.eq_ignore_ascii_case("alias") || c.eq_ignore_ascii_case("aka"))
        .cloned();
    // 增强：识别本地化列（如 zh_CN、zh-cn、name_zh_cn 等）并作为别名来源
    let zh_like_col = game_columns
        .iter()
        .find(|c| {
            let lc = c.to_lowercase();
            lc == "zh_cn" || lc == "zh-cn" || lc == "zh" || lc.contains("name_zh") || lc.contains("chinese")
        })
        .cloned();
    let pcgw_col = game_columns
        .iter()
        .find(|c| c.eq_ignore_ascii_case("pcgw_id") || c.eq_ignore_ascii_case("slug") || c.eq_ignore_ascii_case("wiki_id") || c.eq_ignore_ascii_case("pcgw"))
        .cloned();

    // 提取行并转换为 GameInfo
    let mut games: Vec<GameInfo> = Vec::new();
    let sql = format!("SELECT * FROM {}", game_table);
    let mut s = conn.prepare(&sql)?;
    let col_names: Vec<String> = s
        .column_names()
        .iter()
        .map(|c| c.to_string())
        .collect();

    // 计算列索引
    let name_idx = col_names
        .iter()
        .position(|c| c.eq_ignore_ascii_case(&name_col))
        .ok_or_else(|| anyhow::anyhow!("name column index not found"))?;
    let alias_idx = alias_col
        .as_ref()
        .and_then(|ac| col_names.iter().position(|c| c.eq_ignore_ascii_case(ac)));
    let zh_like_idx = zh_like_col
        .as_ref()
        .and_then(|zc| col_names.iter().position(|c| c.eq_ignore_ascii_case(zc)));
    let pcgw_idx = pcgw_col
        .as_ref()
        .and_then(|pc| col_names.iter().position(|c| c.eq_ignore_ascii_case(pc)));

    // 识别可能的路径列索引
    let path_like_idxs: Vec<usize> = col_names
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let lc = c.to_lowercase();
            if lc.contains("path") || lc.contains("save") || lc.contains("location") || lc.contains("documents") {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    let mut rows = s.query([])?;
    while let Some(row) = rows.next()? {
        // 读取基础字段
        let name: String = row
            .get::<usize, String>(name_idx)
            .unwrap_or_default();
        if name.trim().is_empty() {
            continue;
        }

        let mut aliases: Vec<String> = if let Some(ai) = alias_idx {
            let sopt: Option<String> = row
                .get::<usize, Option<String>>(ai)
                .unwrap_or(None);
            sopt.map(|s| split_aliases(&s)).unwrap_or_default()
        } else {
            Vec::new()
        };
        // 合并中文本地化列到别名列表
        if let Some(zi) = zh_like_idx {
            if let Some(zv) = row.get::<usize, Option<String>>(zi).unwrap_or(None) {
                let z = zv.trim().to_string();
                if !z.is_empty() && !aliases.iter().any(|a| a.eq_ignore_ascii_case(&z)) {
                    aliases.push(z);
                }
            }
        }

        let pcgw_id: Option<String> = if let Some(pi) = pcgw_idx {
            row.get::<usize, Option<String>>(pi).unwrap_or(None)
        } else { None };

        let mut gi = GameInfo {
            name,
            aliases,
            pcgw_id,
            install_rules: Vec::new(),
            save_rules: Vec::new(),
        };

        // 读取可能的路径列
        for idx in &path_like_idxs {
            let val_opt: Option<String> = row
                .get::<usize, Option<String>>(*idx)
                .unwrap_or(None);
            if let Some(val) = val_opt {
                if !val.trim().is_empty() {
                    gi.save_rules.push(super::types::SavePathRule {
                        id: format!("{}-{}", gi.name.replace(' ', "_"), col_names[*idx].as_str()),
                        description: Some(format!("Imported from {}.{}", game_table, col_names[*idx])),
                        path_template: normalize_path_template(&val),
                        requires: None,
                        platforms: vec!["windows".into()],
                        confidence: 0.6,
                    });
                }
            }
        }

        games.push(gi);
    }

    Ok(games)
}

// 读取逻辑在 `import_pcgw_index_from_sqlite` 中通过列索引直接完成。

/// 规范化路径模板：简易替换常见Windows路径为项目支持的占位符
fn normalize_path_template(p: &str) -> String {
    let mut s = p.trim().to_string();
    // 简单规则映射：用户文档与AppData系列
    if s.contains("\\Documents\\") || s.contains("/Documents/") {
        s = s.replace("%USERPROFILE%", "<home>");
        s = s.replace("C:/Users/%USERNAME%", "<home>");
        s = s.replace("%USERNAME%", "<osUserName>");
        if !s.to_lowercase().contains("<home>") {
            let suffix = if s.starts_with('/') { s.clone() } else { format!("/{}", s) };
            s = format!("<home>{}", suffix);
        }
    }
    if s.contains("AppData") {
        // 将 AppData/Roaming 映射到 <winAppData>
        s = s.replace("%APPDATA%", "<winAppData>");
        s = s.replace("C:/Users/%USERNAME%/AppData/Roaming", "<winAppData>");
    }
    s
}

/// 拆分别名字符串，支持逗号或竖线
fn split_aliases(s: &str) -> Vec<String> {
    s.split(|c| c == ',' || c == '|')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// 通过名称或别名在索引中查找游戏
///
/// - 输入：索引切片与待匹配名称
/// - 输出：找到的 `GameInfo`（若存在）
/// - 行为：大小写不敏感匹配，忽略前后空白；优先匹配主名称，其次匹配别名
pub fn find_by_name<'a>(index: &'a [GameInfo], name: &str) -> Option<&'a GameInfo> {
    let lower = name.trim().to_lowercase();
    index.iter().find(|g| {
        if g.name.to_lowercase() == lower {
            return true;
        }
        g.aliases.iter().any(|a| a.to_lowercase() == lower)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：从字符串解析最小 PCGW 索引并查询
    #[test]
    fn parse_pcgw_index_and_query() {
        let json = r#"{
            "version": "1.0.0",
            "games": [
                {
                    "name": "Example Game",
                    "aliases": ["EG"],
                    "pcgw_id": "example-game",
                    "install_rules": [],
                    "save_rules": []
                },
                {
                    "name": "Stardew Valley",
                    "aliases": ["SV"],
                    "pcgw_id": "stardew-valley",
                    "install_rules": [],
                    "save_rules": []
                }
            ]
        }"#;
        let idx: PcgwIndex = serde_json::from_str(json).expect("parse index json");
        assert_eq!(idx.version, "1.0.0");
        assert_eq!(idx.games.len(), 2);

        let g = find_by_name(&idx.games, "sv").expect("find by alias");
        assert_eq!(g.name, "Stardew Valley");
        assert_eq!(g.pcgw_id.as_deref(), Some("stardew-valley"));
    }
}
