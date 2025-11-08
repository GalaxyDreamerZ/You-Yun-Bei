use anyhow::Result;
use log::{info, warn};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tauri_specta::Event;

use super::types::{DetectedGame, SaveMatchResult, ScanOptions, ScanProgressEvent, ScanResult};
use crate::game_scan::platform::{detect_installed_games, match_save_paths, generate_save_units};
use super::db::{load_pcgw_index, find_by_name};
use super::types::{PcgwQueryOptions, PcgwQueryItem, PcgwIndexMeta};
use super::db::{update_pcgw_index_remote, import_pcgw_index_from_file, import_pcgw_index_from_sqlite};

/// 扫描进度事件（用于前端订阅显示）
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[derive(Event)]
pub struct ScanProgress(pub ScanProgressEvent);

/// 进度事件发送器（带节流）
///
/// - 目的：避免在短时间内重复发送相同或同一步骤的事件，降低前端渲染压力与 IPC 频率
/// - 策略：
///   1. 步骤切换（step 字段变化）时总是立刻发送；
///   2. 同一步骤在 `min_interval` 时间内重复事件将被抑制；
///   3. 若事件内容完全一致（step/current/total/message 相同），即使超过间隔也跳过一次以减少冗余；
struct ProgressEmitter {
    app: AppHandle,
    last_emit_at: Option<Instant>,
    last_step: Option<String>,
    last_payload: Option<ScanProgressEvent>,
    min_interval: Duration,
}

impl ProgressEmitter {
    /// 创建一个新的进度事件发送器
    fn new(app: AppHandle, min_interval: Duration) -> Self {
        Self {
            app,
            last_emit_at: None,
            last_step: None,
            last_payload: None,
            min_interval,
        }
    }

    /// 发送进度事件（遵循节流策略）
    fn emit(&mut self, payload: ScanProgressEvent) {
        let now = Instant::now();

        // 步骤变化，立即发送
        let step_changed = match &self.last_step {
            Some(s) => s != &payload.step,
            None => true,
        };
        if step_changed {
            let _ = ScanProgress(payload.clone()).emit(&self.app);
            self.last_step = Some(payload.step.clone());
            self.last_emit_at = Some(now);
            self.last_payload = Some(payload);
            return;
        }

        // 内容完全重复，跳过一次
        if let Some(prev) = &self.last_payload {
            if prev.step == payload.step
                && prev.current == payload.current
                && prev.total == payload.total
                && prev.message == payload.message
            {
                return;
            }
        }

        // 同一步骤内的节流
        if let Some(last) = self.last_emit_at {
            if now.duration_since(last) < self.min_interval {
                return;
            }
        }

        let _ = ScanProgress(payload.clone()).emit(&self.app);
        self.last_emit_at = Some(now);
        self.last_payload = Some(payload);
    }
}

/// 触发扫描流程的命令（最小实现）
///
/// - 输入：`ScanOptions` 控制扫描选项，`AppHandle` 用于事件发送
/// - 输出：`ScanResult` 扫描结果（当前为最小实现，返回空集合）
/// - 行为：按阶段发送两到三次 `ScanProgress` 事件，便于前端调试 UI 与绑定
/// 扫描入口命令
///
/// - 输入：`options` 控制扫描行为；`app` 用于事件发送与资源解析
/// - 行为：阶段化发送 `ScanProgress` 事件，并记录各阶段耗时；
/// - 输出：返回聚合的检测与存档匹配结果
#[tauri::command]
#[specta::specta]
pub async fn scan_games(app: AppHandle, options: ScanOptions) -> Result<ScanResult, String> {
    info!(target:"rgsm::game_scan", "Starting scan with options: {:?}", options);
    let mut emitter = ProgressEmitter::new(app.clone(), Duration::from_millis(250));
    let t_total = Instant::now();

    // 预读取 PCGW 索引（最小实现）：用于丰富检测结果的规则信息
    let t_index = Instant::now();
    let pcgw_index: Vec<super::types::GameInfo> = match load_pcgw_index(&app).await {
        Ok(list) => list,
        Err(e) => {
            warn!(target:"rgsm::game_scan", "Failed to load PCGW index: {e}");
            Vec::new()
        }
    };
    info!(target:"rgsm::game_scan", "PCGW index loaded in {:?}, entries: {}", t_index.elapsed(), pcgw_index.len());

    // Step 1: 发送索引加载进度
    emitter.emit(ScanProgressEvent {
        step: "index_load".into(),
        current: 1,
        total: 4,
        message: Some(t!("backend.scan.index_load").to_string()),
    });

    // TODO: 后续实现实际的索引加载、Windows 检测与路径匹配

    // Step 2: 发送检测游戏进度
    emitter.emit(ScanProgressEvent {
        step: "detect_games".into(),
        current: 2,
        total: 4,
        message: Some(t!("backend.scan.detect_games").to_string()),
    });

    // 细化平台扫描阶段事件（Epic / Origin），用于前端显示更细粒度进度
    if options.search_epic {
        emitter.emit(ScanProgressEvent {
            step: "epic_scanning".into(),
            current: 2,
            total: 4,
            message: Some("Scanning Epic manifests".into()),
        });
    }
    if options.search_origin {
        emitter.emit(ScanProgressEvent {
            step: "origin_scanning".into(),
            current: 2,
            total: 4,
            message: Some("Scanning EA/Origin installed list".into()),
        });
    }
    if options.search_common_dirs {
        emitter.emit(ScanProgressEvent {
            step: "common_directories_scanning".into(),
            current: 2,
            total: 4,
            message: Some("Scanning common game directories".into()),
        });
    }

    // 执行平台检测（Windows 基础版）
    let t_detect = Instant::now();
    let detected: Vec<DetectedGame> = {
        #[cfg(target_os = "windows")]
        {
            detect_installed_games(&options)
                .await
                .map_err(|e| e.to_string())?
        }
        #[cfg(not(target_os = "windows"))]
        {
            Vec::new()
        }
    };
    info!(target:"rgsm::game_scan", "Detected {} game candidates in {:?}", detected.len(), t_detect.elapsed());

    // 合并/丰富检测结果：按名称或别名匹配 PCGW 索引，将规则注入
    let detected = enrich_with_pcgw(detected, &pcgw_index);
    info!(target:"rgsm::game_scan", "Enriched detections with PCGW, total: {}", detected.len());

    // 平台扫描完成事件（Epic / Origin）
    if options.search_epic {
        emitter.emit(ScanProgressEvent {
            step: "epic_done".into(),
            current: 2,
            total: 4,
            message: Some("Epic scan done".into()),
        });
    }
    if options.search_origin {
        emitter.emit(ScanProgressEvent {
            step: "origin_done".into(),
            current: 2,
            total: 4,
            message: Some("Origin scan done".into()),
        });
    }
    if options.search_common_dirs {
        emitter.emit(ScanProgressEvent {
            step: "common_done".into(),
            current: 2,
            total: 4,
            message: Some("Common directories scan done".into()),
        });
    }

    // Step 3: 发送匹配存档进度
    emitter.emit(ScanProgressEvent {
        step: "match_saves".into(),
        current: 3,
        total: 4,
        message: Some(t!("backend.scan.match_saves").to_string()),
    });

    // 执行存档匹配（Windows 基础版）
    let mut matches: Vec<SaveMatchResult> = Vec::new();
    let t_match = Instant::now();
    #[cfg(target_os = "windows")]
    for d in &detected {
        if let Some(ref install) = d.install_path {
            let ms = match_save_paths(&d.info, install)
                .await
                .map_err(|e| e.to_string())?;
            matches.extend(ms);
        }
    }
    info!(target:"rgsm::game_scan", "Matched save paths: {}, elapsed: {:?}", matches.len(), t_match.elapsed());

    let result = ScanResult {
        detected,
        matches,
        errors: Vec::new(),
    };

    // Step 4: 发送完成进度
    emitter.emit(ScanProgressEvent {
        step: "done".into(),
        current: 4,
        total: 4,
        message: Some(t!("backend.scan.done").to_string()),
    });

    info!(target:"rgsm::game_scan", "Scan finished, total elapsed: {:?}", t_total.elapsed());
    Ok(result)
}

/// 查询 PCGamingWiki 索引中的游戏信息（名称或别名匹配）
///
/// - 输入：`name` 为待查询的游戏名称或别名，`AppHandle` 用于解析资源路径
/// - 输出：匹配到的 `GameInfo`，无匹配时返回 `None`
/// - 错误：资源读取或解析失败返回错误信息字符串（已转换为友好可读）
#[tauri::command]
#[specta::specta]
pub async fn pcgw_query(app: AppHandle, name: String) -> Result<Option<super::types::GameInfo>, String> {
    let index = load_pcgw_index(&app).await.map_err(|e| e.to_string())?;
    Ok(find_by_name(&index, &name).cloned())
}

/// 完整查询 PCGamingWiki 索引（支持模糊、平台过滤与结果上限）
///
/// - 输入：`name` 查询关键字（名称或别名），`options` 查询选项
/// - 行为：按以下优先级计算评分并排序：
///   1. 主名称完全匹配：score=1.0，matched_by="name"
///   2. 别名完全匹配：score=0.95，matched_by="alias"
///   3. 模糊匹配（包含）：name 包含则 score≈0.75~1.0，alias 包含则 score≈0.7~1.0，matched_by="fuzzy"
/// - 过滤：若设置 `platform`，仅保留有保存规则包含该平台的条目
/// - 限制：返回不超过 `limit` 个结果（默认 20）
#[tauri::command]
#[specta::specta]
pub async fn pcgw_search(app: AppHandle, name: String, options: PcgwQueryOptions) -> Result<Vec<PcgwQueryItem>, String> {
    let index = load_pcgw_index(&app).await.map_err(|e| e.to_string())?;
    let q = name.trim().to_lowercase();
    let limit = options.limit.unwrap_or(20);

    // 平台过滤器
    let platform_ok = |gi: &super::types::GameInfo| -> bool {
        if let Some(ref p) = options.platform {
            let pl = p.to_lowercase();
            return gi.save_rules.iter().any(|r| r.platforms.iter().any(|rp| rp.to_lowercase() == pl));
        }
        true
    };

    // 评分计算
    let mut items: Vec<PcgwQueryItem> = Vec::new();
    for gi in index.iter() {
        if !platform_ok(gi) { continue; }
        let name_l = gi.name.to_lowercase();

        // 完全匹配（名称）
        if name_l == q {
            items.push(PcgwQueryItem { info: gi.clone(), score: 1.0, matched_by: "name".into() });
            continue;
        }

        // 完全匹配（别名）
        if gi.aliases.iter().any(|a| a.to_lowercase() == q) {
            items.push(PcgwQueryItem { info: gi.clone(), score: 0.95, matched_by: "alias".into() });
            continue;
        }

        // 模糊匹配（包含）
        if options.fuzzy {
            let mut pushed = false;
            if name_l.contains(&q) {
                // 简单长度比例评分（0.75~1.0）
                let ratio = (q.len() as f32) / (gi.name.len().max(1) as f32);
                let score = 0.75 + 0.25 * ratio.min(1.0);
                items.push(PcgwQueryItem { info: gi.clone(), score, matched_by: "fuzzy".into() });
                pushed = true;
            }
            if !pushed {
                for a in gi.aliases.iter() {
                    let al = a.to_lowercase();
                    if al.contains(&q) {
                        let ratio = (q.len() as f32) / (a.len().max(1) as f32);
                        let score = 0.70 + 0.30 * ratio.min(1.0);
                        items.push(PcgwQueryItem { info: gi.clone(), score, matched_by: "fuzzy".into() });
                        break;
                    }
                }
            }
        }
    }

    // 排序并截断
    items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    if items.len() > limit { items.truncate(limit); }
    Ok(items)
}

/// 为已检测到的游戏生成 SaveUnit 列表（带设备映射）
///
/// - 输入：`game_info`（PCGW 索引中的游戏信息）、`install_path`（解析出的安装目录字符串）
/// - 行为：调用平台实现的 `generate_save_units`，只返回存在的文件/文件夹并映射到当前设备
/// - 输出：`SaveUnit` 列表，供前端“一键填充/添加”使用
#[tauri::command]
#[specta::specta]
pub async fn generate_save_units_for_game(
    game_info: super::types::GameInfo,
    install_path: String,
) -> Result<Vec<crate::backup::SaveUnit>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::path::Path;
        generate_save_units(&game_info, Path::new(&install_path))
            .await
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

/// 刷新 PCGW 索引（返回版本与条目数量）
///
/// - 行为：首先尝试从远端拉取并缓存索引；失败则回退读取打包资源
/// - 返回：索引版本与条目数量，便于前端显示状态
#[tauri::command]
#[specta::specta]
pub async fn pcgw_refresh_index(app: AppHandle) -> Result<PcgwIndexMeta, String> {
    match update_pcgw_index_remote(&app).await {
        Ok(meta) => Ok(meta),
        Err(e) => Err(format!("{}", e)),
    }
}

/// 从本地文件导入 PCGW 索引（覆盖缓存并返回元信息）
#[tauri::command]
#[specta::specta]
pub async fn pcgw_import_index_from_file(app: AppHandle, file_path: String) -> Result<PcgwIndexMeta, String> {
    let path = std::path::PathBuf::from(file_path);
    import_pcgw_index_from_file(&app, &path)
        .await
        .map_err(|e| e.to_string())
}

/// 从SQLite数据库导入 PCGW 索引（例如 Game-Save-Manager 的 database.db）
#[tauri::command]
#[specta::specta]
pub async fn pcgw_import_index_from_sqlite(app: AppHandle, file_path: String) -> Result<PcgwIndexMeta, String> {
    let path = std::path::PathBuf::from(file_path);
    import_pcgw_index_from_sqlite(&app, &path)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{DetectedGame, GameInfo, DetectionSource};

    /// 测试：将检测到的游戏集合与 PCGW 索引合并，规则被正确注入
    #[test]
    fn enrich_with_pcgw_injects_rules() {
        let detected = vec![DetectedGame {
            info: GameInfo {
                name: "Stardew Valley".into(),
                aliases: vec!["SV".into()],
                pcgw_id: None,
                install_rules: Vec::new(),
                save_rules: Vec::new(),
            },
            install_path: None,
            source: DetectionSource::CommonDir,
        }];

        let index = vec![GameInfo {
            name: "Stardew Valley".into(),
            aliases: vec!["SV".into()],
            pcgw_id: Some("stardew-valley".into()),
            install_rules: Vec::new(),
            save_rules: vec![
                super::super::types::SavePathRule {
                    id: "save-stardew-default".into(),
                    description: Some("AppData saves".into()),
                    path_template: "<winAppData>/StardewValley/Saves".into(),
                    requires: None,
                    platforms: vec!["windows".into()],
                    confidence: 0.95,
                }
            ],
        }];

        let merged = enrich_with_pcgw(detected, &index);
        assert_eq!(merged.len(), 1);
        let info = &merged[0].info;
        assert_eq!(info.pcgw_id.as_deref(), Some("stardew-valley"));
        assert_eq!(info.save_rules.len(), 1);
        assert_eq!(info.save_rules[0].path_template, "<winAppData>/StardewValley/Saves");
    }

    /// 测试：规范化+模糊匹配可将目录名 "BlackMythWukong" 匹配到 "Black Myth: Wukong"
    #[test]
    fn enrich_with_pcgw_fuzzy_normalize() {
        let detected = vec![DetectedGame {
            info: GameInfo {
                name: "BlackMythWukong".into(),
                aliases: Vec::new(),
                pcgw_id: None,
                install_rules: Vec::new(),
                save_rules: Vec::new(),
            },
            install_path: None,
            source: DetectionSource::CommonDir,
        }];

        let index = vec![GameInfo {
            name: "Black Myth: Wukong".into(),
            aliases: vec!["Black Myth Wukong".into()],
            pcgw_id: Some("black-myth-wukong".into()),
            install_rules: Vec::new(),
            save_rules: vec![
                super::super::types::SavePathRule {
                    id: "save-bmw-default".into(),
                    description: Some("AppData saves".into()),
                    path_template: "<winAppData>/BlackMythWukong/Saved/SaveGames".into(),
                    requires: None,
                    platforms: vec!["windows".into()],
                    confidence: 0.90,
                }
            ],
        }];

        let merged = enrich_with_pcgw(detected, &index);
        assert_eq!(merged.len(), 1);
        let info = &merged[0].info;
        assert_eq!(info.pcgw_id.as_deref(), Some("black-myth-wukong"));
        assert_eq!(info.save_rules.len(), 1);
        assert!(info.save_rules[0].path_template.contains("BlackMythWukong"));
    }
}
/// 将平台检测到的游戏集合与 PCGW 索引进行合并，丰富规则信息
///
/// - 输入：检测结果与 PCGW 索引切片
/// - 输出：带有 `install_rules` / `save_rules` 等信息的检测结果集合
/// - 行为：按名称或别名进行大小写不敏感匹配；若匹配成功，保留原安装路径与来源，替换为索引中的 `GameInfo`
///
/// 注意：该函数不会修改 `install_path` 与 `source` 字段，仅替换 `info`
fn enrich_with_pcgw(mut detected: Vec<DetectedGame>, index: &[super::types::GameInfo]) -> Vec<DetectedGame> {
    // 辅助：规范化字符串，仅保留 ASCII 字母数字并转小写
    fn normalize_key(s: &str) -> String {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
    }

    // 辅助：在索引中进行模糊查找（包含与规范化对比），返回最优候选
    fn find_by_name_fuzzy<'a>(index: &'a [super::types::GameInfo], name: &str) -> Option<&'a super::types::GameInfo> {
        let q_raw = name.trim().to_lowercase();
        let q_norm = normalize_key(&q_raw);

        let mut best: Option<(&super::types::GameInfo, f32)> = None;
        for gi in index.iter() {
            // 主名称优先
            let name_l = gi.name.to_lowercase();
            if name_l == q_raw {
                return Some(gi);
            }
            if gi.aliases.iter().any(|a| a.to_lowercase() == q_raw) {
                return Some(gi);
            }

            // 规范化后比较，处理去空格/去标点的目录名（如 BlackMythWukong vs Black Myth: Wukong）
            let gi_norm = normalize_key(&gi.name);
            if !gi_norm.is_empty() && !q_norm.is_empty() {
                if gi_norm == q_norm {
                    return Some(gi);
                }
                let contains = gi_norm.contains(&q_norm) || q_norm.contains(&gi_norm);
                if contains {
                    // 简单长度比例作为评分，越接近越高
                    let shorter = gi_norm.len().min(q_norm.len()) as f32;
                    let longer = gi_norm.len().max(q_norm.len()) as f32;
                    let score = 0.80 + 0.20 * (shorter / longer);
                    match best {
                        Some((_, s)) if s >= score => {}
                        _ => best = Some((gi, score)),
                    }
                }
            }

            // 别名的规范化包含匹配
            for a in gi.aliases.iter() {
                let al = a.to_lowercase();
                let an = normalize_key(&al);
                if an.is_empty() || q_norm.is_empty() { continue; }
                if an == q_norm {
                    return Some(gi);
                }
                if an.contains(&q_norm) || q_norm.contains(&an) {
                    let shorter = an.len().min(q_norm.len()) as f32;
                    let longer = an.len().max(q_norm.len()) as f32;
                    let score = 0.75 + 0.25 * (shorter / longer);
                    match best {
                        Some((_, s)) if s >= score => {}
                        _ => best = Some((gi, score)),
                    }
                    break;
                }
            }
        }
        best.map(|(gi, _)| gi)
    }

    for d in detected.iter_mut() {
        let name = d.info.name.clone();
        // 1) 优先精确匹配（名称或别名）
        if let Some(gi) = find_by_name(index, &name) {
            d.info = gi.clone();
        } else {
            // 2) 模糊匹配（包含与规范化对比）
            if let Some(gi) = find_by_name_fuzzy(index, &name) {
                d.info = gi.clone();
            } else if let Some(alias) = d.info.aliases.first() {
                // 3) 兜底：尝试别名精确匹配（若后续补充了别名）
                if let Some(gi) = find_by_name(index, alias) {
                    d.info = gi.clone();
                }
            }
        }
    }
    detected
}