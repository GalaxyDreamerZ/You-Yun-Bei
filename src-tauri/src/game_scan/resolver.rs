use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::env;

use super::types::SavePathRule;
use crate::config::Config;
use crate::path_resolver;

/// 变量解析环境
#[derive(Debug, Clone)]
pub struct ResolverEnv {
    /// 变量映射，如 `<home>`, `<winAppData>` 等
    pub variables: HashMap<String, PathBuf>,
}

/// 构建默认解析环境
///
/// - 输入：全局配置 `Config`
/// - 输出：包含常用变量（如 `<home>`, `<documents>`, `<winAppData>` 等）的 `ResolverEnv`
/// - 行为：从系统环境变量与常规路径推断变量值，不进行 IO 检查
pub fn default_env(_config: &Config) -> ResolverEnv {
    let mut vars: HashMap<String, PathBuf> = HashMap::new();

    // 推断 home 目录（跨平台）
    let home = if cfg!(target_os = "windows") {
        env::var("USERPROFILE").ok().map(PathBuf::from)
    } else {
        env::var("HOME").ok().map(PathBuf::from)
    };
    if let Some(home_dir) = home {
        vars.insert("home".into(), home_dir.clone());

        // Documents 目录（尽量通用地推断，不保证存在性）
        let docs = if cfg!(target_os = "windows") {
            home_dir.join("Documents")
        } else {
            home_dir.join("Documents")
        };
        vars.insert("documents".into(), docs);
    }

    // Windows 专有：AppData 路径
    if cfg!(target_os = "windows") {
        if let Ok(appdata) = env::var("APPDATA") {
            vars.insert("winAppData".into(), PathBuf::from(appdata));
        }
        if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
            vars.insert("winLocalAppData".into(), PathBuf::from(local_appdata));
        }
    }

    ResolverEnv { variables: vars }
}

/// 应用环境变量映射，将模板中的 `<var>` 替换为具体路径字符串
///
/// - 输入：原始模板字符串、解析环境
/// - 输出：替换变量后的模板字符串（仍可能包含未识别变量）
fn apply_env_variables(template: &str, env: &ResolverEnv) -> String {
    let mut out = template.to_string();
    for (key, path) in &env.variables {
        let token = format!("<{}>", key);
        if out.contains(&token) {
            let value = path.to_string_lossy();
            out = out.replace(&token, &value);
        }
    }
    out
}

/// 解析路径模板为绝对路径（使用默认配置，避免测试环境 IO 依赖）
///
/// - 输入：规则模板字符串（可能包含变量）
/// - 输出：解析后的绝对路径
/// - 行为：先用 `ResolverEnv.variables` 进行基本替换，再调用 `path_resolver::resolve_path`
pub fn resolve_template(template: &str, _env: &ResolverEnv) -> Result<PathBuf> {
    #[cfg(test)]
    let config = crate::config::Config::default();
    #[cfg(not(test))]
    let config = crate::config::get_config()?;
    let templ = apply_env_variables(template, _env);
    let p = path_resolver::resolve_path(&templ, None, &config)?;
    Ok(p)
}

/// 将保存规则解析为实际路径集合
///
/// - 输入：`SavePathRule` 与解析环境
/// - 输出：解析出的路径集合；后续可扩展到多模板与平台过滤
pub fn resolve_save_rule(rule: &SavePathRule, env: &ResolverEnv) -> Result<Vec<PathBuf>> {
    let p = resolve_template(&rule.path_template, env)?;
    Ok(vec![p])
}
