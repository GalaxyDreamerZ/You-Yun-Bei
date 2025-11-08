use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

/// 游戏基础信息与路径规则集合
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GameInfo {
    /// 游戏显示名称
    pub name: String,
    /// 可选的别名（匹配安装来源/进程名时使用）
    pub aliases: Vec<String>,
    /// PCGamingWiki 对应的条目 ID（用于外部索引）
    pub pcgw_id: Option<String>,
    /// 安装路径匹配规则集合
    pub install_rules: Vec<InstallPathRule>,
    /// 存档路径匹配规则集合
    pub save_rules: Vec<SavePathRule>,
}

/// PCGW 查询选项
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PcgwQueryOptions {
    /// 是否启用模糊匹配（令包含与前后缀匹配生效）
    pub fuzzy: bool,
    /// 可选的平台过滤（例如 `windows`、`macos`、`linux`），为空则不筛选
    pub platform: Option<String>,
    /// 返回条目上限，缺省为 20
    pub limit: Option<usize>,
}

/// PCGW 查询结果项（包含评分）
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PcgwQueryItem {
    /// 命中的游戏信息
    pub info: GameInfo,
    /// 匹配评分（0.0~1.0），用于排序显示
    pub score: f32,
    /// 命中依据（`name`/`alias`/`fuzzy`），便于前端标注
    pub matched_by: String,
}

/// PCGW 索引元信息（用于刷新与状态显示）
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PcgwIndexMeta {
    /// 索引版本号（若不可用则为空）
    pub version: Option<String>,
    /// 游戏条目数量
    pub count: usize,
}

/// 安装路径匹配规则
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct InstallPathRule {
    /// 规则标识符（便于调试与日志）
    pub id: String,
    /// 规则简短描述
    pub description: Option<String>,
    /// 通用的路径模式（支持变量，如 `<home>`, `<winAppData>` 等）
    pub patterns: Vec<String>,
    /// 可选的注册表键（Windows），用于提升匹配可靠度
    pub registry_keys: Option<Vec<String>>, // Windows only
}

/// 存档路径匹配规则
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SavePathRule {
    /// 规则标识符（便于调试与日志）
    pub id: String,
    /// 规则简短描述
    pub description: Option<String>,
    /// 路径模板（支持变量与占位符）
    pub path_template: String,
    /// 需要的前置条件（如必须存在安装目录）
    pub requires: Option<Vec<String>>,
    /// 支持的平台标识（如 `windows`, `macos`, `linux`）
    pub platforms: Vec<String>,
    /// 规则的可信度（0.0~1.0），用于结果排序
    pub confidence: f32,
}

/// 扫描选项
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanOptions {
    /// 平台标识（如 `windows`、`macos`、`linux`）
    pub platform: String,
    /// 是否扫描 Steam 安装目录
    pub search_steam: bool,
    /// 是否扫描 Epic 安装目录
    pub search_epic: bool,
    /// 是否扫描 Origin/EA 安装目录
    pub search_origin: bool,
    /// 是否读取注册表提升检测（Windows）
    pub search_registry: bool,
    /// 是否扫描常见安装路径（如 `Program Files` 等）
    pub search_common_dirs: bool,
    /// 是否通过当前运行进程进行辅助匹配
    pub search_processes: bool,
}

/// 安装来源，用于标注检测到的依据
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub enum DetectionSource {
    Steam,
    Epic,
    Origin,
    Registry,
    CommonDir,
    Process,
    Manual,
}

/// 已检测到的游戏条目
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DetectedGame {
    /// 游戏基础信息
    pub info: GameInfo,
    /// 解析出的安装路径（若无法解析则为 None）
    pub install_path: Option<PathBuf>,
    /// 检测来源
    pub source: DetectionSource,
}

/// 存档路径匹配结果
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SaveMatchResult {
    /// 对应的规则 ID
    pub rule_id: String,
    /// 解析出的实际路径
    pub resolved_path: PathBuf,
    /// 路径是否存在（快速校验）
    pub exists: bool,
    /// 可信度（综合规则与存在性校验）
    pub confidence: f32,
}

/// 扫描进度事件载荷（用于前端进度显示）
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanProgressEvent {
    /// 当前步骤名称（如 `index_load`, `detect_games`, `match_saves`）
    pub step: String,
    /// 当前进度值
    pub current: u32,
    /// 总进度值
    pub total: u32,
    /// 可选的附加信息
    pub message: Option<String>,
}

/// 完整扫描结果
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ScanResult {
    /// 检测到的游戏列表
    pub detected: Vec<DetectedGame>,
    /// 匹配到的存档路径结果（聚合）
    pub matches: Vec<SaveMatchResult>,
    /// 错误消息（若有）
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    /// 测试 ScanOptions 的序列化与反序列化是否正确
    #[test]
    fn serde_roundtrip_scan_options() {
        let opts = ScanOptions {
            platform: "windows".into(),
            search_steam: true,
            search_epic: false,
            search_origin: false,
            search_registry: true,
            search_common_dirs: true,
            search_processes: false,
        };
        let s = serde_json::to_string(&opts).expect("serialize ScanOptions");
        let d: ScanOptions = serde_json::from_str(&s).expect("deserialize ScanOptions");
        assert_eq!(d.platform, "windows");
        assert!(d.search_steam);
        assert!(d.search_registry);
        assert!(d.search_common_dirs);
        assert!(!d.search_epic);
        assert!(!d.search_origin);
        assert!(!d.search_processes);
    }

    /// 测试 GameInfo 的序列化与反序列化是否正确
    #[test]
    fn serde_roundtrip_game_info() {
        let gi = GameInfo {
            name: "Example Game".into(),
            aliases: vec!["EG".into()],
            pcgw_id: Some("pcgw-123".into()),
            install_rules: vec![InstallPathRule {
                id: "rule-install-1".into(),
                description: Some("Steam default".into()),
                patterns: vec!["<home>/Games/Example".into()],
                registry_keys: Some(vec!["HKLM\\Software\\Example".into()]),
            }],
            save_rules: vec![SavePathRule {
                id: "rule-save-1".into(),
                description: Some("My Games default".into()),
                path_template: "<home>/Documents/My Games/Example".into(),
                requires: None,
                platforms: vec!["windows".into()],
                confidence: 0.9,
            }],
        };
        let s = serde_json::to_string(&gi).expect("serialize GameInfo");
        let d: GameInfo = serde_json::from_str(&s).expect("deserialize GameInfo");
        assert_eq!(d.name, "Example Game");
        assert_eq!(d.aliases, vec!["EG"]);
        assert_eq!(d.pcgw_id.as_deref(), Some("pcgw-123"));
        assert_eq!(d.install_rules.len(), 1);
        assert_eq!(d.save_rules.len(), 1);
    }

    /// 测试 SaveMatchResult 的序列化与反序列化是否正确
    #[test]
    fn serde_roundtrip_save_match_result() {
        let r = SaveMatchResult {
            rule_id: "rule-save-1".into(),
            resolved_path: PathBuf::from("C:/Example/Save"),
            exists: false,
            confidence: 0.75,
        };
        let s = serde_json::to_string(&r).expect("serialize SaveMatchResult");
        let d: SaveMatchResult = serde_json::from_str(&s).expect("deserialize SaveMatchResult");
        assert_eq!(d.rule_id, "rule-save-1");
        assert_eq!(d.resolved_path, PathBuf::from("C:/Example/Save"));
        assert!(!d.exists);
        assert!((d.confidence - 0.75).abs() < f32::EPSILON);
    }
}