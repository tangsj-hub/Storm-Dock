//! Grok Bot 官方模板分享链接（https://x.ai/bot/<id>）的本地收藏。
//! 数据与账号无关，存放在 app_kv 中；元信息来自分享页的 og:title / og:description。

use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::http::{Body, Budget, Call, Client, Retry};

pub(crate) const APP_KV_KEY: &str = "grok_bot_templates";
/// 已经播种过的内置模版 ID（JSON 字符串数组）；删除后依然保留，避免重新加回。
pub(crate) const BUILTIN_SEEDED_KV_KEY: &str = "grok_bot_templates_builtin_seeded";
const FETCH_BUDGET: Duration = Duration::from_secs(15);
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BotTemplate {
    pub(crate) url: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
    /// 用户备注，仅本地可见。
    #[serde(default)]
    pub(crate) note: Option<String>,
    /// 切换账号后是否默认勾选自动导入。
    #[serde(default)]
    pub(crate) auto_import: bool,
    pub(crate) created_at: u64,
    /// 内置模版的固定 ID；用户自己添加或导入的为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) builtin_id: Option<String>,
    /// 播种/上次更新时内置内容（name/description/note）的指纹，用于判断用户是否改过。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) builtin_hash: Option<String>,
}

/// 随应用发布的内置模版。修改内容即视为"更新"，未被用户改动的副本会被覆盖。
pub(crate) struct BuiltinTemplate {
    pub(crate) id: &'static str,
    pub(crate) url: &'static str,
    pub(crate) name: &'static str,
    pub(crate) description: Option<&'static str>,
    pub(crate) note: Option<&'static str>,
}

pub(crate) const BUILTIN_TEMPLATES: &[BuiltinTemplate] = &[BuiltinTemplate {
    id: "xiao-c",
    url: "https://x.ai/bot/hXd0y-LKCK729av9pzg_X",
    name: "小C",
    description: Some("一个直接在你本地项目里写代码、修 bug 的编程助手，方向性问题会先问你，小事自己拿主意。"),
    note: None,
}];

/// FNV-1a 64 指纹，跨版本稳定。
fn content_hash(name: &str, description: Option<&str>, note: Option<&str>) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for part in [name, "\u{1f}", description.unwrap_or(""), "\u{1f}", note.unwrap_or("")] {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

fn template_hash(item: &BotTemplate) -> String {
    content_hash(&item.name, item.description.as_deref(), item.note.as_deref())
}

fn same_url(a: &str, normalized: &str) -> bool {
    normalize_template_url(a).map(|url| url == normalized).unwrap_or(a == normalized)
}

/// 播种从未加过的内置模版，并更新用户未改动过的内置副本。返回是否有变化。
/// 已播种（包括被用户删除）的 ID 不会再加回。
pub(crate) fn sync_builtins(list: &mut Vec<BotTemplate>, seeded: &mut Vec<String>, builtins: &[BuiltinTemplate], now: u64) -> bool {
    let mut changed = false;
    for builtin in builtins {
        let hash = content_hash(builtin.name, builtin.description, builtin.note);
        let builtin_url = normalize_template_url(builtin.url).unwrap_or_else(|_| builtin.url.to_string());
        let newly = !seeded.iter().any(|id| id == builtin.id);
        if newly {
            seeded.push(builtin.id.to_string());
            changed = true;
        }
        // 认领：同链接且未标记的记录（包括旧逻辑下已播种的用户记录）。
        if !list.iter().any(|item| item.builtin_id.as_deref() == Some(builtin.id)) {
            if let Some(item) = list.iter_mut().find(|item| item.builtin_id.is_none() && same_url(&item.url, &builtin_url)) {
                item.builtin_id = Some(builtin.id.to_string());
                // 内容与内置一致 => 视为未修改（两种情况下指纹都等于内置内容指纹）；否则视为用户修改，后续更新不会覆盖。
                item.builtin_hash = Some(hash.clone());
                changed = true;
                continue;
            }
        }
        if newly {
            if list.iter().any(|item| same_url(&item.url, &builtin_url)) {
                continue;
            }
            list.push(BotTemplate {
                url: builtin_url,
                name: builtin.name.to_string(),
                description: builtin.description.map(str::to_string),
                note: builtin.note.map(str::to_string),
                auto_import: false,
                created_at: now,
                builtin_id: Some(builtin.id.to_string()),
                builtin_hash: Some(hash),
            });
            continue;
        }
        let Some(item) = list.iter_mut().find(|item| item.builtin_id.as_deref() == Some(builtin.id)) else { continue };
        if item.builtin_hash.as_deref() == Some(hash.as_str()) {
            continue;
        }
        if item.builtin_hash.as_deref() == Some(template_hash(item).as_str()) {
            item.name = builtin.name.to_string();
            item.description = builtin.description.map(str::to_string);
            item.note = builtin.note.map(str::to_string);
            item.builtin_hash = Some(hash);
            changed = true;
        }
    }
    changed
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BotTemplatePatch {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) note: Option<String>,
    #[serde(default)]
    pub(crate) auto_import: bool,
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
}

/// 按链接更新一条模板（链接本身不可修改）。
pub(crate) fn update_template(mut list: Vec<BotTemplate>, url: &str, patch: BotTemplatePatch) -> Result<Vec<BotTemplate>> {
    let name = patch.name.trim();
    if name.is_empty() {
        return Err(AppError::Message("请填写模板名称".into()));
    }
    let item = list
        .iter_mut()
        .find(|item| item.url == url)
        .ok_or_else(|| AppError::Message("模板不存在或已被删除".into()))?;
    item.name = name.to_string();
    item.description = clean_optional(patch.description);
    item.note = clean_optional(patch.note);
    item.auto_import = patch.auto_import;
    Ok(list)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BotTemplateMeta {
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
}

/// 规范化并校验分享链接；只接受 https://x.ai/bot/<id>（允许 www. 前缀、尾部斜杠、查询串）。
pub(crate) fn normalize_template_url(raw: &str) -> Result<String> {
    let invalid = || AppError::Message("请输入 Grok Bot 官方模板链接，例如 https://x.ai/bot/xxxx".into());
    let parsed = reqwest::Url::parse(raw.trim()).map_err(|_| invalid())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(invalid());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "x.ai" && host != "www.x.ai" {
        return Err(invalid());
    }
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(|items| items.filter(|item| !item.is_empty()).collect())
        .unwrap_or_default();
    match segments.as_slice() {
        ["bot", id]
            if id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
        {
            Ok(format!("https://x.ai/bot/{id}"))
        }
        _ => Err(invalid()),
    }
}

fn decode_entities(text: &str) -> String {
    let re = Regex::new(r"&(#x[0-9a-fA-F]+|#[0-9]+|amp|lt|gt|quot|apos|nbsp);").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let entity = &caps[1];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32),
            _ => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
        };
        decoded.map(String::from).unwrap_or_else(|| caps[0].to_string())
    })
    .into_owned()
}

/// 读取 <meta property="..." content="...">，属性顺序和引号不限。
pub(crate) fn og_meta(html: &str, property: &str) -> Option<String> {
    let tag_re = Regex::new(r"(?is)<meta\b[^>]*>").unwrap();
    let attr_re = Regex::new(r#"(?is)([a-zA-Z:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap();
    for tag in tag_re.find_iter(html) {
        let mut key = None;
        let mut content = None;
        for caps in attr_re.captures_iter(tag.as_str()) {
            let name = caps[1].to_ascii_lowercase();
            let value = caps.get(2).or_else(|| caps.get(3)).map(|m| m.as_str()).unwrap_or("");
            match name.as_str() {
                "property" | "name" => key = Some(value.to_string()),
                "content" => content = Some(value.to_string()),
                _ => {}
            }
        }
        if key.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(property)) {
            let value = decode_entities(content?.trim()).trim().to_string();
            return (!value.is_empty()).then_some(value);
        }
    }
    None
}

/// 去掉标题末尾的 " by <作者>"。
pub(crate) fn strip_author_suffix(title: &str) -> String {
    let trimmed = title.trim();
    if let Some(index) = trimmed.rfind(" by ") {
        let head = trimmed[..index].trim();
        if !head.is_empty() && !trimmed[index + 4..].trim().is_empty() {
            return head.to_string();
        }
    }
    trimmed.to_string()
}

pub(crate) fn parse_template_meta(html: &str) -> BotTemplateMeta {
    BotTemplateMeta {
        name: og_meta(html, "og:title")
            .map(|title| strip_author_suffix(&title))
            .filter(|name| !name.is_empty()),
        description: og_meta(html, "og:description"),
    }
}

pub(crate) fn fetch_template_meta(url: &str) -> Result<BotTemplateMeta> {
    let url = normalize_template_url(url)?;
    let response = Client::shared().send(
        &Call {
            method: reqwest::Method::GET,
            url,
            headers: vec![
                ("User-Agent".into(), BROWSER_UA.into()),
                ("Accept".into(), "text/html,application/xhtml+xml".into()),
            ],
            query: Vec::new(),
            body: Body::Empty,
        },
        &Budget::new(FETCH_BUDGET),
        Retry::Transient,
    )?;
    if !(200..300).contains(&response.status) {
        return Err(AppError::Message(format!("读取模板页面失败（HTTP {}）", response.status)));
    }
    let meta = parse_template_meta(&response.text());
    if meta.name.is_none() {
        return Err(AppError::Message("没有在模板页面中找到名称".into()));
    }
    Ok(meta)
}

pub(crate) fn parse_templates(json: Option<&str>) -> Vec<BotTemplate> {
    json.and_then(|value| serde_json::from_str(value).ok()).unwrap_or_default()
}

/// 新增或替换（按链接去重）后返回新列表；新条目排在最前。
pub(crate) fn upsert_template(mut list: Vec<BotTemplate>, template: BotTemplate) -> (Vec<BotTemplate>, bool) {
    if list.iter().any(|item| item.url == template.url) {
        return (list, false);
    }
    list.insert(0, template);
    (list, true)
}

/// 按给定链接顺序重排；未列出的条目保持原相对顺序排在末尾，未知链接忽略。
pub(crate) fn reorder_templates(list: Vec<BotTemplate>, urls: &[String]) -> Vec<BotTemplate> {
    let mut rest = list;
    let mut ordered = Vec::with_capacity(rest.len());
    for url in urls {
        if let Some(index) = rest.iter().position(|item| &item.url == url) {
            ordered.push(rest.remove(index));
        }
    }
    ordered.extend(rest);
    ordered
}

/// 客户端导入深链。格式取自分享页「在 Grok Bot 中打开」按钮的 href / RSC addHref，
/// 并与 Grok Bot.app 注册的 URL scheme（grokbot、sand）一致。
pub(crate) fn client_deep_link(url: &str) -> Result<String> {
    let normalized = normalize_template_url(url)?;
    let id = normalized.rsplit('/').next().unwrap_or_default();
    Ok(format!("grokbot://app/v1/bot-template?id={id}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OpenedIn {
    Client,
    Web,
}

/// 优先用深链交给 Grok Bot 客户端导入；失败时回退到网页链接。
pub(crate) fn open_in_client(url: &str) -> Result<OpenedIn> {
    let web = normalize_template_url(url)?;
    let deep = client_deep_link(&web)?;
    match crate::cursor::oauth::open_browser(&deep) {
        Ok(()) => Ok(OpenedIn::Client),
        Err(error) => {
            eprintln!("bot template: deep link failed ({error}), falling back to web");
            crate::cursor::oauth::open_browser(&web).map(|_| OpenedIn::Web)
        }
    }
}

/// 切换账号后需要自动导入的模板，按列表顺序。
pub(crate) fn auto_import_items(list: &[BotTemplate]) -> Vec<crate::bot_import_queue::QueueItem> {
    list.iter()
        .filter(|item| item.auto_import)
        .map(|item| crate::bot_import_queue::QueueItem { url: item.url.clone(), name: item.name.clone() })
        .collect()
}

// ---------- App-format export / import ----------

pub(crate) const EXPORT_FORMAT: &str = "storm-dock/bot-templates";
pub(crate) const EXPORT_VERSION: u32 = 1;
const IMPORT_MAX_BYTES: usize = 1024 * 1024;
const IMPORT_MAX_ITEMS: usize = 500;
const TEXT_MAX_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExportItem {
    pub(crate) url: String,
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
    #[serde(default)]
    pub(crate) auto_import: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExportFile {
    pub(crate) format: String,
    pub(crate) version: u32,
    pub(crate) exported_at: String,
    pub(crate) templates: Vec<ExportItem>,
}

fn now_rfc3339() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// Only url / name / description / note / autoImport — never accounts or secrets.
/// `urls = None` exports everything; otherwise the selection in list order.
pub(crate) fn export_file(list: &[BotTemplate], urls: Option<&[String]>, exported_at: String) -> ExportFile {
    ExportFile {
        format: EXPORT_FORMAT.into(),
        version: EXPORT_VERSION,
        exported_at,
        templates: list
            .iter()
            .filter(|item| item.builtin_id.is_none())
            .filter(|item| urls.is_none_or(|urls| urls.contains(&item.url)))
            .map(|item| ExportItem {
                url: item.url.clone(),
                name: item.name.clone(),
                description: item.description.clone(),
                note: item.note.clone(),
                auto_import: item.auto_import,
            })
            .collect(),
    }
}

pub(crate) fn export_json(list: &[BotTemplate], urls: Option<&[String]>) -> Result<String> {
    Ok(serde_json::to_string_pretty(&export_file(list, urls, now_rfc3339()))?)
}

fn limit_text(value: Option<String>) -> Option<String> {
    clean_optional(value).map(|text| text.chars().take(TEXT_MAX_CHARS).collect())
}

/// Strict parse: exact format id, supported version, x.ai/bot links, non-empty names.
/// Imported templates always start with auto-import OFF (it opens deep links after a switch).
pub(crate) fn parse_import(json: &str) -> Result<Vec<BotTemplate>> {
    let invalid = |detail: &str| AppError::Message(format!("不是有效的 Storm Dock Bot模版文件：{detail}"));
    if json.len() > IMPORT_MAX_BYTES {
        return Err(invalid("文件过大"));
    }
    let value: serde_json::Value = serde_json::from_str(json.trim()).map_err(|_| invalid("JSON 格式错误"))?;
    if value.get("format").and_then(serde_json::Value::as_str) != Some(EXPORT_FORMAT) {
        return Err(invalid("format 不匹配"));
    }
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(v) if v == u64::from(EXPORT_VERSION) => {}
        Some(v) => return Err(invalid(&format!("不支持的版本 {v}"))),
        None => return Err(invalid("缺少 version")),
    }
    let file: ExportFile = serde_json::from_value(value).map_err(|error| invalid(&error.to_string()))?;
    if file.templates.is_empty() {
        return Err(invalid("没有模版"));
    }
    if file.templates.len() > IMPORT_MAX_ITEMS {
        return Err(invalid("模版数量过多"));
    }
    let created_at = crate::models::now();
    file.templates
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let url = normalize_template_url(&item.url).map_err(|_| invalid(&format!("第 {} 个链接无效", index + 1)))?;
            let name: String = item.name.trim().chars().take(TEXT_MAX_CHARS).collect();
            if name.is_empty() {
                return Err(invalid(&format!("第 {} 个模版缺少名称", index + 1)));
            }
            Ok(BotTemplate {
                url,
                name,
                description: limit_text(item.description),
                note: limit_text(item.note),
                auto_import: false,
                created_at,
                builtin_id: None,
                builtin_hash: None,
            })
        })
        .collect()
}

/// Append new links in file order; existing links (and repeats inside the file) are skipped.
pub(crate) fn merge_import(mut list: Vec<BotTemplate>, incoming: Vec<BotTemplate>) -> (Vec<BotTemplate>, usize, usize) {
    let (mut added, mut skipped) = (0, 0);
    for item in incoming {
        if list.iter().any(|existing| existing.url == item.url) {
            skipped += 1;
        } else {
            list.push(item);
            added += 1;
        }
    }
    (list, added, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bot_links() {
        assert_eq!(normalize_template_url(" https://x.ai/bot/hXd0y-LKCK729av9pzg_X/?ref=a ").unwrap(), "https://x.ai/bot/hXd0y-LKCK729av9pzg_X");
        assert_eq!(normalize_template_url("https://www.x.ai/bot/abc").unwrap(), "https://x.ai/bot/abc");
        assert!(normalize_template_url("https://x.ai/bot/").is_err());
        assert!(normalize_template_url("https://evil.com/bot/abc").is_err());
        assert!(normalize_template_url("https://x.ai/other/abc").is_err());
        assert!(normalize_template_url("not a url").is_err());
    }

    #[test]
    fn parses_og_meta_in_any_attribute_order() {
        let html = r#"<head><meta content="小C by Michael" property="og:title"/>
            <meta property='og:description' content='Tom &amp; Jerry&#x27;s &quot;bot&quot;'></head>"#;
        let meta = parse_template_meta(html);
        assert_eq!(meta.name.as_deref(), Some("小C"));
        assert_eq!(meta.description.as_deref(), Some("Tom & Jerry's \"bot\""));
    }

    #[test]
    fn missing_meta_is_none() {
        let meta = parse_template_meta("<html><title>x</title></html>");
        assert_eq!(meta, BotTemplateMeta::default());
    }

    #[test]
    fn strips_trailing_author_only() {
        assert_eq!(strip_author_suffix("小C by Michael"), "小C");
        assert_eq!(strip_author_suffix("Stand by me by Ann"), "Stand by me");
        assert_eq!(strip_author_suffix("Plain"), "Plain");
        assert_eq!(strip_author_suffix("by Ann"), "by Ann");
    }

    #[test]
    fn dedupes_by_url() {
        let item = |url: &str| BotTemplate { url: url.into(), name: "n".into(), description: None, note: None, auto_import: false, created_at: 1, builtin_id: None, builtin_hash: None };
        let (list, added) = upsert_template(vec![item("a")], item("b"));
        assert!(added);
        assert_eq!(list[0].url, "b");
        let (list, added) = upsert_template(list, item("a"));
        assert!(!added);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn reorders_by_urls_keeping_unlisted() {
        let item = |url: &str| BotTemplate { url: url.into(), name: "n".into(), description: None, note: None, auto_import: false, created_at: 1, builtin_id: None, builtin_hash: None };
        let list = vec![item("a"), item("b"), item("c")];
        let urls: Vec<String> = vec!["c".into(), "x".into(), "a".into()];
        let order: Vec<String> = reorder_templates(list, &urls).into_iter().map(|t| t.url).collect();
        assert_eq!(order, vec!["c", "a", "b"]);
    }

    #[test]
    fn updates_fields_by_url() {
        let item = |url: &str| BotTemplate { url: url.into(), name: "n".into(), description: Some("d".into()), note: None, auto_import: false, created_at: 1, builtin_id: None, builtin_hash: None };
        let patch = BotTemplatePatch { name: "  新名字 ".into(), description: Some("  ".into()), note: Some(" 备注 ".into()), auto_import: true };
        let list = update_template(vec![item("a"), item("b")], "b", patch.clone()).unwrap();
        assert_eq!(list[0], item("a"));
        assert_eq!(list[1].name, "新名字");
        assert_eq!(list[1].description, None);
        assert_eq!(list[1].note.as_deref(), Some("备注"));
        assert!(list[1].auto_import);
        assert!(update_template(vec![item("a")], "x", patch.clone()).is_err());
        let blank = BotTemplatePatch { name: " ".into(), ..patch };
        assert!(update_template(vec![item("a")], "a", blank).is_err());
    }

    #[test]
    fn old_records_deserialize_with_defaults() {
        let list = parse_templates(Some(r#"[{"url":"https://x.ai/bot/a","name":"A","createdAt":1}]"#));
        assert_eq!(list[0].note, None);
        assert!(!list[0].auto_import);
    }

    #[test]
    fn auto_import_keeps_list_order_and_skips_unchecked() {
        let item = |url: &str, auto_import: bool| BotTemplate { url: url.into(), name: "n".into(), description: None, note: None, auto_import, created_at: 1, builtin_id: None, builtin_hash: None };
        let list = vec![item("a", true), item("b", false), item("c", true)];
        let urls: Vec<String> = auto_import_items(&list).into_iter().map(|i| i.url).collect();
        assert_eq!(urls, vec!["a", "c"]);
        assert!(auto_import_items(&[item("x", false)]).is_empty());
    }

    #[test]
    fn builds_client_deep_link() {
        assert_eq!(
            client_deep_link("https://x.ai/bot/hXd0y-LKCK729av9pzg_X/?ref=1").unwrap(),
            "grokbot://app/v1/bot-template?id=hXd0y-LKCK729av9pzg_X"
        );
        assert!(client_deep_link("https://evil.com/bot/abc").is_err());
    }

    fn sample(url: &str, auto_import: bool) -> BotTemplate {
        BotTemplate { url: url.into(), name: format!("n-{url}"), description: Some("d".into()), note: None, auto_import, created_at: 1, builtin_id: None, builtin_hash: None }
    }

    #[test]
    fn export_has_only_public_fields_and_selection_order() {
        let list = vec![sample("https://x.ai/bot/a", true), sample("https://x.ai/bot/b", false)];
        let file = export_file(&list, Some(&["https://x.ai/bot/b".to_string()]), "2026-10-22T00:00:00Z".into());
        assert_eq!(file.templates.len(), 1);
        let json = serde_json::to_value(&file).unwrap();
        assert_eq!(json["format"], "storm-dock/bot-templates");
        assert_eq!(json["version"], 1);
        let keys: Vec<&String> = json["templates"][0].as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["autoImport", "description", "name", "url"]);
        assert_eq!(export_file(&list, None, String::new()).templates.len(), 2);
    }

    #[test]
    fn round_trips_and_resets_auto_import() {
        let list = vec![sample("https://x.ai/bot/a", true)];
        let json = export_json(&list, None).unwrap();
        let parsed = parse_import(&json).unwrap();
        assert_eq!(parsed[0].url, "https://x.ai/bot/a");
        assert_eq!(parsed[0].name, "n-https://x.ai/bot/a");
        assert!(!parsed[0].auto_import);
    }

    #[test]
    fn rejects_invalid_import_files() {
        let ok = |templates: &str| format!(r#"{{"format":"storm-dock/bot-templates","version":1,"exportedAt":"x","templates":{templates}}}"#);
        assert!(parse_import("nope").is_err());
        assert!(parse_import(r#"{"format":"other","version":1,"exportedAt":"x","templates":[]}"#).is_err());
        assert!(parse_import(r#"{"format":"storm-dock/bot-templates","version":2,"exportedAt":"x","templates":[{"url":"https://x.ai/bot/a","name":"A"}]}"#).is_err());
        assert!(parse_import(&ok("[]")).is_err());
        assert!(parse_import(&ok(r#"[{"url":"https://evil.com/bot/a","name":"A"}]"#)).is_err());
        assert!(parse_import(&ok(r#"[{"url":"https://x.ai/bot/a","name":"  "}]"#)).is_err());
        assert!(parse_import(&ok(r#"[{"url":"https://x.ai/bot/a","name":"A","token":"secret"}]"#)).is_err());
        assert_eq!(parse_import(&ok(r#"[{"url":"https://x.ai/bot/a/?x=1","name":" A "}]"#)).unwrap()[0].name, "A");
    }

    #[test]
    fn merge_appends_new_and_skips_existing_or_repeated() {
        let existing = vec![sample("https://x.ai/bot/a", false)];
        let incoming = vec![sample("https://x.ai/bot/b", false), sample("https://x.ai/bot/a", false), sample("https://x.ai/bot/c", false), sample("https://x.ai/bot/b", false)];
        let (list, added, skipped) = merge_import(existing, incoming);
        let urls: Vec<&str> = list.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(urls, vec!["https://x.ai/bot/a", "https://x.ai/bot/b", "https://x.ai/bot/c"]);
        assert_eq!((added, skipped), (2, 2));
    }

    const B: BuiltinTemplate = BuiltinTemplate { id: "b1", url: "https://x.ai/bot/b1", name: "B", description: Some("d"), note: None };

    #[test]
    fn seeds_builtins_once_and_respects_deletion() {
        let (mut list, mut seeded) = (vec![sample("https://x.ai/bot/a", false)], vec![]);
        assert!(sync_builtins(&mut list, &mut seeded, &[B], 5));
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].builtin_id.as_deref(), Some("b1"));
        assert!(!list[1].auto_import);
        assert!(!sync_builtins(&mut list, &mut seeded, &[B], 6));
        assert_eq!(list.len(), 2);
        list.retain(|item| item.builtin_id.is_none());
        assert!(!sync_builtins(&mut list, &mut seeded, &[B], 7));
        assert_eq!(list.len(), 1);
        let b2 = BuiltinTemplate { id: "b2", url: "https://x.ai/bot/b2", name: "B2", description: None, note: None };
        assert!(sync_builtins(&mut list, &mut seeded, &[B, b2], 8));
        assert_eq!(list.len(), 2);
        assert_eq!(seeded, vec!["b1", "b2"]);
    }

    #[test]
    fn adopts_existing_record_with_same_link() {
        let mut list = vec![sample("https://x.ai/bot/a", false), sample("https://x.ai/bot/b1/", true)];
        let mut seeded = vec![];
        assert!(sync_builtins(&mut list, &mut seeded, &[B], 1));
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].builtin_id.as_deref(), Some("b1"));
        assert_eq!(list[1].name, "n-https://x.ai/bot/b1/");
        assert!(list[1].auto_import);
        // 用户内容与内置不同 => 视为已修改，更新不覆盖
        let v2 = BuiltinTemplate { name: "B2", ..B };
        assert!(!sync_builtins(&mut list, &mut seeded, &[v2], 2));
        assert!(list[1].name.starts_with("n-"));
    }

    #[test]
    fn adopted_unmodified_record_still_receives_updates() {
        let mut item = sample("https://x.ai/bot/b1", false);
        item.name = "B".into();
        let (mut list, mut seeded) = (vec![item], vec![]);
        sync_builtins(&mut list, &mut seeded, &[B], 1);
        let v2 = BuiltinTemplate { name: "B2", ..B };
        assert!(sync_builtins(&mut list, &mut seeded, &[v2], 2));
        assert_eq!(list[0].name, "B2");
    }

    #[test]
    fn migrates_records_seeded_by_old_logic() {
        let (mut list, mut seeded) = (vec![sample("https://x.ai/bot/b1", false)], vec!["b1".to_string()]);
        assert!(sync_builtins(&mut list, &mut seeded, &[B], 1));
        assert_eq!(list[0].builtin_id.as_deref(), Some("b1"));
        assert_eq!(seeded.len(), 1);
        // 已有其它记录持有该 builtinId 时不再认领
        let mut owner = sample("https://x.ai/bot/zz", false);
        owner.builtin_id = Some("b1".into());
        owner.builtin_hash = Some(content_hash("B", Some("d"), None));
        let mut list = vec![owner, sample("https://x.ai/bot/b1", false)];
        assert!(!sync_builtins(&mut list, &mut seeded, &[B], 2));
        assert_eq!(list[1].builtin_id, None);
        // 删除后不会被重新认领/加回
        let mut list = vec![sample("https://x.ai/bot/a", false)];
        assert!(!sync_builtins(&mut list, &mut seeded, &[B], 3));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn updates_only_unmodified_builtins() {
        let (mut list, mut seeded) = (vec![], vec![]);
        sync_builtins(&mut list, &mut seeded, &[B], 1);
        let v2 = BuiltinTemplate { description: Some("new"), ..B };
        assert!(sync_builtins(&mut list, &mut seeded, &[v2], 2));
        assert_eq!(list[0].description.as_deref(), Some("new"));
        list[0].note = Some("mine".into());
        list[0].auto_import = true;
        let v3 = BuiltinTemplate { name: "B3", ..B };
        assert!(!sync_builtins(&mut list, &mut seeded, &[v3], 3));
        assert_eq!(list[0].name, "B");
        assert_eq!(list[0].note.as_deref(), Some("mine"));
    }

    #[test]
    fn builtins_are_never_exported() {
        let (mut list, mut seeded) = (vec![sample("https://x.ai/bot/a", false)], vec![]);
        sync_builtins(&mut list, &mut seeded, &[B], 1);
        assert_eq!(export_file(&list, None, String::new()).templates.len(), 1);
        assert!(export_file(&list, Some(&["https://x.ai/bot/b1".to_string()]), String::new()).templates.is_empty());
        let json = export_json(&list, None).unwrap();
        assert!(!json.contains("b1") && !json.contains("builtin"));
    }
}
