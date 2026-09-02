use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, RANGE, USER_AGENT};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

use crate::store::AppState;

pub(crate) const CHUNK_SIZE: u64 = 32 * 1024 * 1024;
pub(crate) const CHUNK_THRESHOLD: u64 = 32 * 1024 * 1024;
pub(crate) const MAX_CHUNK_WORKERS: usize = 8;

static STORED_HF_TOKEN: Mutex<Option<String>> = Mutex::new(None);
static REFRESH_LOCK: Mutex<()> = Mutex::new(());
static REFRESH_EPOCH: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum ModelSource {
    #[serde(rename = "huggingface")]
    HuggingFace,
    #[serde(rename = "modelscope")]
    ModelScope,
}

impl ModelSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::ModelScope => "modelscope",
        }
    }

    fn default_revision(self) -> &'static str {
        match self {
            Self::HuggingFace => "main",
            Self::ModelScope => "master",
        }
    }

    fn token_env(self) -> &'static [&'static str] {
        match self {
            Self::HuggingFace => &["HF_TOKEN", "HUGGING_FACE_HUB_TOKEN"],
            Self::ModelScope => &["MODELSCOPE_TOKEN", "MODELSCOPE_API_TOKEN"],
        }
    }
}

pub(crate) fn source_from(value: &str) -> Result<ModelSource, String> {
    match value {
        "huggingface" => Ok(ModelSource::HuggingFace),
        "modelscope" => Ok(ModelSource::ModelScope),
        _ => Err("未知模型来源".into()),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteModelFile {
    pub path: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ModelFit {
    Fits,
    Marginal,
    Partial,
    Ram,
    Oom,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteModelVariant {
    pub id: String,
    pub label: String,
    pub size: u64,
    pub files: Vec<String>,
    pub fit: ModelFit,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteModelCard {
    pub author: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub license: Option<String>,
    pub library: Option<String>,
    pub pipeline: Option<String>,
    pub base_model: Option<String>,
    pub downloads: Option<u64>,
    pub likes: Option<u64>,
    pub params: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteModelProbe {
    pub source: ModelSource,
    pub repo: String,
    pub revision: String,
    pub files: Vec<RemoteModelFile>,
    pub variants: Vec<RemoteModelVariant>,
    pub default_variant_id: String,
    pub card: RemoteModelCard,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteModelHit {
    pub source: ModelSource,
    pub repo: String,
    pub name: String,
    pub author: String,
    pub downloads: Option<u64>,
    pub likes: Option<u64>,
    pub library: Option<String>,
    pub pipeline: Option<String>,
    pub tags: Vec<String>,
    pub params: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalLlm {
    pub id: String,
    pub source: ModelSource,
    pub repo: String,
    pub revision: String,
    pub path: String,
    pub size: u64,
    pub files: u32,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Manifest {
    pub source: ModelSource,
    pub repo: String,
    pub revision: String,
    pub files: Vec<RemoteModelFile>,
}

#[derive(Deserialize)]
struct HfTreeItem {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    size: Option<u64>,
    lfs: Option<HfLfs>,
}

#[derive(Deserialize)]
struct HfLfs {
    size: u64,
    oid: Option<String>,
}

pub(crate) fn parse_repo_id(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_matches('/');
    let stripped = trimmed
        .strip_prefix("https://huggingface.co/")
        .or_else(|| trimmed.strip_prefix("http://huggingface.co/"))
        .or_else(|| trimmed.strip_prefix("https://www.modelscope.cn/models/"))
        .or_else(|| trimmed.strip_prefix("https://modelscope.cn/models/"))
        .or_else(|| trimmed.strip_prefix("http://www.modelscope.cn/models/"))
        .unwrap_or(trimmed)
        .trim_matches('/');
    let stripped = stripped.split('?').next().unwrap_or(stripped);
    let stripped = stripped.split("/tree/").next().unwrap_or(stripped);
    let stripped = stripped.split("/resolve/").next().unwrap_or(stripped);
    let mut parts = stripped.split('/');
    let owner = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if parts.next().is_some() || owner.is_empty() || name.is_empty() {
        return Err("请输入 owner/name 格式的模型 ID".into());
    }
    if !valid_repo_part(owner) || !valid_repo_part(name) {
        return Err("模型 ID 含有非法字符".into());
    }
    Ok(format!("{owner}/{name}"))
}

fn valid_repo_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub(crate) fn safe_file_path(relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err("文件路径无效".into());
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) if name != ".." => out.push(name),
            Component::CurDir => {}
            _ => return Err("文件路径无效".into()),
        }
    }
    if out.as_os_str().is_empty() {
        return Err("文件路径无效".into());
    }
    Ok(out)
}

pub(crate) fn plan_chunks(size: u64, chunk: u64) -> Vec<(u64, u64)> {
    if size == 0 || chunk == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < size {
        let end = start.saturating_add(chunk).min(size) - 1;
        out.push((start, end));
        start = end + 1;
    }
    out
}

pub(crate) fn is_complete(path: &Path, size: u64) -> bool {
    fs::metadata(path)
        .map(|meta| meta.len() == size)
        .unwrap_or(false)
}

const DISK_SLACK: u64 = 64 * 1024 * 1024;

pub(crate) fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

pub(crate) fn chunks_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".chunks");
    dest.with_file_name(name)
}

pub(crate) fn file_have_bytes(dest_root: &Path, file: &RemoteModelFile) -> u64 {
    let dest = dest_root.join(safe_file_path(&file.path).unwrap_or_default());
    if is_complete(&dest, file.size) {
        return file.size;
    }
    let sidecar = chunks_path(&dest);
    if sidecar.exists() {
        let done: Vec<usize> = fs::read(&sidecar)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let chunks = plan_chunks(file.size, CHUNK_SIZE);
        return done
            .iter()
            .filter_map(|index| chunks.get(*index))
            .map(|(start, end)| end.saturating_sub(*start).saturating_add(1))
            .sum();
    }
    fs::metadata(part_path(&dest))
        .map(|meta| meta.len().min(file.size))
        .unwrap_or(0)
}

pub(crate) fn remaining_download_bytes(dest_root: &Path, files: &[RemoteModelFile]) -> u64 {
    files
        .iter()
        .map(|file| file.size.saturating_sub(file_have_bytes(dest_root, file)))
        .sum()
}

pub(crate) fn downloaded_bytes(dest_root: &Path, files: &[RemoteModelFile]) -> u64 {
    files
        .iter()
        .map(|file| file_have_bytes(dest_root, file))
        .sum()
}

pub(crate) fn disk_space_shortfall(
    needed: u64,
    free: Option<u64>,
    slack: u64,
) -> Option<(u64, u64)> {
    let free = free?;
    (needed > 0 && free < needed.saturating_add(slack)).then_some((needed, free))
}

pub(crate) fn ensure_disk_space(dest_root: &Path, files: &[RemoteModelFile]) -> Result<(), String> {
    let needed = remaining_download_bytes(dest_root, files);
    if let Some((needed, free)) =
        disk_space_shortfall(needed, free_disk_bytes_at(dest_root), DISK_SLACK)
    {
        return Err(format!("insufficient-disk:{needed}:{free}"));
    }
    Ok(())
}

fn free_disk_bytes_at(path: &Path) -> Option<u64> {
    let mut current = path;
    loop {
        if current.exists() {
            return free_disk_bytes(current);
        }
        current = current.parent()?;
    }
}

#[cfg(unix)]
fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = unsafe { std::mem::zeroed::<libc::statvfs>() };
    let rc = unsafe { libc::statvfs(cpath.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    Some(stat.f_frsize as u64 * stat.f_bavail as u64)
}

#[cfg(windows)]
fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_available: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
}

#[cfg(not(any(unix, windows)))]
fn free_disk_bytes(_path: &Path) -> Option<u64> {
    None
}

pub(crate) fn dir_name(repo: &str) -> String {
    repo.replace('/', "__")
}

pub(crate) fn model_id(source: ModelSource, repo: &str) -> String {
    format!("{}/{}", source.as_str(), dir_name(repo))
}

pub(crate) fn models_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("models"))
        .map_err(|error| error.to_string())
}

pub(crate) fn http_client() -> Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .tcp_keepalive(Duration::from_secs(30))
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(MAX_CHUNK_WORKERS)
                .build()
                .unwrap_or_else(|_| Client::new())
        })
        .clone()
}

pub(crate) fn bind_stored_hf_token(state: &State<'_, AppState>) {
    let token = state
        .0
        .lock()
        .ok()
        .and_then(|controller| controller.hf_token());
    if let Ok(mut slot) = STORED_HF_TOKEN.lock() {
        *slot = token;
    }
}

pub(crate) fn same_origin(a: &reqwest::Url, b: &reqwest::Url) -> bool {
    a.scheme() == b.scheme()
        && a.host() == b.host()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn user_agent_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, "Storm-Dock/1.1".parse().unwrap());
    headers
}

pub(crate) fn download_headers(
    source: ModelSource,
    target: &reqwest::Url,
    origin: &reqwest::Url,
) -> HeaderMap {
    if same_origin(target, origin) {
        auth_headers(source)
    } else {
        user_agent_headers()
    }
}

pub(crate) fn cdn_expired(status: StatusCode) -> bool {
    matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
}

pub(crate) fn follow_cdn_url(
    client: &Client,
    source: ModelSource,
    url: reqwest::Url,
) -> reqwest::Url {
    let Ok(response) = client
        .get(url.clone())
        .headers(auth_headers(source))
        .header(RANGE, "bytes=0-0")
        .send()
    else {
        return url;
    };
    response.url().clone()
}

pub(crate) fn auth_headers(source: ModelSource) -> HeaderMap {
    let mut headers = user_agent_headers();
    let stored = match source {
        ModelSource::HuggingFace => STORED_HF_TOKEN.lock().ok().and_then(|slot| slot.clone()),
        ModelSource::ModelScope => None,
    };
    let token = stored.filter(|value| !value.is_empty()).or_else(|| {
        source.token_env().iter().find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
    });
    if let Some(token) = token {
        if let Ok(value) = format!("Bearer {token}").parse() {
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
    }
    headers
}

pub(crate) fn status_error(source: ModelSource, status: StatusCode) -> String {
    match status {
        StatusCode::NOT_FOUND => "模型不存在".into(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => match source {
            ModelSource::HuggingFace => "该模型需要 Hugging Face 令牌，请在设置中配置".into(),
            ModelSource::ModelScope => {
                "模型存在但需要魔搭令牌，请设置环境变量 MODELSCOPE_TOKEN".into()
            }
        },
        other => format!("远程接口返回 HTTP {other}"),
    }
}

pub(crate) fn probe(
    source: ModelSource,
    repo: &str,
    revision: Option<&str>,
) -> Result<RemoteModelProbe, String> {
    let repo = parse_repo_id(repo)?;
    let mut revision = revision
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| source.default_revision())
        .to_string();
    if source == ModelSource::HuggingFace && revision.len() < 40 {
        let client = http_client();
        if let Ok(response) = client
            .get(format!("https://huggingface.co/api/models/{repo}"))
            .headers(auth_headers(source))
            .send()
        {
            if let Ok(meta) = response.json::<serde_json::Value>() {
                if let Some(commit) = meta.get("sha").and_then(|value| value.as_str()) {
                    revision = commit.to_string();
                }
            }
        }
    }
    let (card, files) = match source {
        ModelSource::HuggingFace => probe_huggingface(&repo, &revision)?,
        ModelSource::ModelScope => probe_modelscope(&repo, &revision)?,
    };
    let (mut variants, default_variant_id) = group_weight_variants(&files);
    apply_variant_fit(&mut variants);
    Ok(RemoteModelProbe {
        source,
        repo,
        revision,
        files,
        variants,
        default_variant_id,
        card,
    })
}

fn probe_huggingface(
    repo: &str,
    revision: &str,
) -> Result<(RemoteModelCard, Vec<RemoteModelFile>), String> {
    let client = http_client();
    let headers = auth_headers(ModelSource::HuggingFace);
    let info = client
        .get(format!("https://huggingface.co/api/models/{repo}"))
        .headers(headers.clone())
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|error| error.to_string())?;
    if !info.status().is_success() {
        return Err(status_error(ModelSource::HuggingFace, info.status()));
    }
    let meta: serde_json::Value = info.json().map_err(|error| error.to_string())?;
    let mut card = card_from_hf(&meta, repo);
    if card.description.is_empty() {
        if let Some(text) = fetch_plain(
            &client,
            &headers,
            &format!("https://huggingface.co/{repo}/raw/{revision}/README.md"),
        ) {
            card.description = first_readme_paragraph(&text);
        }
    }
    let mut files = Vec::new();
    let mut url = format!("https://huggingface.co/api/models/{repo}/tree/{revision}?recursive=1");
    loop {
        let response = client
            .get(&url)
            .headers(headers.clone())
            .timeout(Duration::from_secs(30))
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(status_error(ModelSource::HuggingFace, response.status()));
        }
        let next = link_next(
            response
                .headers()
                .get(reqwest::header::LINK)
                .and_then(|value| value.to_str().ok()),
        );
        let items: Vec<HfTreeItem> = response.json().map_err(|error| error.to_string())?;
        for item in items {
            if item.kind != "file" {
                continue;
            }
            let lfs = item.lfs;
            let size = lfs.as_ref().map(|lfs| lfs.size).or(item.size).unwrap_or(0);
            files.push(RemoteModelFile {
                path: item.path,
                size,
                sha256: lfs.and_then(|value| value.oid).and_then(|oid| {
                    oid.strip_prefix("sha256:")
                        .map(str::to_string)
                        .or(Some(oid))
                }),
                revision: Some(revision.to_string()),
            });
        }
        match next {
            Some(next_url) => url = next_url,
            None => break,
        }
    }
    Ok((card, files))
}

fn link_next(header: Option<&str>) -> Option<String> {
    let header = header?;
    header.split(',').find_map(|part| {
        let part = part.trim();
        if !part.contains("rel=\"next\"") && !part.contains("rel=next") {
            return None;
        }
        let start = part.find('<')? + 1;
        let end = part.find('>')?;
        Some(part[start..end].to_string())
    })
}

fn probe_modelscope(
    repo: &str,
    revision: &str,
) -> Result<(RemoteModelCard, Vec<RemoteModelFile>), String> {
    let client = http_client();
    let headers = auth_headers(ModelSource::ModelScope);
    let info = client
        .get(format!("https://www.modelscope.cn/api/v1/models/{repo}"))
        .headers(headers.clone())
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|error| error.to_string())?;
    if !info.status().is_success() {
        return Err(status_error(ModelSource::ModelScope, info.status()));
    }
    let envelope: serde_json::Value = info.json().map_err(|error| error.to_string())?;
    if envelope.get("Success").and_then(|value| value.as_bool()) == Some(false)
        || envelope
            .get("Code")
            .and_then(|value| value.as_i64())
            .is_some_and(|code| code != 200)
    {
        let message = envelope
            .get("Message")
            .and_then(|value| value.as_str())
            .unwrap_or("模型不存在");
        return Err(message.into());
    }
    let card = card_from_modelscope(&envelope, repo);
    let response = client
        .get(format!(
            "https://www.modelscope.cn/api/v1/models/{repo}/repo/files?Revision={revision}&Recursive=true"
        ))
        .headers(headers)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(status_error(ModelSource::ModelScope, response.status()));
    }
    let body: serde_json::Value = response.json().map_err(|error| error.to_string())?;
    if body.get("Success").and_then(|value| value.as_bool()) == Some(false) {
        return Err(body
            .get("Message")
            .and_then(|value| value.as_str())
            .unwrap_or("无法读取模型文件列表")
            .into());
    }
    let data = body.get("Data").unwrap_or(&body);
    let items = data
        .get("Files")
        .and_then(|value| value.as_array())
        .or_else(|| data.as_array())
        .ok_or_else(|| "无法读取模型文件列表".to_string())?;
    let mut files = Vec::new();
    for item in items {
        let kind = item
            .get("Type")
            .or_else(|| item.get("type"))
            .and_then(|value| value.as_str())
            .unwrap_or("blob");
        if matches!(kind, "tree" | "dir" | "directory") {
            continue;
        }
        let path = item
            .get("Path")
            .or_else(|| item.get("path"))
            .or_else(|| item.get("Name"))
            .or_else(|| item.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if path.is_empty() || path == ".gitignore" || path == ".gitattributes" {
            continue;
        }
        let size = item
            .get("Size")
            .or_else(|| item.get("size"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        files.push(RemoteModelFile {
            path: path.to_string(),
            size,
            sha256: item
                .get("Sha256")
                .or_else(|| item.get("sha256"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
            revision: item
                .get("Revision")
                .or_else(|| item.get("revision"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
        });
    }
    Ok((card, files))
}

fn fetch_plain(
    client: &reqwest::blocking::Client,
    headers: &HeaderMap,
    url: &str,
) -> Option<String> {
    let response = client
        .get(url)
        .headers(headers.clone())
        .timeout(Duration::from_secs(15))
        .send()
        .ok()?;
    response
        .status()
        .is_success()
        .then(|| response.text().ok())?
}

fn first_readme_paragraph(raw: &str) -> String {
    let body = raw
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---"))
        .map(|(_, rest)| rest)
        .unwrap_or(raw);
    let mut out = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        if line.starts_with('#')
            || line.starts_with('!')
            || line.starts_with("<")
            || line.starts_with("[![")
        {
            continue;
        }
        let cleaned = line.trim_start_matches(['>', '-', '*']).trim();
        if cleaned.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(cleaned);
        if out.chars().count() >= 280 {
            break;
        }
    }
    out.chars().take(280).collect()
}

fn card_from_hf(meta: &serde_json::Value, repo: &str) -> RemoteModelCard {
    let (fallback_author, fallback_name) = split_repo(repo);
    let card_data = meta
        .get("cardData")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let pipeline = json_text(meta, &["pipeline_tag"]);
    let mut tags = json_strings(meta, "tags");
    tags.extend(json_strings(&card_data, "tags"));
    RemoteModelCard {
        author: json_text(meta, &["author"]).unwrap_or(fallback_author),
        name: json_text(&card_data, &["pretty_name"]).unwrap_or(fallback_name),
        description: json_text(&card_data, &["model_summary"])
            .or_else(|| json_text(meta, &["description"]))
            .unwrap_or_default(),
        tags: useful_tags(tags, pipeline.as_deref()),
        license: json_text(&card_data, &["license"]).or_else(|| json_text(meta, &["license"])),
        library: json_text(meta, &["library_name"]),
        pipeline,
        base_model: json_base_model(&card_data),
        downloads: json_u64(meta, &["downloads", "downloadsAllTime"]),
        likes: json_u64(meta, &["likes"]),
        params: json_u64(
            meta.get("safetensors").unwrap_or(&serde_json::Value::Null),
            &["total"],
        )
        .map(format_params),
        updated_at: json_text(meta, &["lastModified"]),
    }
}

fn card_from_modelscope(envelope: &serde_json::Value, repo: &str) -> RemoteModelCard {
    let data = envelope.get("Data").unwrap_or(envelope);
    let (fallback_author, fallback_name) = split_repo(repo);
    let pipeline = json_text(data, &["Tasks"])
        .or_else(|| json_strings(data, "Tasks").into_iter().next())
        .or_else(|| json_strings(data, "ModelType").into_iter().next());
    let mut tags = json_strings(data, "Tags");
    tags.extend(json_strings(data, "Tasks"));
    RemoteModelCard {
        author: json_text(data, &["Author", "Path"]).unwrap_or(fallback_author),
        name: json_text(data, &["ChineseName", "Name"]).unwrap_or(fallback_name),
        description: json_text(data, &["Description", "ChineseDescription"]).unwrap_or_default(),
        tags: useful_tags(tags, pipeline.as_deref()),
        license: json_text(data, &["License"]),
        library: json_text(data, &["LibraryName", "Frameworks"]),
        pipeline,
        base_model: json_text(data, &["BaseModelId", "BaseModel"]),
        downloads: json_u64(data, &["Downloads", "DownloadCount", "DownloadsCount"]),
        likes: json_u64(data, &["Stars", "Likes", "StarCount"]),
        params: json_text(data, &["Parameters", "ParameterSize"]),
        updated_at: json_text(data, &["LastUpdatedTime", "GmtModified", "CreatedTime"]),
    }
}

fn split_repo(repo: &str) -> (String, String) {
    match repo.split_once('/') {
        Some((author, name)) => (author.to_string(), name.to_string()),
        None => (String::new(), repo.to_string()),
    }
}

fn json_text(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| match item {
            serde_json::Value::String(text) => {
                Some(text.trim().to_string()).filter(|text| !text.is_empty())
            }
            serde_json::Value::Array(items) => items
                .iter()
                .find_map(|item| item.as_str().map(str::to_string)),
            _ => None,
        })
    })
}

fn json_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_u64()
                .or_else(|| item.as_f64().map(|number| number as u64))
        })
    })
}

const SEARCH_LIMIT: usize = 30;

fn hf_format_filter(format: &str) -> Option<&'static str> {
    match format {
        "gguf" => Some("gguf"),
        "safetensors" => Some("safetensors"),
        "mlx" => Some("mlx"),
        "finetune" => Some("peft"),
        _ => None,
    }
}

fn format_needle(format: &str) -> Option<&'static str> {
    hf_format_filter(format)
}

fn hit_matches_format(hit: &RemoteModelHit, format: &str) -> bool {
    let Some(needle) = format_needle(format) else {
        return true;
    };
    let hay = format!(
        "{} {} {} {} {}",
        hit.repo,
        hit.name,
        hit.library.as_deref().unwrap_or(""),
        hit.pipeline.as_deref().unwrap_or(""),
        hit.tags.join(" ")
    )
    .to_ascii_lowercase();
    hay.contains(needle)
}

fn hit_from_card(source: ModelSource, repo: String, card: RemoteModelCard) -> RemoteModelHit {
    RemoteModelHit {
        source,
        name: card.name,
        author: card.author,
        downloads: card.downloads,
        likes: card.likes,
        library: card.library,
        pipeline: card.pipeline,
        tags: card.tags,
        params: card.params,
        updated_at: card.updated_at,
        repo,
    }
}

fn lookup_exact(source: ModelSource, query: &str) -> Option<RemoteModelHit> {
    let repo = parse_repo_id(query).ok()?;
    match source {
        ModelSource::HuggingFace => lookup_huggingface(&repo),
        ModelSource::ModelScope => lookup_modelscope(&repo),
    }
}

fn lookup_huggingface(repo: &str) -> Option<RemoteModelHit> {
    let client = http_client();
    let headers = auth_headers(ModelSource::HuggingFace);
    let response = client
        .get(format!("https://huggingface.co/api/models/{repo}"))
        .headers(headers)
        .timeout(Duration::from_secs(20))
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let meta: serde_json::Value = response.json().ok()?;
    Some(hit_from_card(
        ModelSource::HuggingFace,
        repo.to_string(),
        card_from_hf(&meta, repo),
    ))
}

fn lookup_modelscope(repo: &str) -> Option<RemoteModelHit> {
    let client = http_client();
    let headers = auth_headers(ModelSource::ModelScope);
    let response = client
        .get(format!("https://www.modelscope.cn/api/v1/models/{repo}"))
        .headers(headers)
        .timeout(Duration::from_secs(20))
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let envelope: serde_json::Value = response.json().ok()?;
    if envelope.get("Success").and_then(|value| value.as_bool()) == Some(false)
        || envelope
            .get("Code")
            .and_then(|value| value.as_i64())
            .is_some_and(|code| code != 200)
    {
        return None;
    }
    Some(hit_from_card(
        ModelSource::ModelScope,
        repo.to_string(),
        card_from_modelscope(&envelope, repo),
    ))
}

fn merge_exact(
    exact: Option<RemoteModelHit>,
    mut hits: Vec<RemoteModelHit>,
) -> Vec<RemoteModelHit> {
    let Some(exact) = exact else {
        return hits;
    };
    hits.retain(|hit| !hit.repo.eq_ignore_ascii_case(&exact.repo));
    hits.insert(0, exact);
    hits.truncate(SEARCH_LIMIT);
    hits
}

fn hits_from_hf_value(value: &serde_json::Value, source: ModelSource) -> Vec<RemoteModelHit> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| hit_from_hf(item, source))
        .take(SEARCH_LIMIT)
        .collect()
}

fn hit_from_hf(item: &serde_json::Value, source: ModelSource) -> Option<RemoteModelHit> {
    let repo = json_text(item, &["id", "modelId"])?;
    let (author, name) = split_repo(&repo);
    let tags = json_strings(item, "tags");
    Some(RemoteModelHit {
        source,
        name: json_text(item, &["pretty_name"]).unwrap_or(name),
        author: json_text(item, &["author"]).unwrap_or(author),
        downloads: json_u64(item, &["downloads", "downloadsAllTime"]),
        likes: json_u64(item, &["likes"]),
        library: json_text(item, &["library_name"]),
        pipeline: json_text(item, &["pipeline_tag"]),
        params: hit_params(item, &tags),
        updated_at: hit_updated(item),
        tags,
        repo,
    })
}

fn modelscope_model_items(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    const PATHS: &[&[&str]] = &[
        &["data", "models"],
        &["Data", "Model", "Models"],
        &["Data", "Models"],
        &["Data", "models"],
        &["Data"],
        &["Models"],
    ];
    for keys in PATHS {
        let mut current = value;
        let mut found = true;
        for key in *keys {
            match current.get(*key) {
                Some(next) => current = next,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if found {
            if let Some(items) = current.as_array() {
                return items.iter().collect();
            }
        }
    }
    Vec::new()
}

fn hits_from_modelscope_value(value: &serde_json::Value, format: &str) -> Vec<RemoteModelHit> {
    modelscope_model_items(value)
        .into_iter()
        .filter_map(hit_from_modelscope)
        .filter(|hit| hit_matches_format(hit, format))
        .take(SEARCH_LIMIT)
        .collect()
}

fn hit_from_modelscope(item: &serde_json::Value) -> Option<RemoteModelHit> {
    let repo = json_text(item, &["id", "Path", "ModelId", "Name"])?;
    let (author, name) = split_repo(&repo);
    let mut tags = json_strings(item, "Tags");
    tags.extend(json_strings(item, "tags"));
    tags.extend(json_strings(item, "Libraries"));
    tags.extend(json_strings(item, "Tasks"));
    tags.extend(json_strings(item, "tasks"));
    Some(RemoteModelHit {
        source: ModelSource::ModelScope,
        name: json_text(item, &["display_name", "ChineseName", "Name"]).unwrap_or(name),
        author: json_text(item, &["author", "Author"]).unwrap_or(author),
        downloads: json_u64(
            item,
            &["downloads", "Downloads", "DownloadCount", "DownloadsCount"],
        ),
        likes: json_u64(item, &["likes", "Stars", "Likes", "StarCount"]),
        library: json_strings(item, "Libraries").into_iter().next(),
        pipeline: json_strings(item, "tasks")
            .into_iter()
            .next()
            .or_else(|| json_strings(item, "Tasks").into_iter().next()),
        params: hit_params(item, &tags),
        updated_at: hit_updated(item),
        tags,
        repo,
    })
}

fn search_huggingface(query: &str, format: &str) -> Result<Vec<RemoteModelHit>, String> {
    let client = http_client();
    let headers = auth_headers(ModelSource::HuggingFace);
    let mut url = reqwest::Url::parse("https://huggingface.co/api/models")
        .map_err(|error| error.to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("search", query);
        pairs.append_pair("limit", &SEARCH_LIMIT.to_string());
        pairs.append_pair("sort", "downloads");
        if let Some(filter) = hf_format_filter(format) {
            pairs.append_pair("filter", filter);
        }
    }
    let response = client
        .get(url)
        .headers(headers)
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(status_error(ModelSource::HuggingFace, response.status()));
    }
    let body: serde_json::Value = response.json().map_err(|error| error.to_string())?;
    Ok(hits_from_hf_value(&body, ModelSource::HuggingFace))
}

fn search_modelscope(query: &str, format: &str) -> Result<Vec<RemoteModelHit>, String> {
    let client = http_client();
    let headers = auth_headers(ModelSource::ModelScope);
    let mut url = reqwest::Url::parse("https://www.modelscope.cn/openapi/v1/models")
        .map_err(|error| error.to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("search", query);
        pairs.append_pair("page_size", &SEARCH_LIMIT.to_string());
        pairs.append_pair("sort", "downloads");
    }
    let response = client
        .get(url)
        .headers(headers)
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(status_error(ModelSource::ModelScope, response.status()));
    }
    let body: serde_json::Value = response.json().map_err(|error| error.to_string())?;
    if body.get("success").and_then(|value| value.as_bool()) == Some(false)
        || body.get("Success").and_then(|value| value.as_bool()) == Some(false)
    {
        return Err(body
            .get("message")
            .or_else(|| body.get("Message"))
            .and_then(|value| value.as_str())
            .unwrap_or("无法搜索模型")
            .into());
    }
    Ok(hits_from_modelscope_value(&body, format))
}

fn json_strings(value: &serde_json::Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .or_else(|| item.get("Name").and_then(|value| value.as_str()))
                    .map(str::to_string)
            })
            .filter(|text| !text.is_empty())
            .collect(),
        Some(serde_json::Value::String(text)) if !text.is_empty() => vec![text.clone()],
        _ => Vec::new(),
    }
}

fn json_base_model(card_data: &serde_json::Value) -> Option<String> {
    match card_data.get("base_model") {
        Some(serde_json::Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .find_map(|item| item.as_str().map(str::to_string)),
        _ => None,
    }
}

fn useful_tags(tags: Vec<String>, pipeline: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(pipeline) = pipeline {
        out.push(pipeline.replace('-', " "));
    }
    for tag in tags {
        let lower = tag.to_ascii_lowercase();
        if lower.contains(':')
            || matches!(
                lower.as_str(),
                "transformers"
                    | "pytorch"
                    | "safetensors"
                    | "gguf"
                    | "endpoints_compatible"
                    | "text-generation-inference"
                    | "region:us"
            )
        {
            continue;
        }
        if out
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&tag))
        {
            continue;
        }
        out.push(tag);
        if out.len() == 6 {
            break;
        }
    }
    out
}

fn format_params(count: u64) -> String {
    let (scaled, suffix) = if count >= 1_000_000_000 {
        (count as f64 / 1_000_000_000.0, "B")
    } else if count >= 1_000_000 {
        (count as f64 / 1_000_000.0, "M")
    } else {
        return count.to_string();
    };
    let rounded = (scaled * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < f64::EPSILON {
        format!("{:.0}{suffix}", rounded)
    } else {
        format!("{rounded:.1}{suffix}")
    }
}

fn params_from_label(raw: &str) -> Option<String> {
    let tag = raw.trim();
    let (number, unit) = tag.split_at(tag.len().checked_sub(1)?);
    let unit = unit.to_ascii_uppercase();
    if unit != "B" && unit != "M" {
        return None;
    }
    number.parse::<f64>().ok().filter(|value| *value > 0.0)?;
    Some(format!("{number}{unit}"))
}

fn hit_params(item: &serde_json::Value, tags: &[String]) -> Option<String> {
    json_u64(
        item.get("safetensors").unwrap_or(&serde_json::Value::Null),
        &["total"],
    )
    .map(format_params)
    .or_else(|| json_text(item, &["Parameters", "ParameterSize"]))
    .or_else(|| tags.iter().find_map(|tag| params_from_label(tag)))
}

fn hit_updated(item: &serde_json::Value) -> Option<String> {
    json_text(
        item,
        &[
            "lastModified",
            "LastUpdatedTime",
            "GmtModified",
            "UpdatedTime",
            "gmt_modified",
        ],
    )
    .or_else(|| {
        json_u64(
            item,
            &[
                "lastModified",
                "LastUpdatedTime",
                "GmtModified",
                "UpdatedTime",
            ],
        )
        .map(|value| value.to_string())
    })
}

const QUANT_PREF: &[&str] = &["Q4_K_M", "Q4_K_XL", "Q5_K_M", "Q4_K_S", "Q8_0", "Q6_K"];

fn group_weight_variants(files: &[RemoteModelFile]) -> (Vec<RemoteModelVariant>, String) {
    let by_path: HashMap<&str, u64> = files
        .iter()
        .map(|file| (file.path.as_str(), file.size))
        .collect();
    let mut sidecars = Vec::new();
    let mut safetensors = Vec::new();
    let mut pytorch = Vec::new();
    let mut gguf: HashMap<String, (String, Vec<String>)> = HashMap::new();
    let mut others = Vec::new();
    for file in files {
        let name = file_name(&file.path);
        if is_sidecar(&file.path) {
            sidecars.push(file.path.clone());
            continue;
        }
        if name.to_ascii_lowercase().ends_with(".gguf") {
            let id = gguf_group_id(&file.path);
            let label = gguf_display_label(&file.path, name);
            gguf.entry(id)
                .or_insert_with(|| (label, Vec::new()))
                .1
                .push(file.path.clone());
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".safetensors") || lower.ends_with(".safetensors.index.json") {
            safetensors.push(file.path.clone());
            continue;
        }
        if lower.starts_with("pytorch_model") && lower.ends_with(".bin") {
            pytorch.push(file.path.clone());
            continue;
        }
        if is_other_weight(&lower) {
            others.push(file.path.clone());
        }
    }
    let mut variants = Vec::new();
    let mut gguf_groups: Vec<_> = gguf.into_iter().collect();
    gguf_groups.sort_by(|left, right| left.0.cmp(&right.0));
    for (id, (label, mut paths)) in gguf_groups {
        paths.sort();
        variants.push(make_variant(id, label, paths, &sidecars, &by_path));
    }
    if !safetensors.is_empty() {
        safetensors.sort();
        variants.push(make_variant(
            "safetensors".into(),
            "Safetensors".into(),
            safetensors,
            &sidecars,
            &by_path,
        ));
    }
    if !pytorch.is_empty() {
        pytorch.sort();
        variants.push(make_variant(
            "pytorch".into(),
            "PyTorch".into(),
            pytorch,
            &sidecars,
            &by_path,
        ));
    }
    for path in others {
        let name = file_name(&path);
        variants.push(make_variant(
            format!("other:{path}"),
            stem_label(name),
            vec![path],
            &sidecars,
            &by_path,
        ));
    }
    let default_variant_id = pick_default_variant(&variants);
    (variants, default_variant_id)
}

fn make_variant(
    id: String,
    label: String,
    mut weights: Vec<String>,
    sidecars: &[String],
    by_path: &HashMap<&str, u64>,
) -> RemoteModelVariant {
    let gguf_variant = id.starts_with("gguf:");
    for sidecar in sidecars {
        let sidecar_lower = sidecar.to_ascii_lowercase();
        let matches_gguf = !gguf_variant
            || !sidecar_lower.ends_with(".gguf")
            || weights.iter().any(|weight| {
                let weight_lower = weight.to_ascii_lowercase();
                let quant = ["q2_k_xl", "q3_k_xl", "q4_k_xl", "q5_k_xl", "q6_k_xl", "q8_0", "q4_k_m", "q5_k_m", "q6_k", "f16", "bf16"]
                    .iter().find(|key| weight_lower.contains(**key));
                quant.map(|key| {
                    !["q2_k_xl", "q3_k_xl", "q4_k_xl", "q5_k_xl", "q6_k_xl", "q8_0", "q4_k_m", "q5_k_m", "q6_k", "f16", "bf16"]
                        .iter().any(|candidate| sidecar_lower.contains(candidate))
                        || sidecar_lower.contains(key)
                }).unwrap_or(true)
            });
        if matches_gguf && !weights.iter().any(|path| path == sidecar) {
            weights.push(sidecar.clone());
        }
    }
    let size = weights
        .iter()
        .map(|path| *by_path.get(path.as_str()).unwrap_or(&0))
        .sum();
    RemoteModelVariant {
        id,
        label,
        size,
        files: weights,
        fit: ModelFit::Unknown,
    }
}

const GIB: u64 = 1 << 30;

#[derive(Clone, Copy)]
struct HostMemory {
    vram: u64,
    ram: u64,
    unified: bool,
}

fn apply_variant_fit(variants: &mut [RemoteModelVariant]) {
    let host = host_memory();
    for variant in variants {
        variant.fit = classify_model_fit(variant.size, host.vram, host.ram, host.unified);
    }
}

fn host_memory() -> HostMemory {
    static HOST: OnceLock<HostMemory> = OnceLock::new();
    *HOST.get_or_init(detect_host_memory)
}

fn classify_model_fit(size: u64, vram: u64, ram: u64, unified: bool) -> ModelFit {
    if vram == 0 && ram == 0 {
        return ModelFit::Unknown;
    }
    if unified {
        let vram_budget = ram * 3 / 4;
        let ram_offload = ram.saturating_sub(vram_budget);
        return classify_against(size, vram_budget, ram_offload);
    }
    if vram == 0 {
        return if size <= ram / 2 {
            ModelFit::Ram
        } else {
            ModelFit::Oom
        };
    }
    classify_against(size, vram, ram / 2)
}

fn classify_against(size: u64, vram_budget: u64, ram_offload: u64) -> ModelFit {
    if size.saturating_add(GIB) <= vram_budget {
        ModelFit::Fits
    } else if size <= vram_budget {
        ModelFit::Marginal
    } else if size <= vram_budget.saturating_add(ram_offload) {
        ModelFit::Partial
    } else {
        ModelFit::Oom
    }
}

fn detect_host_memory() -> HostMemory {
    #[cfg(target_os = "macos")]
    {
        return detect_macos();
    }
    #[cfg(target_os = "linux")]
    {
        return detect_linux();
    }
    #[cfg(windows)]
    {
        return detect_windows();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    HostMemory {
        vram: 0,
        ram: 0,
        unified: false,
    }
}

fn command_stdout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(cmd.output());
    });
    let output = rx.recv_timeout(timeout).ok()?.ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn nvidia_vram() -> Option<u64> {
    let out = command_stdout(
        "nvidia-smi",
        &["--query-gpu=memory.total", "--format=csv,noheader,nounits"],
        Duration::from_secs(2),
    )?;
    let mut total = 0u64;
    for line in out.lines() {
        let mib: u64 = line.trim().parse().ok()?;
        total = total.saturating_add(mib.saturating_mul(1024 * 1024));
    }
    (total > 0).then_some(total)
}

fn parse_size_label(raw: &str) -> Option<u64> {
    let mut parts = raw.split_whitespace();
    let n: f64 = parts.next()?.replace(',', "").parse().ok()?;
    let unit = parts.next().unwrap_or("MB").to_ascii_uppercase();
    let mul = match unit.as_str() {
        "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "MB" | "MIB" => 1024.0 * 1024.0,
        "KB" | "KIB" => 1024.0,
        "B" => 1.0,
        _ => return None,
    };
    Some((n * mul) as u64)
}

fn parse_profiler_vram(text: &str) -> u64 {
    let mut total = 0u64;
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("VRAM (Total):") else {
            continue;
        };
        if let Some(bytes) = parse_size_label(rest.trim()) {
            total = total.saturating_add(bytes);
        }
    }
    total
}

#[cfg(any(windows, test))]
fn first_u64_line(text: &str) -> Option<u64> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim().replace(',', "");
            if trimmed.chars().all(|ch| ch.is_ascii_digit()) && !trimmed.is_empty() {
                trimmed.parse().ok()
            } else {
                None
            }
        })
        .next()
}

#[cfg(target_os = "macos")]
fn sysctl_u64(name: &str) -> Option<u64> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut value = 0u64;
    let mut size = std::mem::size_of::<u64>();
    let rc = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            &mut value as *mut _ as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && value > 0).then_some(value)
}

#[cfg(target_os = "macos")]
fn sysctl_i32(name: &str) -> Option<i32> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut value = 0i32;
    let mut size = std::mem::size_of::<i32>();
    let rc = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            &mut value as *mut _ as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then_some(value)
}

#[cfg(target_os = "macos")]
fn detect_macos() -> HostMemory {
    let ram = sysctl_u64("hw.memsize").unwrap_or(0);
    if sysctl_i32("hw.optional.arm64") == Some(1) {
        return HostMemory {
            vram: 0,
            ram,
            unified: ram > 0,
        };
    }
    let vram = nvidia_vram().unwrap_or_else(|| {
        command_stdout(
            "system_profiler",
            &["SPDisplaysDataType"],
            Duration::from_secs(8),
        )
        .map(|text| parse_profiler_vram(&text))
        .unwrap_or(0)
    });
    HostMemory {
        vram,
        ram,
        unified: false,
    }
}

#[cfg(target_os = "linux")]
fn detect_linux() -> HostMemory {
    let ram = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let rest = line.strip_prefix("MemTotal:")?;
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                Some(kb.saturating_mul(1024))
            })
        })
        .unwrap_or(0);
    let vram = nvidia_vram().or_else(drm_vram).unwrap_or(0);
    HostMemory {
        vram,
        ram,
        unified: false,
    }
}

#[cfg(target_os = "linux")]
fn drm_vram() -> Option<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir("/sys/class/drm").ok()? {
        let path = entry.ok()?.path().join("device/mem_info_vram_total");
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(bytes) = text.trim().parse::<u64>() {
                total = total.saturating_add(bytes);
            }
        }
    }
    (total > 0).then_some(total)
}

#[cfg(windows)]
fn detect_windows() -> HostMemory {
    let ram = command_stdout(
        "wmic",
        &["ComputerSystem", "get", "TotalPhysicalMemory"],
        Duration::from_secs(3),
    )
    .and_then(|text| first_u64_line(&text))
    .or_else(|| {
        command_stdout(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ],
            Duration::from_secs(5),
        )
        .and_then(|text| first_u64_line(&text))
    })
    .unwrap_or(0);
    HostMemory {
        vram: nvidia_vram().unwrap_or(0),
        ram,
        unified: false,
    }
}

fn pick_default_variant(variants: &[RemoteModelVariant]) -> String {
    let gguf: Vec<_> = variants
        .iter()
        .filter(|variant| variant.id.starts_with("gguf:"))
        .collect();
    if !gguf.is_empty() {
        for pref in QUANT_PREF {
            if let Some(variant) = gguf.iter().find(|variant| {
                variant
                    .label
                    .to_ascii_uppercase()
                    .replace('-', "_")
                    .contains(pref)
            }) {
                return variant.id.clone();
            }
        }
        let mut sorted = gguf;
        sorted.sort_by_key(|variant| variant.size);
        return sorted[sorted.len() / 2].id.clone();
    }
    if let Some(variant) = variants.iter().find(|variant| variant.id == "safetensors") {
        return variant.id.clone();
    }
    if let Some(variant) = variants.iter().find(|variant| variant.id == "pytorch") {
        return variant.id.clone();
    }
    variants
        .iter()
        .max_by_key(|variant| variant.size)
        .map(|variant| variant.id.clone())
        .unwrap_or_default()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn stem_label(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_string()
}

fn is_sidecar(path: &str) -> bool {
    let lower_path = path.to_ascii_lowercase();
    let name = file_name(&lower_path);
    lower_path.starts_with("additional_chat_templates/")
        || name.starts_with("mmproj")
        || name.starts_with("tokenizer")
        || name.starts_with("chat_template")
        || name.ends_with(".tiktoken")
        || name.ends_with(".py")
        || matches!(
            name,
            "config.json"
                | "generation_config.json"
                | "configuration.json"
                | "special_tokens_map.json"
                | "vocab.json"
                | "vocab.txt"
                | "merges.txt"
                | "added_tokens.json"
                | "preprocessor_config.json"
                | "processor_config.json"
                | "video_preprocessor_config.json"
                | "spiece.model"
                | "spm.model"
                | "normalizer.json"
                | "tokenizer.model.v3"
                | "sentencepiece.bpe.model"
                | "sentencepiece.model"
                | "source.spm"
                | "target.spm"
                | "bpe.codes"
                | "vocab.bpe"
                | "vocab-src.json"
                | "vocab-tgt.json"
        )
}

fn is_other_weight(lower: &str) -> bool {
    lower.ends_with(".onnx")
        || lower.ends_with(".pt")
        || lower.ends_with(".pth")
        || lower.ends_with(".msgpack")
        || lower.ends_with(".npz")
        || lower.ends_with(".tflite")
        || lower.ends_with(".ggml")
}

fn gguf_group_id(path: &str) -> String {
    let name = file_name(path);
    if let Some(group) = gguf_shard_group(name) {
        match path.rsplit_once(['/', '\\']) {
            Some((dir, _)) => format!("gguf:{dir}/{group}"),
            None => format!("gguf:{group}"),
        }
    } else {
        format!("gguf:{path}")
    }
}

fn gguf_shard_group(name: &str) -> Option<String> {
    let stem = name
        .strip_suffix(".gguf")
        .or_else(|| name.strip_suffix(".GGUF"))?;
    let (left, total) = stem.rsplit_once("-of-")?;
    total.parse::<u32>().ok()?;
    let (prefix, index) = left.rsplit_once('-')?;
    index.parse::<u32>().ok()?;
    Some(prefix.to_string())
}

fn gguf_display_label(path: &str, name: &str) -> String {
    let mut parts = Vec::new();
    if let Some(category) = gguf_category(path) {
        parts.push(category);
    }
    if let Some(quant) = gguf_quant_label(name) {
        parts.push(quant);
    }
    if let Some(extra) = gguf_extra(name) {
        parts.push(extra);
    }
    if parts.is_empty() {
        stem_label(name)
    } else {
        parts.join(" · ")
    }
}

fn gguf_category(path: &str) -> Option<String> {
    let hay = path.to_ascii_lowercase().replace(['_', '-'], " ");
    if hay.contains("reference") {
        Some("References".into())
    } else if hay.contains("mmproj") || hay.contains("projector") {
        Some("Projector".into())
    } else if hay.contains("frame") || hay.contains("vision") {
        Some("Text & frames".into())
    } else {
        None
    }
}

fn gguf_extra(name: &str) -> Option<String> {
    let hay = name.to_ascii_uppercase().replace('-', "_");
    ["PRUNED", "IMATRIX", "INSTRUCT"]
        .into_iter()
        .find(|token| hay.contains(token))
        .map(|token| format!("{}{}", &token[..1], token[1..].to_ascii_lowercase()))
}

fn gguf_quant_label(name: &str) -> Option<String> {
    let quant = gguf_quant(name)?;
    let hay = name.to_ascii_uppercase().replace('-', "_");
    if hay.contains("UD_") || hay.contains("_UD") || hay.starts_with("UD") {
        Some(format!("UD-{quant}"))
    } else {
        Some(quant)
    }
}

fn gguf_quant(name: &str) -> Option<String> {
    const KNOWN: &[&str] = &[
        "Q4_K_XL", "Q3_K_XL", "Q5_K_XL", "Q6_K_XL", "Q2_K_XL", "IQ2_XXS", "IQ3_XXS", "IQ2_XS",
        "IQ3_XS", "IQ4_XS", "IQ4_NL", "IQ1_S", "IQ2_S", "IQ3_S", "IQ3_M", "Q3_K_S", "Q3_K_M",
        "Q3_K_L", "Q4_K_S", "Q4_K_M", "Q5_K_S", "Q5_K_M", "Q2_K", "Q3_K", "Q4_K", "Q5_K", "Q6_K",
        "Q4_0", "Q4_1", "Q5_0", "Q5_1", "Q8_0", "BF16", "F16", "F32",
    ];
    let needle = name.to_ascii_uppercase().replace('-', "_");
    KNOWN
        .iter()
        .copied()
        .find(|quant| needle.contains(quant))
        .map(str::to_string)
}

pub(crate) fn resolve_url(
    source: ModelSource,
    repo: &str,
    revision: &str,
    file: &str,
) -> Result<reqwest::Url, String> {
    match source {
        ModelSource::HuggingFace => {
            let mut url =
                reqwest::Url::parse("https://huggingface.co").map_err(|error| error.to_string())?;
            {
                let mut segments = url
                    .path_segments_mut()
                    .map_err(|_| "无法构造下载地址".to_string())?;
                for part in repo.split('/') {
                    segments.push(part);
                }
                segments.push("resolve");
                segments.push(revision);
                for part in file.split('/') {
                    segments.push(part);
                }
            }
            Ok(url)
        }
        ModelSource::ModelScope => {
            let mut url = reqwest::Url::parse(&format!(
                "https://www.modelscope.cn/api/v1/models/{repo}/repo"
            ))
            .map_err(|error| error.to_string())?;
            url.query_pairs_mut()
                .append_pair("Revision", revision)
                .append_pair("FilePath", file);
            Ok(url)
        }
    }
}

type ModelCache = HashMap<String, LocalLlm>;

fn list_models(root: &Path, cache: &ModelCache) -> Vec<LocalLlm> {
    scan_locations(root, &hf_hub_cache(), &modelscope_cache(), cache)
}

fn scan_locations(
    app_root: &Path,
    hf_root: &Path,
    ms_root: &Path,
    cache: &ModelCache,
) -> Vec<LocalLlm> {
    thread::scope(|scope| {
        let app = scope.spawn(|| scan_app_models_with(app_root, cache));
        let hf = scope.spawn(|| scan_hf_hub(hf_root, cache));
        let ms = scope.spawn(|| scan_modelscope(ms_root, cache));
        let mut models = app.join().expect("app model scan");
        push_unique(&mut models, hf.join().expect("huggingface scan"));
        push_unique(&mut models, ms.join().expect("modelscope scan"));
        models
    })
}

fn push_unique(models: &mut Vec<LocalLlm>, extra: Vec<LocalLlm>) {
    for model in extra {
        if models.iter().any(|existing| existing.id == model.id) {
            continue;
        }
        models.push(model);
    }
}

fn hf_hub_cache() -> PathBuf {
    if let Ok(path) = std::env::var("HF_HUB_CACHE") {
        return PathBuf::from(path);
    }
    if let Ok(home) = std::env::var("HF_HOME") {
        return PathBuf::from(home).join("hub");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("huggingface")
        .join("hub")
}

fn modelscope_cache() -> PathBuf {
    if let Ok(path) = std::env::var("MODELSCOPE_CACHE") {
        let path = PathBuf::from(path);
        let hub = path.join("hub");
        return if hub.is_dir() { hub } else { path };
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("modelscope")
        .join("hub")
}

pub(crate) fn source_cache_root(source: ModelSource) -> PathBuf {
    match source {
        ModelSource::HuggingFace => hf_hub_cache(),
        ModelSource::ModelScope => modelscope_cache(),
    }
}

pub(crate) fn source_cache_model_dir(source: ModelSource, repo: &str) -> Result<PathBuf, String> {
    let (owner, name) = repo.split_once('/').ok_or("模型 ID 无效")?;
    if owner.is_empty() || name.is_empty() || owner == "." || owner == ".." || name == "." || name == ".."
        || owner.contains(['/', '\\']) || name.contains(['/', '\\']) {
        return Err("模型 ID 无效".into());
    }
    Ok(match source {
        ModelSource::HuggingFace => source_cache_root(source).join(format!("models--{owner}--{name}")),
        ModelSource::ModelScope => source_cache_root(source).join(owner).join(name),
    })
}

pub(crate) fn copy_tree_without_links(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let kind = fs::symlink_metadata(&from).map_err(|e| e.to_string())?;
        if kind.file_type().is_symlink() {
            let real = fs::canonicalize(&from).map_err(|e| e.to_string())?;
            if fs::hard_link(&real, &to).is_err() {
                fs::copy(real, &to).map_err(|e| e.to_string())?;
            }
        } else if kind.is_dir() {
            copy_tree_without_links(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn migrate_directory(source_path: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err("目标模型目录已存在，拒绝覆盖".into());
    }
    if let Some(parent) = destination.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let has_links = fn_has_links(source_path)?;
    if !has_links {
        fs::rename(source_path, destination).map_err(|e| e.to_string())?;
    } else {
        copy_tree_without_links(source_path, destination)?;
        fs::remove_dir_all(source_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn fn_has_links(path: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let child = entry.path();
        let meta = fs::symlink_metadata(&child).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() || (meta.is_dir() && fn_has_links(&child)?) { return Ok(true); }
    }
    Ok(false)
}

#[cfg(test)]
fn scan_app_models(root: &Path) -> Vec<LocalLlm> {
    scan_app_models_with(root, &ModelCache::new())
}

fn scan_app_models_with(root: &Path, cache: &ModelCache) -> Vec<LocalLlm> {
    let mut models = Vec::new();
    if !root.exists() {
        return models;
    }
    for source in [ModelSource::HuggingFace, ModelSource::ModelScope] {
        let dir = root.join(source.as_str());
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry_is_dir(&entry) {
                continue;
            }
            let path = entry.path();
            if let Some(repo) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(repo_from_dir_name)
            {
                if let Some(model) = reuse_cached(cache, &model_id(source, &repo), &path) {
                    models.push(model);
                    continue;
                }
            }
            if let Some(model) =
                read_model(source, &path, cache).or_else(|| infer_app_model(source, &path, cache))
            {
                models.push(model);
            }
        }
    }
    models
}

fn infer_app_model(source: ModelSource, path: &Path, cache: &ModelCache) -> Option<LocalLlm> {
    let name = path.file_name()?.to_str()?;
    let repo = repo_from_dir_name(name)?;
    assemble_model(
        source,
        repo,
        source.default_revision().into(),
        path,
        path,
        cache,
    )
}

pub(crate) fn repo_from_dir_name(name: &str) -> Option<String> {
    let (owner, rest) = name.split_once("__")?;
    if owner.is_empty() || rest.is_empty() {
        return None;
    }
    Some(format!("{owner}/{rest}"))
}

fn repo_from_hf_cache_dir(name: &str) -> Option<String> {
    let rest = name.strip_prefix("models--")?;
    let (owner, model) = rest.split_once("--")?;
    if owner.is_empty() || model.is_empty() {
        return None;
    }
    Some(format!("{owner}/{model}"))
}

fn scan_hf_hub(root: &Path, cache: &ModelCache) -> Vec<LocalLlm> {
    let mut models = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return models;
    };
    for entry in entries.flatten() {
        if !entry_is_dir(&entry) {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(repo) = repo_from_hf_cache_dir(name) else {
            continue;
        };
        if let Some(model) = reuse_cached(cache, &model_id(ModelSource::HuggingFace, &repo), &path)
        {
            models.push(model);
            continue;
        }
        let Some((revision, content)) = hf_snapshot(&path) else {
            continue;
        };
        if let Some(model) = assemble_model(
            ModelSource::HuggingFace,
            repo,
            revision,
            &content,
            &path,
            cache,
        ) {
            models.push(model);
        }
    }
    models
}

fn hf_snapshot(model_dir: &Path) -> Option<(String, PathBuf)> {
    let refs = model_dir.join("refs");
    for preferred in ["main", "master"] {
        if let Some(found) = snapshot_from_ref(model_dir, &refs.join(preferred), preferred) {
            return Some(found);
        }
    }
    if let Ok(entries) = fs::read_dir(&refs) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let revision = name.to_string_lossy();
            if let Some(found) = snapshot_from_ref(model_dir, &entry.path(), &revision) {
                return Some(found);
            }
        }
    }
    let snapshots = model_dir.join("snapshots");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(&snapshots).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let modified = entry.metadata().ok()?.modified().ok()?;
        if best
            .as_ref()
            .map(|(time, _)| modified > *time)
            .unwrap_or(true)
        {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| {
        let revision = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main".into());
        (revision, path)
    })
}

fn snapshot_from_ref(
    model_dir: &Path,
    ref_path: &Path,
    revision: &str,
) -> Option<(String, PathBuf)> {
    let hash = fs::read_to_string(ref_path).ok()?;
    let snapshot = model_dir.join("snapshots").join(hash.trim());
    snapshot.is_dir().then(|| (revision.to_string(), snapshot))
}

fn scan_modelscope(root: &Path, cache: &ModelCache) -> Vec<LocalLlm> {
    let mut models = Vec::new();
    collect_owner_name_models(root, cache, &mut models);
    collect_owner_name_models(&root.join("models"), cache, &mut models);
    models
}

fn collect_owner_name_models(root: &Path, cache: &ModelCache, models: &mut Vec<LocalLlm>) {
    let Ok(owners) = fs::read_dir(root) else {
        return;
    };
    for owner in owners.flatten() {
        if !entry_is_dir(&owner) {
            continue;
        }
        let owner_path = owner.path();
        let Some(owner_name) = owner_path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if owner_name.starts_with('.') || owner_name == "models" {
            continue;
        }
        let Ok(names) = fs::read_dir(&owner_path) else {
            continue;
        };
        for name in names.flatten() {
            if !entry_is_dir(&name) {
                continue;
            }
            let path = name.path();
            let Some(model_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if model_name.starts_with('.') {
                continue;
            }
            let repo = format!("{owner_name}/{model_name}");
            if let Some(model) =
                reuse_cached(cache, &model_id(ModelSource::ModelScope, &repo), &path)
            {
                models.push(model);
                continue;
            }
            let (revision, content) = modelscope_revision(&path);
            if let Some(model) = assemble_model(
                ModelSource::ModelScope,
                repo,
                revision,
                &content,
                &path,
                cache,
            ) {
                models.push(model);
            }
        }
    }
}

fn modelscope_revision(path: &Path) -> (String, PathBuf) {
    for preferred in ["master", "main"] {
        let nested = path.join(preferred);
        if nested.is_dir() && dir_nonempty(&nested) {
            return (preferred.into(), nested);
        }
    }
    (
        ModelSource::ModelScope.default_revision().into(),
        path.to_path_buf(),
    )
}

fn reuse_cached(cache: &ModelCache, id: &str, listed_path: &Path) -> Option<LocalLlm> {
    let cached = cache.get(id)?;
    if cached.files == 0 || cached.path != listed_path.to_string_lossy() {
        return None;
    }
    Some(cached.clone())
}

fn entry_is_dir(entry: &fs::DirEntry) -> bool {
    entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
}

fn assemble_model(
    source: ModelSource,
    repo: String,
    revision: String,
    content: &Path,
    listed_path: &Path,
    cache: &ModelCache,
) -> Option<LocalLlm> {
    let id = model_id(source, &repo);
    if let Some(cached) = reuse_cached(cache, &id, listed_path) {
        return Some(cached);
    }
    let path = listed_path.to_string_lossy().into_owned();
    let (size, files) = manifest_stats(content).unwrap_or_else(|| dir_stats(content));
    if files == 0 {
        return None;
    }
    Some(LocalLlm {
        id,
        source,
        repo,
        revision,
        path,
        size,
        files,
    })
}

fn manifest_stats(path: &Path) -> Option<(u64, u32)> {
    let bytes = fs::read(path.join("manifest.json")).ok()?;
    let manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
    if manifest.files.is_empty() {
        return None;
    }
    Some((
        manifest.files.iter().map(|file| file.size).sum(),
        manifest.files.len() as u32,
    ))
}

fn dir_nonempty(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        !is_junk(&name.to_string_lossy())
    })
}

fn is_junk(name: &str) -> bool {
    name == ".DS_Store"
        || name == "manifest.json"
        || name == "blobs"
        || name == "refs"
        || name.starts_with('.')
        || name.ends_with(".part")
        || name.ends_with(".chunks")
        || name.ends_with(".incomplete")
        || name.ends_with(".lock")
}

fn read_model(_source: ModelSource, path: &Path, cache: &ModelCache) -> Option<LocalLlm> {
    let bytes = fs::read(path.join("manifest.json")).ok()?;
    let manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
    assemble_model(
        manifest.source,
        manifest.repo,
        manifest.revision,
        path,
        path,
        cache,
    )
}

fn dir_stats(path: &Path) -> (u64, u32) {
    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0);
    };
    let mut size = 0;
    let mut files = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_junk(&name) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let child = entry.path();
        if kind.is_dir() {
            let (nested_size, nested_files) = dir_stats(&child);
            size += nested_size;
            files += nested_files;
            continue;
        }
        let Ok(meta) = fs::metadata(&child) else {
            continue;
        };
        if meta.is_dir() {
            let (nested_size, nested_files) = dir_stats(&child);
            size += nested_size;
            files += nested_files;
        } else {
            size += meta.len();
            files += 1;
        }
    }
    (size, files)
}

#[cfg(test)]
fn resolve_local(root: &Path, id: &str) -> Result<PathBuf, String> {
    let (source, rest) = id.split_once('/').ok_or("模型 ID 无效")?;
    let source = match source {
        "huggingface" => ModelSource::HuggingFace,
        "modelscope" => ModelSource::ModelScope,
        _ => return Err("模型 ID 无效".into()),
    };
    if rest.contains("..") || rest.contains('/') || rest.contains('\\') {
        return Err("模型 ID 无效".into());
    }
    let path = root.join(source.as_str()).join(rest);
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let canonical = path.canonicalize().map_err(|_| "模型不存在".to_string())?;
    if !canonical.starts_with(&canonical_root) {
        return Err("模型 ID 无效".into());
    }
    Ok(canonical)
}

fn open_dir(path: &Path) -> Result<(), String> {
    let status = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(path).status()
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer").arg(path).status()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            std::process::Command::new("xdg-open").arg(path).status()
        }
    };
    status
        .map_err(|error| error.to_string())
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err("无法打开目录".into())
            }
        })
}

#[tauri::command]
pub(crate) fn probe_remote_model(
    source: ModelSource,
    repo: String,
    revision: Option<String>,
    state: State<'_, AppState>,
) -> Result<RemoteModelProbe, String> {
    bind_stored_hf_token(&state);
    probe(source, &repo, revision.as_deref())
}

#[tauri::command]
pub(crate) async fn search_remote_models(
    source: ModelSource,
    query: String,
    format: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<RemoteModelHit>, String> {
    bind_stored_hf_token(&state);
    let query = query.trim().to_string();
    if query.is_empty() { return Err("请输入搜索关键词".into()); }
    let format = format.unwrap_or_else(|| "all".into());
    tauri::async_runtime::spawn_blocking(move || {
        // Exact lookup and ranked search hit independent endpoints; run them
        // concurrently so latency is bounded by the slower request.
        std::thread::scope(|scope| {
            let exact = scope.spawn(|| lookup_exact(source, &query));
            let searched = scope.spawn(|| match source {
                ModelSource::HuggingFace => search_huggingface(&query, &format),
                ModelSource::ModelScope => search_modelscope(&query, &format),
            }).join().map_err(|_| "搜索线程异常退出".to_string())?;
            let exact = exact.join().map_err(|_| "搜索线程异常退出".to_string())?;
            match (exact, searched) {
                (Some(exact), Ok(hits)) => Ok(merge_exact(Some(exact), hits)),
                (Some(exact), Err(_)) => Ok(vec![exact]),
                (None, Ok(hits)) => Ok(hits),
                (None, Err(error)) => Err(error),
            }
        })
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn list_local_models(state: State<'_, AppState>) -> Result<Vec<LocalLlm>, String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .listed_local_models()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn refresh_local_models(
    app: AppHandle,
    _state: State<'_, AppState>,
) -> Result<Vec<LocalLlm>, String> {
    let root = models_root(&app)?;
    let seen = REFRESH_EPOCH.load(Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || refresh_models_blocking(app, root, seen))
        .await
        .map_err(|error| error.to_string())?
}

fn refresh_models_blocking(
    app: AppHandle,
    root: PathBuf,
    seen: u64,
) -> Result<Vec<LocalLlm>, String> {
    let _gate = REFRESH_LOCK
        .lock()
        .map_err(|_| "模型刷新被中断".to_string())?;
    let state = app.state::<AppState>();
    if REFRESH_EPOCH.load(Ordering::SeqCst) != seen {
        return listed_from(&*state);
    }
    let cache: ModelCache = {
        let controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
        controller
            .listed_local_models()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|model| (model.id.clone(), model))
            .collect()
    };
    let scanned = list_models(&root, &cache);
    let mut controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
    controller
        .sync_local_models(&scanned)
        .map_err(|error| error.to_string())?;
    REFRESH_EPOCH.fetch_add(1, Ordering::SeqCst);
    controller
        .listed_local_models()
        .map_err(|error| error.to_string())
}

fn listed_from(state: &AppState) -> Result<Vec<LocalLlm>, String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .listed_local_models()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn reorder_local_models(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "账户存储不可用".to_string())?
        .reorder_local_models(ids)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn delete_local_model(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let model = {
        let controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
        controller
            .local_model(&id)
            .map_err(|error| error.to_string())?
    };
    crate::download::cancel_jobs_for_repo(&app, model.source, &model.repo);
    let path = PathBuf::from(&model.path);
    if path.exists() {
        fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
    }
    let mut controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
    controller
        .delete_local_model_row(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn open_local_model_dir(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let path = {
        let controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
        controller
            .local_model(&id)
            .map_err(|error| error.to_string())?
            .path
    };
    open_dir(Path::new(&path))
}

#[tauri::command]
pub(crate) fn migrate_local_model(
    state: State<'_, AppState>,
    id: String,
    target: ModelSource,
) -> Result<(), String> {
    let model = {
        let controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
        controller.local_model(&id).map_err(|error| error.to_string())?
    };
    let source_path = PathBuf::from(&model.path);
    if !source_path.is_dir() { return Err("模型目录不存在或尚未完成下载".into()); }
    let content = if model.source == ModelSource::HuggingFace {
        hf_snapshot(&source_path).map(|(_, path)| path).unwrap_or_else(|| source_path.clone())
    } else {
        let (_, path) = modelscope_revision(&source_path);
        path
    };
    let target_root = source_cache_model_dir(target, &model.repo)?;
    if target == ModelSource::HuggingFace {
        let revision = model.revision.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect::<String>();
        let revision = if revision.is_empty() { "main".into() } else { revision };
        let snapshot = target_root.join("snapshots").join(&revision);
        migrate_directory(&content, &snapshot)?;
        fs::create_dir_all(target_root.join("refs")).map_err(|e| e.to_string())?;
        fs::write(target_root.join("refs").join("main"), &revision).map_err(|e| e.to_string())?;
    } else {
        migrate_directory(&content, &target_root)?;
    }
    if content != source_path && source_path.exists() { let _ = fs::remove_dir_all(&source_path); }
    let new_model = LocalLlm { id: model_id(target, &model.repo), source: target, repo: model.repo.clone(), revision: if target == ModelSource::HuggingFace { "main".into() } else { "master".into() }, path: target_root.to_string_lossy().into_owned(), size: model.size, files: model.files };
    let mut controller = state.0.lock().map_err(|_| "账户存储不可用".to_string())?;
    controller.delete_local_model_row(&id).map_err(|error| error.to_string())?;
    controller.upsert_local_model(&new_model).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "storm-dock-models-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn remote_file(path: &str, size: u64) -> RemoteModelFile {
        RemoteModelFile {
            path: path.into(),
            size,
            sha256: None,
            revision: None,
        }
    }

    #[test]
    fn groups_gguf_quants_and_defaults_q4_k_m() {
        let files = vec![
            remote_file("README.md", 100),
            remote_file("config.json", 10),
            remote_file("tokenizer.json", 20),
            remote_file("model-Q5_K_M.gguf", 5000),
            remote_file("model-Q4_K_M.gguf", 4000),
            remote_file("model-Q8_0.gguf", 8000),
        ];
        let (variants, default) = group_weight_variants(&files);
        assert_eq!(variants.len(), 3);
        assert_eq!(default, "gguf:model-Q4_K_M.gguf");
        let q4 = variants
            .iter()
            .find(|variant| variant.label == "Q4_K_M")
            .unwrap();
        assert!(q4.files.contains(&"model-Q4_K_M.gguf".into()));
        assert!(q4.files.contains(&"config.json".into()));
        assert!(q4.files.contains(&"tokenizer.json".into()));
        assert!(!q4.files.iter().any(|path| path == "README.md"));
        assert!(!q4.files.iter().any(|path| path.contains("Q8_0")));
    }

    #[test]
    fn safetensors_shards_are_one_variant_without_readme() {
        let files = vec![
            remote_file("model-00001-of-00002.safetensors", 100),
            remote_file("model-00002-of-00002.safetensors", 100),
            remote_file("model.safetensors.index.json", 5),
            remote_file("config.json", 1),
            remote_file("LICENSE", 2),
        ];
        let (variants, default) = group_weight_variants(&files);
        assert_eq!(variants.len(), 1);
        assert_eq!(default, "safetensors");
        assert_eq!(variants[0].files.len(), 4);
        assert!(!variants[0].files.iter().any(|path| path == "LICENSE"));
    }

    #[test]
    fn transformers_auxiliary_files_follow_each_variant() {
        for path in [
            "tokenizer_config.json",
            "processor_config.json",
            "video_preprocessor_config.json",
            "spiece.model",
            "sentencepiece.bpe.model",
            "tokenizer.tiktoken",
            "custom_modeling.py",
            "additional_chat_templates/default.jinja",
        ] {
            assert!(is_sidecar(path), "expected auxiliary file: {path}");
        }
        assert!(!is_sidecar("README.md"));
    }

    #[test]
    fn sharded_gguf_is_one_variant() {
        let files = vec![
            remote_file("x-Q4_K_M-00001-of-00002.gguf", 10),
            remote_file("x-Q4_K_M-00002-of-00002.gguf", 10),
        ];
        let (variants, default) = group_weight_variants(&files);
        assert_eq!(variants.len(), 1);
        assert_eq!(default, "gguf:x-Q4_K_M");
        assert_eq!(variants[0].files.len(), 2);
        assert_eq!(variants[0].label, "Q4_K_M");
    }

    #[test]
    fn gguf_label_includes_role_quant_and_pruned() {
        assert_eq!(
            gguf_display_label(
                "vision/model-UD-Q4_K-Pruned.gguf",
                "model-UD-Q4_K-Pruned.gguf"
            ),
            "Text & frames · UD-Q4_K · Pruned"
        );
        assert_eq!(
            gguf_display_label("references/model-Q3_K.gguf", "model-Q3_K.gguf"),
            "References · Q3_K"
        );
    }

    #[test]
    fn card_from_hf_reads_intro_fields() {
        let meta = serde_json::json!({
            "author": "unsloth",
            "likes": 37,
            "downloads": 12,
            "pipeline_tag": "image-text-to-text",
            "library_name": "transformers",
            "lastModified": "2026-08-31T00:00:00.000Z",
            "tags": ["gguf", "conversational", "license:mit"],
            "cardData": {
                "pretty_name": "DeepSeek-V4-Flash-Vision-Exp-GGUF",
                "license": "mit",
                "base_model": "deepseek-ai/DeepSeek-V4-Flash-Vision-Exp",
                "model_summary": "Unsloth Dynamic 2.0."
            },
            "safetensors": { "total": 284300000000_i64 }
        });
        let card = card_from_hf(&meta, "unsloth/DeepSeek-V4-Flash-Vision-Exp-GGUF");
        assert_eq!(card.author, "unsloth");
        assert_eq!(card.name, "DeepSeek-V4-Flash-Vision-Exp-GGUF");
        assert_eq!(card.license.as_deref(), Some("mit"));
        assert_eq!(
            card.base_model.as_deref(),
            Some("deepseek-ai/DeepSeek-V4-Flash-Vision-Exp")
        );
        assert_eq!(card.description, "Unsloth Dynamic 2.0.");
        assert_eq!(card.likes, Some(37));
        assert!(card
            .tags
            .iter()
            .any(|tag| tag.to_ascii_lowercase().contains("conversational")));
        assert!(card.params.as_deref().unwrap().contains('B'));
    }

    #[test]
    fn params_from_label_reads_compact_size_tags() {
        assert_eq!(params_from_label("20b").as_deref(), Some("20B"));
        assert_eq!(params_from_label("1.5B").as_deref(), Some("1.5B"));
        assert_eq!(params_from_label("gguf"), None);
        assert_eq!(format_params(20_000_000_000), "20B");
        assert_eq!(format_params(284_300_000_000), "284.3B");
    }

    #[test]
    fn first_readme_paragraph_skips_frontmatter_and_badges() {
        let text = "---\nlicense: mit\n---\n\n# Title\n\n![badge](x.png)\n\nUnsloth Dynamic 2.0 achieves superior accuracy.\n\nMore.";
        assert_eq!(
            first_readme_paragraph(text),
            "Unsloth Dynamic 2.0 achieves superior accuracy."
        );
    }

    #[test]
    fn parse_repo_id_accepts_owner_name_and_urls() {
        assert_eq!(parse_repo_id("Qwen/Qwen2.5-7B").unwrap(), "Qwen/Qwen2.5-7B");
        assert_eq!(
            parse_repo_id(" https://huggingface.co/Qwen/Qwen2.5-7B ").unwrap(),
            "Qwen/Qwen2.5-7B"
        );
        assert_eq!(
            parse_repo_id("https://www.modelscope.cn/models/qwen/Qwen2.5-7B").unwrap(),
            "qwen/Qwen2.5-7B"
        );
        assert!(parse_repo_id("../etc/passwd").is_err());
        assert!(parse_repo_id("onlyone").is_err());
        assert!(parse_repo_id("a/b/c").is_err());
        assert!(parse_repo_id("Qwen/..").is_err());
    }

    #[test]
    fn safe_file_path_rejects_traversal() {
        assert_eq!(
            safe_file_path("tokenizer/vocab.json").unwrap(),
            PathBuf::from("tokenizer/vocab.json")
        );
        assert!(safe_file_path("../secret").is_err());
        assert!(safe_file_path("/etc/passwd").is_err());
        assert!(safe_file_path("").is_err());
    }

    #[test]
    fn plan_chunks_covers_exact_and_remainder_sizes() {
        assert_eq!(plan_chunks(0, CHUNK_SIZE), Vec::<(u64, u64)>::new());
        assert_eq!(
            plan_chunks(CHUNK_SIZE, CHUNK_SIZE),
            vec![(0, CHUNK_SIZE - 1)]
        );
        assert_eq!(
            plan_chunks(CHUNK_SIZE + 8 * 1024 * 1024, CHUNK_SIZE),
            vec![
                (0, CHUNK_SIZE - 1),
                (CHUNK_SIZE, CHUNK_SIZE + 8 * 1024 * 1024 - 1)
            ]
        );
    }

    #[test]
    fn same_origin_ignores_path() {
        let origin: reqwest::Url = "https://huggingface.co/a/b/resolve/main/x".parse().unwrap();
        let same: reqwest::Url = "https://huggingface.co/c/d".parse().unwrap();
        let cdn: reqwest::Url = "https://cdn-lfs.huggingface.co/x".parse().unwrap();
        assert!(same_origin(&origin, &same));
        assert!(!same_origin(&origin, &cdn));
        let headers = download_headers(ModelSource::HuggingFace, &cdn, &origin);
        assert!(!headers.contains_key(reqwest::header::AUTHORIZATION));
        assert!(headers.contains_key(USER_AGENT));
    }

    #[test]
    fn cdn_expired_retries_auth_failures() {
        assert!(cdn_expired(StatusCode::FORBIDDEN));
        assert!(cdn_expired(StatusCode::UNAUTHORIZED));
        assert!(!cdn_expired(StatusCode::NOT_FOUND));
        assert!(!cdn_expired(StatusCode::PARTIAL_CONTENT));
    }

    #[test]
    fn is_complete_matches_size() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("weights.bin");
        fs::write(&path, [0_u8; 8]).unwrap();
        assert!(is_complete(&path, 8));
        assert!(!is_complete(&path, 16));
        assert!(!is_complete(&dir.join("missing.bin"), 8));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_error_maps_missing_and_gated_models() {
        assert_eq!(
            status_error(ModelSource::HuggingFace, StatusCode::NOT_FOUND),
            "模型不存在"
        );
        assert!(status_error(ModelSource::HuggingFace, StatusCode::UNAUTHORIZED).contains("设置"));
        assert!(status_error(ModelSource::ModelScope, StatusCode::FORBIDDEN)
            .contains("MODELSCOPE_TOKEN"));
    }

    #[test]
    fn list_models_reads_manifests_and_delete_stays_inside_root() {
        let root = temp_dir();
        let model = root.join("huggingface").join("Qwen__Tiny");
        fs::create_dir_all(&model).unwrap();
        fs::write(
            model.join("manifest.json"),
            r#"{"source":"huggingface","repo":"Qwen/Tiny","revision":"main","files":[{"path":"config.json","size":2}]}"#,
        )
        .unwrap();
        fs::write(model.join("config.json"), "{}").unwrap();
        let listed = scan_app_models(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].repo, "Qwen/Tiny");
        assert_eq!(listed[0].id, "huggingface/Qwen__Tiny");
        let deleted = resolve_local(&root, "huggingface/Qwen__Tiny").unwrap();
        assert_eq!(deleted, model.canonicalize().unwrap());
        assert!(resolve_local(&root, "huggingface/../secret").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn link_next_reads_rfc_header() {
        assert_eq!(
            link_next(Some(
                r#"<https://huggingface.co/api/next>; rel="next", <https://huggingface.co/api/last>; rel="last""#
            )),
            Some("https://huggingface.co/api/next".into())
        );
        assert_eq!(link_next(Some(r#"<https://example>; rel="last""#)), None);
    }

    #[test]
    fn scans_huggingface_and_modelscope_cache_layouts() {
        let root = temp_dir();
        let hf = root.join("hub");
        let snapshot = hf
            .join("models--Qwen--Tiny")
            .join("snapshots")
            .join("abc123");
        fs::create_dir_all(&snapshot).unwrap();
        fs::create_dir_all(hf.join("models--Qwen--Tiny").join("refs")).unwrap();
        fs::write(
            hf.join("models--Qwen--Tiny").join("refs").join("main"),
            "abc123\n",
        )
        .unwrap();
        fs::write(snapshot.join("config.json"), "{}").unwrap();
        fs::write(snapshot.join("model.safetensors"), [0_u8; 8]).unwrap();

        let ms = root.join("ms");
        fs::create_dir_all(ms.join("qwen").join("Qwen2")).unwrap();
        fs::write(ms.join("qwen").join("Qwen2").join("config.json"), "{}").unwrap();

        let app = root.join("app");
        fs::create_dir_all(app.join("huggingface").join("Local__Weights")).unwrap();
        fs::write(
            app.join("huggingface")
                .join("Local__Weights")
                .join("weights.bin"),
            [1_u8; 4],
        )
        .unwrap();

        let listed = scan_locations(&app, &hf, &ms, &ModelCache::new());
        assert!(listed
            .iter()
            .any(|model| model.repo == "Qwen/Tiny" && model.source == ModelSource::HuggingFace));
        assert!(listed
            .iter()
            .any(|model| model.repo == "qwen/Qwen2" && model.source == ModelSource::ModelScope));
        assert!(listed.iter().any(|model| model.repo == "Local/Weights"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn syncs_scanned_models_into_sqlite() {
        use crate::store::Controller;
        let root = temp_dir();
        let data_dir = root.join("data");
        let model_dir = root.join("huggingface").join("Qwen__Tiny");
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("config.json"), "{}").unwrap();
        let mut controller = Controller::new(data_dir).unwrap();
        let scanned = scan_app_models(&root);
        controller.sync_local_models(&scanned).unwrap();
        let listed = controller.listed_local_models().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].repo, "Qwen/Tiny");
        assert_eq!(listed[0].path, model_dir.to_string_lossy());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reorder_survives_rescan() {
        use crate::store::Controller;
        let root = temp_dir();
        let data_dir = root.join("data");
        let first = root.join("huggingface").join("Aaa__One");
        let second = root.join("huggingface").join("Zzz__Two");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("config.json"), "{}").unwrap();
        fs::write(second.join("config.json"), "{}").unwrap();
        let mut controller = Controller::new(data_dir).unwrap();
        let scanned = scan_app_models(&root);
        controller.sync_local_models(&scanned).unwrap();
        let listed = controller.listed_local_models().unwrap();
        assert_eq!(listed.len(), 2);
        let reversed: Vec<_> = listed.iter().rev().map(|model| model.id.clone()).collect();
        controller.reorder_local_models(reversed.clone()).unwrap();
        assert_eq!(
            controller
                .listed_local_models()
                .unwrap()
                .iter()
                .map(|model| model.id.clone())
                .collect::<Vec<_>>(),
            reversed
        );
        controller.sync_local_models(&scanned).unwrap();
        assert_eq!(
            controller
                .listed_local_models()
                .unwrap()
                .iter()
                .map(|model| model.id.clone())
                .collect::<Vec<_>>(),
            reversed
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rescan_reuses_cached_rows_and_still_discovers_new_models() {
        let root = temp_dir();
        let first = root.join("huggingface").join("Aaa__One");
        fs::create_dir_all(&first).unwrap();
        fs::write(first.join("a.bin"), [0_u8; 4]).unwrap();
        let initial = scan_app_models(&root);
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].size, 4);
        fs::write(first.join("b.bin"), [0_u8; 100]).unwrap();
        let cache: ModelCache = initial
            .iter()
            .cloned()
            .map(|model| (model.id.clone(), model))
            .collect();
        let reused = scan_app_models_with(&root, &cache);
        assert_eq!(reused.len(), 1);
        assert_eq!(reused[0].size, 4);
        let second = root.join("huggingface").join("Bbb__Two");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("c.bin"), [0_u8; 8]).unwrap();
        let discovered = scan_app_models_with(&root, &cache);
        assert_eq!(discovered.len(), 2);
        assert!(discovered
            .iter()
            .any(|model| model.repo == "Bbb/Two" && model.size == 8));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn classify_discrete_gpu_uses_last_gb_and_half_ram() {
        let vram = 24 * GIB;
        let ram = 32 * GIB;
        assert_eq!(
            classify_model_fit(20 * GIB, vram, ram, false),
            ModelFit::Fits
        );
        assert_eq!(
            classify_model_fit(23 * GIB + 1, vram, ram, false),
            ModelFit::Marginal
        );
        assert_eq!(
            classify_model_fit(30 * GIB, vram, ram, false),
            ModelFit::Partial
        );
        assert_eq!(
            classify_model_fit(50 * GIB, vram, ram, false),
            ModelFit::Oom
        );
    }

    #[test]
    fn classify_unified_memory_does_not_double_count_ram() {
        let ram = 32 * GIB;
        assert_eq!(classify_model_fit(20 * GIB, 0, ram, true), ModelFit::Fits);
        assert_eq!(
            classify_model_fit(23 * GIB + 1, 0, ram, true),
            ModelFit::Marginal
        );
        assert_eq!(
            classify_model_fit(28 * GIB, 0, ram, true),
            ModelFit::Partial
        );
        assert_eq!(classify_model_fit(36 * GIB, 0, ram, true), ModelFit::Oom);
        assert_eq!(
            classify_model_fit(36 * GIB, 24 * GIB, ram, false),
            ModelFit::Partial
        );
    }

    #[test]
    fn classify_cpu_only_and_unknown_host() {
        assert_eq!(
            classify_model_fit(8 * GIB, 0, 32 * GIB, false),
            ModelFit::Ram
        );
        assert_eq!(
            classify_model_fit(20 * GIB, 0, 32 * GIB, false),
            ModelFit::Oom
        );
        assert_eq!(classify_model_fit(8 * GIB, 0, 0, false), ModelFit::Unknown);
    }

    #[test]
    fn parse_system_profiler_vram_total_lines() {
        let text = "Chipset Model: AMD\n        VRAM (Total): 16 GB\n        VRAM (Dynamic, Max): 48 GB\nChipset Model: Intel\n        VRAM (Total): 1536 MB\n";
        assert_eq!(parse_profiler_vram(text), 16 * GIB + 1536 * 1024 * 1024);
        assert_eq!(
            first_u64_line("TotalPhysicalMemory\n\n34251726848\n"),
            Some(34251726848)
        );
    }

    #[test]
    fn remaining_download_skips_complete_and_counts_partial() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.bin"), [0_u8; 8]).unwrap();
        fs::write(dir.join("b.bin.part"), [0_u8; 3]).unwrap();
        let files = vec![
            remote_file("a.bin", 8),
            remote_file("b.bin", 10),
            remote_file("c.bin", 5),
        ];
        assert_eq!(remaining_download_bytes(&dir, &files), 12);
        fs::write(dir.join("d.bin.part"), vec![0_u8; 64]).unwrap();
        fs::write(dir.join("d.bin.chunks"), "[0]").unwrap();
        let with_chunks = vec![remote_file("d.bin", CHUNK_SIZE + 32)];
        assert_eq!(file_have_bytes(&dir, &with_chunks[0]), CHUNK_SIZE);
        assert_eq!(remaining_download_bytes(&dir, &with_chunks), 32);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn disk_space_shortfall_allows_unknown_and_blocks_tight_disks() {
        assert_eq!(disk_space_shortfall(10, None, 5), None);
        assert_eq!(disk_space_shortfall(0, Some(1), 5), None);
        assert_eq!(disk_space_shortfall(10, Some(20), 5), None);
        assert_eq!(disk_space_shortfall(10, Some(14), 5), Some((10, 14)));
    }

    #[test]
    fn hf_format_filter_maps_known_values() {
        assert_eq!(hf_format_filter("gguf"), Some("gguf"));
        assert_eq!(hf_format_filter("safetensors"), Some("safetensors"));
        assert_eq!(hf_format_filter("mlx"), Some("mlx"));
        assert_eq!(hf_format_filter("finetune"), Some("peft"));
        assert_eq!(hf_format_filter("all"), None);
    }

    #[test]
    fn hits_from_hf_json_read_repo_and_stats() {
        let body = serde_json::json!([
            {
                "id": "Qwen/Qwen2.5-7B-Instruct",
                "author": "Qwen",
                "downloads": 12,
                "likes": 3,
                "library_name": "transformers",
                "pipeline_tag": "image-text-to-text",
                "lastModified": "2026-08-15T00:00:00.000Z",
                "safetensors": { "total": 20_000_000_000_u64 },
                "tags": ["gguf", "qwen", "20b"]
            }
        ]);
        let hits = hits_from_hf_value(&body, ModelSource::HuggingFace);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].repo, "Qwen/Qwen2.5-7B-Instruct");
        assert_eq!(hits[0].downloads, Some(12));
        assert_eq!(hits[0].library.as_deref(), Some("transformers"));
        assert_eq!(hits[0].params.as_deref(), Some("20B"));
        assert_eq!(
            hits[0].updated_at.as_deref(),
            Some("2026-08-15T00:00:00.000Z")
        );
    }

    #[test]
    fn hits_from_modelscope_json_filter_by_format_tags() {
        let body = serde_json::json!({
            "Data": {
                "Model": {
                    "Models": [
                        { "Path": "qwen/A", "Downloads": 1, "Tags": ["gguf"] },
                        { "Path": "qwen/B", "Downloads": 2, "Libraries": ["safetensors"] }
                    ]
                }
            }
        });
        let gguf = hits_from_modelscope_value(&body, "gguf");
        assert_eq!(gguf.len(), 1);
        assert_eq!(gguf[0].repo, "qwen/A");
        let all = hits_from_modelscope_value(&body, "all");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn hits_from_modelscope_openapi_reads_id_and_gguf_tags() {
        let body = serde_json::json!({
            "success": true,
            "data": {
                "models": [
                    {
                        "id": "unsloth/MiniMax-H3-GGUF",
                        "display_name": "MiniMax-H3-GGUF",
                        "downloads": 51497,
                        "likes": 17,
                        "tags": ["library:gguf", "custom_tag:unsloth"]
                    },
                    {
                        "id": "qwen/Qwen2.5-7B",
                        "display_name": "Qwen2.5-7B",
                        "tags": ["library:safetensors"]
                    }
                ]
            }
        });
        let gguf = hits_from_modelscope_value(&body, "gguf");
        assert_eq!(gguf.len(), 1);
        assert_eq!(gguf[0].repo, "unsloth/MiniMax-H3-GGUF");
        assert_eq!(gguf[0].name, "MiniMax-H3-GGUF");
        let all = hits_from_modelscope_value(&body, "all");
        assert_eq!(all.len(), 2);
    }

    fn sample_hit(repo: &str) -> RemoteModelHit {
        RemoteModelHit {
            source: ModelSource::ModelScope,
            repo: repo.into(),
            name: repo.rsplit('/').next().unwrap_or(repo).into(),
            author: repo.split('/').next().unwrap_or("").into(),
            downloads: None,
            likes: None,
            library: None,
            pipeline: None,
            tags: Vec::new(),
            params: None,
            updated_at: None,
        }
    }

    #[test]
    fn merge_exact_puts_typed_repo_first() {
        let exact = sample_hit("unsloth/MiniMax-H3-GGUF");
        let merged = merge_exact(
            Some(exact.clone()),
            vec![sample_hit("qwen/A"), sample_hit("unsloth/MiniMax-H3-GGUF")],
        );
        assert_eq!(merged[0].repo, "unsloth/MiniMax-H3-GGUF");
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn gguf_format_matches_repo_name() {
        assert!(hit_matches_format(
            &sample_hit("unsloth/MiniMax-H3-GGUF"),
            "gguf"
        ));
        assert!(!hit_matches_format(&sample_hit("qwen/Qwen2.5-7B"), "gguf"));
    }
}
