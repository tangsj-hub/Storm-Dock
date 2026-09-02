use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::local_models::{
    auth_headers, ensure_disk_space, model_dir, model_id, models_root, probe, resolve_url,
    safe_file_path, status_error, LocalLlm, Manifest, ModelSource, RemoteModelFile,
};
use crate::store::AppState;

const MAX_RUNNING: usize = 3;
// A larger buffer reduces syscall and per-chunk bookkeeping overhead on large model files.
const COPY_BUFFER: usize = 1024 * 1024;
static RUNNING: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
static RECOVERED_ROOTS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
static METRICS: OnceLock<Mutex<HashMap<String, RuntimeMetrics>>> = OnceLock::new();

fn running() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    RUNNING.get_or_init(|| Mutex::new(HashMap::new()))
}
fn metrics() -> &'static Mutex<HashMap<String, RuntimeMetrics>> {
    METRICS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct RuntimeMetrics {
    current_file: String,
    speed_bps: u64,
    last_bytes: u64,
    last_at: Instant,
}
fn note_transfer(id: &str, file: &str, bytes: u64) {
    let Ok(mut values) = metrics().lock() else {
        return;
    };
    let now = Instant::now();
    let metric = values
        .entry(id.to_string())
        .or_insert_with(|| RuntimeMetrics {
            current_file: file.to_string(),
            speed_bps: 0,
            last_bytes: bytes,
            last_at: now,
        });
    if metric.current_file != file {
        *metric = RuntimeMetrics {
            current_file: file.to_string(),
            speed_bps: 0,
            last_bytes: bytes,
            last_at: now,
        };
        return;
    }
    let elapsed = now.duration_since(metric.last_at).as_secs_f64();
    if elapsed >= 0.15 {
        let instant = bytes.saturating_sub(metric.last_bytes) as f64 / elapsed;
        metric.speed_bps = if metric.speed_bps == 0 {
            instant.round() as u64
        } else {
            (metric.speed_bps as f64 * 0.7 + instant * 0.3).round() as u64
        };
        metric.last_bytes = bytes;
        metric.last_at = now;
    }
}
fn reset_metrics(id: &str) {
    if let Ok(mut values) = metrics().lock() {
        values.remove(id);
    }
}
fn metric_snapshot(id: &str, fallback: String) -> (u64, String) {
    metrics()
        .lock()
        .ok()
        .and_then(|values| {
            values.get(id).map(|metric| {
                if metric.last_at.elapsed().as_secs_f64() < 2.0 {
                    (metric.speed_bps, metric.current_file.clone())
                } else {
                    (0, metric.current_file.clone())
                }
            })
        })
        .unwrap_or((0, fallback))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobStatus {
    Queued,
    Downloading,
    Paused,
    RetryWait,
    Failed,
    Verifying,
    Completed,
    Cancelled,
}
impl JobStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::RetryWait => "retry_wait",
            Self::Failed => "failed",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
    fn from_str(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "downloading" => Self::Downloading,
            "paused" => Self::Paused,
            "retry_wait" => Self::RetryWait,
            "verifying" => Self::Verifying,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }
    fn resumable(self) -> bool {
        matches!(
            self,
            Self::Paused | Self::RetryWait | Self::Failed | Self::Cancelled
        )
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadJobView {
    pub job_id: String,
    pub repo: String,
    pub source: ModelSource,
    pub revision: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub current_file: String,
    pub percent: u8,
    pub status: String,
    pub error: Option<String>,
    pub verification: String,
    pub retry_count: u32,
    pub weakly_verified: bool,
}

#[derive(Clone)]
struct StoredFile {
    index: i64,
    path: String,
    size: u64,
    sha256: Option<String>,
    revision: String,
    downloaded: u64,
}
#[derive(Clone)]
struct StoredJob {
    id: String,
    source: ModelSource,
    repo: String,
    revision: String,
    status: JobStatus,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn source_value(source: ModelSource) -> &'static str {
    source.as_str()
}
fn db_path(root: &Path) -> PathBuf {
    root.join("downloads.sqlite")
}
fn staging_root(root: &Path) -> PathBuf {
    root.join(".downloads")
}
fn db(root: &Path) -> Result<Connection, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let conn = Connection::open(db_path(root)).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS download_jobs (id TEXT PRIMARY KEY, source TEXT NOT NULL, repo TEXT NOT NULL, revision TEXT NOT NULL, status TEXT NOT NULL, error TEXT, retries INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
        CREATE UNIQUE INDEX IF NOT EXISTS download_job_key ON download_jobs(source, repo, revision);
        CREATE TABLE IF NOT EXISTS download_files (job_id TEXT NOT NULL REFERENCES download_jobs(id) ON DELETE CASCADE, file_index INTEGER NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL, sha256 TEXT, revision TEXT NOT NULL, downloaded INTEGER NOT NULL DEFAULT 0, completed INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(job_id, file_index));") .map_err(|e| e.to_string())?;
    let recovered = RECOVERED_ROOTS.get_or_init(|| Mutex::new(HashSet::new()));
    if recovered
        .lock()
        .map_err(|_| "下载器不可用".to_string())?
        .insert(root.to_path_buf())
    {
        conn.execute("UPDATE download_jobs SET status='paused', error=COALESCE(error, '应用退出前下载已暂停'), updated_at=?1 WHERE status IN ('downloading', 'verifying', 'queued', 'retry_wait')", params![now()]).map_err(|e| e.to_string())?;
    }
    Ok(conn)
}

fn read_job(conn: &Connection, id: &str) -> Result<StoredJob, String> {
    conn.query_row(
        "SELECT source, repo, revision, status FROM download_jobs WHERE id=?1",
        params![id],
        |row| {
            let source: String = row.get(0)?;
            Ok(StoredJob {
                id: id.to_string(),
                source: crate::local_models::source_from(&source)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                repo: row.get(1)?,
                revision: row.get(2)?,
                status: JobStatus::from_str(&row.get::<_, String>(3)?),
            })
        },
    )
    .map_err(|e| e.to_string())
}
fn read_files(conn: &Connection, id: &str) -> Result<Vec<StoredFile>, String> {
    let mut stmt = conn.prepare("SELECT file_index,path,size,sha256,revision,downloaded FROM download_files WHERE job_id=?1 ORDER BY file_index").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![id], |row| {
            Ok(StoredFile {
                index: row.get(0)?,
                path: row.get(1)?,
                size: row.get::<_, i64>(2)? as u64,
                sha256: row.get(3)?,
                revision: row.get(4)?,
                downloaded: row.get::<_, i64>(5)? as u64,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
fn update_status(
    root: &Path,
    id: &str,
    status: JobStatus,
    error: Option<&str>,
) -> Result<(), String> {
    db(root)?
        .execute(
            "UPDATE download_jobs SET status=?1,error=?2,updated_at=?3 WHERE id=?4",
            params![status.as_str(), error, now(), id],
        )
        .map_err(|e| e.to_string())?;
    if status != JobStatus::Downloading {
        reset_metrics(id);
    }
    Ok(())
}
fn update_progress(root: &Path, id: &str, index: i64, bytes: u64) -> Result<(), String> {
    db(root)?
        .execute(
            "UPDATE download_files SET downloaded=?1 WHERE job_id=?2 AND file_index=?3",
            params![bytes as i64, id, index],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn set_complete(root: &Path, id: &str, index: i64) -> Result<(), String> {
    db(root)?
        .execute(
            "UPDATE download_files SET completed=1 WHERE job_id=?1 AND file_index=?2",
            params![id, index],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn views(root: &Path) -> Result<Vec<DownloadJobView>, String> {
    let conn = db(root)?;
    let mut stmt = conn.prepare("SELECT id,source,repo,revision,status,error,retries FROM download_jobs ORDER BY created_at DESC").map_err(|e|e.to_string())?;
    let rows: Vec<_> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    rows.into_iter()
        .map(|(id, source, repo, revision, status, error, retries)| {
            let files = read_files(&conn, &id)?;
            let downloaded = files.iter().map(|f| f.downloaded.min(f.size)).sum::<u64>();
            let total = files.iter().map(|f| f.size).sum::<u64>();
            let weak = files.iter().any(|f| f.sha256.is_none());
            let fallback = files
                .iter()
                .find(|f| f.downloaded < f.size)
                .map(|f| f.path.clone())
                .unwrap_or_default();
            let (speed_bps, current_file) = if status == "downloading" {
                metric_snapshot(&id, fallback)
            } else {
                (0, fallback)
            };
            Ok(DownloadJobView {
                job_id: id,
                repo,
                source: crate::local_models::source_from(&source)?,
                revision,
                downloaded_bytes: downloaded,
                total_bytes: total,
                speed_bps,
                current_file,
                percent: if total == 0 {
                    0
                } else {
                    ((downloaded * 100) / total) as u8
                },
                status,
                error,
                verification: if weak { "weak".into() } else { "sha256".into() },
                retry_count: retries as u32,
                weakly_verified: weak,
            })
        })
        .collect()
}
fn emit_snapshot(app: &AppHandle) {
    if let Ok(root) = models_root(app) {
        if let Ok(value) = views(&root) {
            let _ = app.emit("model-download-snapshot", value);
        }
    }
}

fn parse_content_range(
    value: Option<&str>,
    expected_start: u64,
    expected_size: u64,
) -> Result<(), String> {
    let value = value.ok_or_else(|| "服务器未返回 Content-Range，无法安全续传".to_string())?;
    let rest = value
        .strip_prefix("bytes ")
        .ok_or_else(|| "服务器返回了无效 Content-Range".to_string())?;
    let (range, total) = rest
        .split_once('/')
        .ok_or_else(|| "服务器返回了无效 Content-Range".to_string())?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| "服务器返回了无效 Content-Range".to_string())?;
    let start = start
        .parse::<u64>()
        .map_err(|_| "服务器返回了无效 Content-Range".to_string())?;
    let end = end
        .parse::<u64>()
        .map_err(|_| "服务器返回了无效 Content-Range".to_string())?;
    let total = total
        .parse::<u64>()
        .map_err(|_| "服务器返回了无效 Content-Range".to_string())?;
    if start != expected_start || end < start || total != expected_size {
        return Err("远端对象的范围或大小已变化，已停止恢复".into());
    }
    Ok(())
}
fn sha256(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut b = [0u8; COPY_BUFFER];
    loop {
        let n = f.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&b[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn download_file(
    app: &AppHandle,
    root: &Path,
    job: &StoredJob,
    file: &StoredFile,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let staged = staging_root(root)
        .join(&job.id)
        .join("data")
        .join(safe_file_path(&file.path)?);
    if let Some(parent) = staged.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut offset = file.downloaded.min(file.size);
    if staged.metadata().map(|m| m.len()).unwrap_or(0) != offset {
        return Err("本地断点与任务记录不一致，已拒绝继续".into());
    }
    if offset == file.size {
        return verify_file(root, job, file, &staged);
    }
    update_status(root, &job.id, JobStatus::Downloading, None)?;
    note_transfer(&job.id, &file.path, offset);
    emit_snapshot(app);
    let url = resolve_url(job.source, &job.repo, &file.revision, &file.path)?;
    let mut response = crate::local_models::http_client()
        .get(url)
        .headers(auth_headers(job.source))
        .header(RANGE, format!("bytes={offset}-"))
        .send()
        .map_err(|e| e.to_string())?;
    if response.status() == StatusCode::PARTIAL_CONTENT {
        parse_content_range(
            response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok()),
            offset,
            file.size,
        )?;
    } else if !(offset == 0
        && response.status().is_success()
        && response.content_length() == Some(file.size))
    {
        return Err(if response.status().is_success() {
            "服务器不支持安全 Range 续传".into()
        } else {
            status_error(job.source, response.status())
        });
    }
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&staged)
        .map_err(|e| e.to_string())?;
    // Keep the large transfer buffer on the heap; download jobs run on native
    // threads whose stacks can be comparatively small on macOS.
    let mut buf = vec![0u8; COPY_BUFFER];
    let mut last_persisted = offset;
    let mut last_emitted = Instant::now();
    while offset < file.size {
        if cancel.load(Ordering::Relaxed) {
            out.sync_all().map_err(|e| e.to_string())?;
            update_progress(root, &job.id, file.index, offset)?;
            return Ok(());
        }
        let n = response.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if offset + n as u64 > file.size {
            return Err("服务器返回的数据超过声明大小".into());
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        offset += n as u64;
        note_transfer(&job.id, &file.path, offset);
        // Persist and notify at a bounded cadence; doing both for every small read throttles downloads.
        if offset.saturating_sub(last_persisted) >= 4 * COPY_BUFFER as u64 || offset == file.size {
            update_progress(root, &job.id, file.index, offset)?;
            last_persisted = offset;
        }
        if last_emitted.elapsed() >= std::time::Duration::from_millis(200) || offset == file.size {
            emit_snapshot(app);
            last_emitted = Instant::now();
        }
    }
    out.sync_all().map_err(|e| e.to_string())?;
    if offset != file.size {
        return Err("远端响应提前结束".into());
    }
    drop(out);
    verify_file(root, job, file, &staged)
}
fn verify_file(root: &Path, job: &StoredJob, file: &StoredFile, path: &Path) -> Result<(), String> {
    update_status(root, &job.id, JobStatus::Verifying, None)?;
    if path.metadata().map_err(|e| e.to_string())?.len() != file.size {
        return Err("文件大小校验失败".into());
    };
    if let Some(expected) = &file.sha256 {
        let actual = sha256(path)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!("SHA-256 校验失败：{}", file.path));
        }
    }
    set_complete(root, &job.id, file.index)
}
fn publish(
    app: &AppHandle,
    root: &Path,
    job: &StoredJob,
    files: &[StoredFile],
) -> Result<(), String> {
    let staging = staging_root(root).join(&job.id).join("data");
    let destination = model_dir(root, job.source, &job.repo);
    if destination.exists() {
        return Err("目标模型目录已存在，拒绝覆盖".into());
    };
    let manifest = Manifest {
        source: job.source,
        repo: job.repo.clone(),
        revision: job.revision.clone(),
        files: files
            .iter()
            .map(|f| RemoteModelFile {
                path: f.path.clone(),
                size: f.size,
                sha256: f.sha256.clone(),
                revision: Some(f.revision.clone()),
            })
            .collect(),
    };
    fs::write(
        staging.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(&staging, &destination).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir(staging_root(root).join(&job.id));
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut controller) = state.0.lock() {
            let _ = controller.upsert_local_model(&LocalLlm {
                id: model_id(job.source, &job.repo),
                source: job.source,
                repo: job.repo.clone(),
                revision: job.revision.clone(),
                path: destination.to_string_lossy().into_owned(),
                size: files.iter().map(|f| f.size).sum(),
                files: files.len() as u32,
            });
        }
    }
    Ok(())
}
fn execute_job(app: &AppHandle, root: &Path, id: &str, cancel: &AtomicBool) -> Result<(), String> {
    let conn = db(root)?;
    let job = read_job(&conn, id)?;
    let files = read_files(&conn, id)?;
    ensure_disk_space(
        &staging_root(root).join(id),
        &files
            .iter()
            .map(|f| RemoteModelFile {
                path: f.path.clone(),
                size: f.size,
                sha256: f.sha256.clone(),
                revision: Some(f.revision.clone()),
            })
            .collect::<Vec<_>>(),
    )?;
    for file in &files {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        download_file(app, root, &job, file, cancel)?;
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    publish(app, root, &job, &files)
}
fn run_job(app: AppHandle, id: String, cancel: Arc<AtomicBool>) {
    let root = match models_root(&app) {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut result = execute_job(&app, &root, &id, &cancel);
    for retry in 1..=3 {
        if cancel.load(Ordering::Relaxed) || result.is_ok() {
            break;
        }
        let error = result.as_ref().err().cloned().unwrap_or_default();
        if error.contains("SHA-256") || error.contains("范围或大小") || error.contains("本地断点")
        {
            break;
        }
        let _=db(&root).and_then(|conn|conn.execute("UPDATE download_jobs SET status='retry_wait',retries=?1,error=?2,updated_at=?3 WHERE id=?4",params![retry as i64,error,now()]).map_err(|e|e.to_string()).map(|_|()));
        emit_snapshot(&app);
        thread::sleep(std::time::Duration::from_millis(400 * 2u64.pow(retry - 1)));
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let _ = update_status(&root, &id, JobStatus::Downloading, None);
        result = execute_job(&app, &root, &id, &cancel);
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = update_status(&root, &id, JobStatus::Paused, None);
    } else if let Err(error) = result {
        let _ = update_status(&root, &id, JobStatus::Failed, Some(&error));
    } else {
        let _ = update_status(&root, &id, JobStatus::Completed, None);
    }
    if let Ok(mut active) = running().lock() {
        active.remove(&id);
    }
    emit_snapshot(&app);
    pump(&app);
}
fn pump(app: &AppHandle) {
    let Ok(root) = models_root(app) else { return };
    let Ok(conn) = db(&root) else { return };
    let active = running().lock().map(|m| m.len()).unwrap_or(MAX_RUNNING);
    if active >= MAX_RUNNING {
        return;
    };
    let mut stmt = match conn
        .prepare("SELECT id FROM download_jobs WHERE status='queued' ORDER BY created_at")
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let ids: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .take(MAX_RUNNING - active)
        .collect();
    for id in ids {
        let cancel = Arc::new(AtomicBool::new(false));
        if running()
            .lock()
            .map(|mut m| m.insert(id.clone(), cancel.clone()).is_none())
            .unwrap_or(false)
        {
            let _ = update_status(&root, &id, JobStatus::Downloading, None);
            let handle = app.clone();
            thread::spawn(move || run_job(handle, id, cancel));
        }
    }
    emit_snapshot(app)
}

#[tauri::command]
pub(crate) fn start_model_download(
    app: AppHandle,
    source: ModelSource,
    repo: String,
    revision: Option<String>,
    files: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    crate::local_models::bind_stored_hf_token(&state);
    let snapshot = probe(source, &repo, revision.as_deref())?;
    let selected: Vec<_> = match files {
        Some(names) => {
            let wanted: HashSet<_> = names.into_iter().collect();
            snapshot
                .files
                .into_iter()
                .filter(|f| wanted.contains(&f.path))
                .collect()
        }
        None => snapshot.files,
    };
    if selected.is_empty() {
        return Err("没有可下载的文件".into());
    };
    let root = models_root(&app)?;
    ensure_disk_space(&staging_root(&root), &selected)?;
    let conn = db(&root)?;
    if let Ok(id)=conn.query_row("SELECT id FROM download_jobs WHERE source=?1 AND repo=?2 AND revision=?3 AND status!='completed'",params![source_value(source),snapshot.repo,snapshot.revision],|r|r.get::<_,String>(0)){conn.execute("UPDATE download_jobs SET status='queued',error=NULL,updated_at=?1 WHERE id=?2",params![now(),id]).map_err(|e|e.to_string())?;pump(&app);return Ok(id)}
    let id = uuid::Uuid::new_v4().to_string();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO download_jobs(id,source,repo,revision,status,created_at,updated_at) VALUES(?1,?2,?3,?4,'queued',?5,?5)",params![id,source_value(source),snapshot.repo,snapshot.revision,now()]).map_err(|e|e.to_string())?;
    for (index, file) in selected.iter().enumerate() {
        tx.execute("INSERT INTO download_files(job_id,file_index,path,size,sha256,revision) VALUES(?1,?2,?3,?4,?5,?6)",params![id,index as i64,file.path,file.size as i64,file.sha256,file.revision.clone().unwrap_or_else(||snapshot.revision.clone())]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    pump(&app);
    Ok(id)
}
#[tauri::command]
pub(crate) fn cancel_model_download(app: AppHandle, job_id: String) -> Result<(), String> {
    let root = models_root(&app)?;
    if let Ok(map) = running().lock() {
        if let Some(flag) = map.get(&job_id) {
            flag.store(true, Ordering::Relaxed);
            return Ok(());
        }
    }
    update_status(&root, &job_id, JobStatus::Paused, None)?;
    emit_snapshot(&app);
    Ok(())
}
#[tauri::command]
pub(crate) fn list_download_jobs(app: AppHandle) -> Result<Vec<DownloadJobView>, String> {
    views(&models_root(&app)?)
}
#[tauri::command]
pub(crate) fn resume_download_job(app: AppHandle, job_id: String) -> Result<(), String> {
    let root = models_root(&app)?;
    let conn = db(&root)?;
    let job = read_job(&conn, &job_id)?;
    if !job.status.resumable() {
        return Err("该下载任务当前不能继续".into());
    }
    update_status(&root, &job_id, JobStatus::Queued, None)?;
    pump(&app);
    Ok(())
}
#[tauri::command]
pub(crate) fn dismiss_download_job(app: AppHandle, job_id: String) -> Result<(), String> {
    let root = models_root(&app)?;
    let conn = db(&root)?;
    let job = read_job(&conn, &job_id)?;
    if job.status == JobStatus::Downloading {
        return Err("请先暂停下载任务".into());
    }
    conn.execute("DELETE FROM download_jobs WHERE id=?1", params![job_id])
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(staging_root(&root).join(&job.id));
    emit_snapshot(&app);
    Ok(())
}
pub(crate) fn cancel_jobs_for_repo(app: &AppHandle, source: ModelSource, repo: &str) {
    let Ok(root) = models_root(app) else { return };
    let Ok(conn) = db(&root) else { return };
    let ids:Vec<String>=conn.prepare("SELECT id FROM download_jobs WHERE source=?1 AND repo=?2 AND status IN ('queued','downloading')").ok().and_then(|mut s|s.query_map(params![source_value(source),repo],|r|r.get(0)).ok().map(|rows|rows.filter_map(Result::ok).collect())).unwrap_or_default();
    for id in ids {
        let _ = cancel_model_download(app.clone(), id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_range_rejects_wrong_object() {
        assert!(parse_content_range(Some("bytes 0-4/5"), 0, 5).is_ok());
        assert!(parse_content_range(Some("bytes 1-4/5"), 0, 5).is_err());
        assert!(parse_content_range(Some("bytes 0-4/6"), 0, 5).is_err());
    }
}
