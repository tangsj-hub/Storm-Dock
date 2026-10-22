//! Queued Bot模版 import into the Grok Bot client.
//!
//! Opens one `grokbot://` import deep link at a time and waits for the user to
//! confirm in Grok Bot's own dialog (never bypassed). Completion = a new agent
//! id appears in the ACTIVE account's roster cache (read-only); no new agent
//! within `ITEM_TIMEOUT` = skipped. Progress is shown in a banner window that
//! reuses the switcher popup look.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub(crate) const WINDOW_LABEL: &str = "popup-bot-import";
pub(crate) const PAGE: &str = "switcher.html?mode=import";
pub(crate) const PROGRESS_EVENT: &str = "bot-import-progress";
const ITEM_TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_secs(1);
const SUMMARY_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueueItem {
    pub(crate) url: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Imported,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportProgress {
    pub(crate) total: usize,
    /// 1-based position of the item in progress (== total when done).
    pub(crate) position: usize,
    pub(crate) current_name: Option<String>,
    pub(crate) done: bool,
    pub(crate) cancelled: bool,
    pub(crate) imported: Vec<String>,
    pub(crate) skipped: Vec<String>,
    /// Set when the client is too old (or its version unknown): nothing was opened.
    /// Holds the current version, empty string if unknown.
    pub(crate) blocked_version: Option<String>,
    /// Minimum client version for import (single source: grok_bot::version).
    pub(crate) required_version: &'static str,
}

/// Pure queue state machine (no IO), unit tested.
#[derive(Debug, Clone)]
pub(crate) struct ImportQueue {
    items: Vec<QueueItem>,
    next: usize,
    imported: Vec<String>,
    skipped: Vec<String>,
    cancelled: bool,
    blocked_version: Option<String>,
}

impl ImportQueue {
    pub(crate) fn new(items: Vec<QueueItem>) -> Self {
        Self { items, next: 0, imported: Vec::new(), skipped: Vec::new(), cancelled: false, blocked_version: None }
    }

    /// Client too old / unknown: everything is skipped up front.
    pub(crate) fn blocked(items: Vec<QueueItem>, current: Option<String>) -> Self {
        let mut queue = Self::new(items);
        queue.cancel();
        queue.cancelled = false;
        queue.blocked_version = Some(current.unwrap_or_default());
        queue
    }

    pub(crate) fn current(&self) -> Option<&QueueItem> {
        if self.cancelled {
            return None;
        }
        self.items.get(self.next)
    }

    pub(crate) fn finish_current(&mut self, outcome: Outcome) {
        let Some(item) = self.items.get(self.next) else { return };
        match outcome {
            Outcome::Imported => self.imported.push(item.name.clone()),
            Outcome::Skipped => self.skipped.push(item.name.clone()),
        }
        self.next += 1;
    }

    /// Stop: the item in progress and everything after it count as skipped.
    pub(crate) fn cancel(&mut self) {
        if self.cancelled {
            return;
        }
        for item in self.items.iter().skip(self.next) {
            self.skipped.push(item.name.clone());
        }
        self.next = self.items.len();
        self.cancelled = true;
    }

    pub(crate) fn is_done(&self) -> bool {
        self.cancelled || self.blocked_version.is_some() || self.next >= self.items.len()
    }

    pub(crate) fn progress(&self) -> ImportProgress {
        let total = self.items.len();
        ImportProgress {
            total,
            position: (self.next + 1).min(total),
            current_name: self.current().map(|item| item.name.clone()),
            done: self.is_done(),
            cancelled: self.cancelled,
            imported: self.imported.clone(),
            skipped: self.skipped.clone(),
            blocked_version: self.blocked_version.clone(),
            required_version: crate::grok_bot::version::IMPORT_MIN_VERSION,
        }
    }
}

/// A new agent in `after` that was not in `before`; prefers a name match.
pub(crate) fn detect_new_agent(
    before: &HashSet<String>,
    after: &[(String, Option<String>)],
    expected_name: &str,
) -> Option<String> {
    let fresh: Vec<&(String, Option<String>)> = after.iter().filter(|(id, _)| !before.contains(id)).collect();
    let expected = expected_name.trim();
    fresh
        .iter()
        .find(|(_, name)| name.as_deref().map(str::trim) == Some(expected))
        .or_else(|| fresh.first())
        .map(|(id, _)| id.clone())
}

struct Active {
    generation: u64,
    /// Generation of the banner showing this queue.
    popup: u64,
    queue: ImportQueue,
}

static ACTIVE: Lazy<Mutex<Option<Active>>> = Lazy::new(|| Mutex::new(None));
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn is_running() -> bool {
    ACTIVE.lock().map(|active| active.as_ref().is_some_and(|a| !a.queue.is_done())).unwrap_or(false)
}

pub(crate) fn progress() -> Option<ImportProgress> {
    ACTIVE.lock().ok()?.as_ref().map(|active| active.queue.progress())
}

fn emit(app: &AppHandle) {
    if let Some(progress) = progress() {
        let _ = app.emit(PROGRESS_EVENT, &progress);
    }
}

pub(crate) fn cancel(app: &AppHandle) {
    if let Ok(mut active) = ACTIVE.lock() {
        if let Some(active) = active.as_mut() {
            active.queue.cancel();
        }
    }
    emit(app);
}

/// Hide the banner showing `popup` (stale answers are ignored by the popup manager).
pub(crate) fn dismiss(popup: u64, animate: bool) {
    if let Ok(mut active) = ACTIVE.lock() {
        if active.as_ref().is_some_and(|a| a.popup == popup && a.queue.is_done()) {
            *active = None;
        }
    }
    crate::native_popup::hide(WINDOW_LABEL, Some(popup), animate);
}

fn roster_ids() -> HashSet<String> {
    crate::grok_bot_sessions::active_roster_agents()
        .unwrap_or_default()
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

fn import_one(item: &QueueItem, generation: u64) -> Outcome {
    let before = roster_ids();
    if crate::bot_templates::open_in_client(&item.url).is_err() {
        // Neither the deep link nor the web page could be opened.
        return Outcome::Skipped;
    }
    let deadline = Instant::now() + ITEM_TIMEOUT;
    while Instant::now() < deadline {
        thread::sleep(POLL);
        let cancelled = ACTIVE
            .lock()
            .map(|a| a.as_ref().map_or(true, |a| a.generation != generation || a.queue.is_done()))
            .unwrap_or(true);
        if cancelled {
            return Outcome::Skipped;
        }
        if let Some(after) = crate::grok_bot_sessions::active_roster_agents() {
            if detect_new_agent(&before, &after, &item.name).is_some() {
                return Outcome::Imported;
            }
        }
    }
    Outcome::Skipped
}

/// Start a queue; refused while another one is still running.
pub(crate) fn start(app: &AppHandle, items: Vec<QueueItem>) -> Result<(), String> {
    if items.is_empty() {
        return Err("没有勾选自动导入的模版。".into());
    }
    let gate = crate::grok_bot::version::import_gate();
    let generation = {
        let mut active = ACTIVE.lock().map_err(|_| "导入队列不可用".to_string())?;
        if active.as_ref().is_some_and(|a| !a.queue.is_done()) {
            return Err("已有模版正在导入。".into());
        }
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let queue = match &gate {
            Ok(_) => ImportQueue::new(items),
            Err(current) => {
                eprintln!("bot template import skipped: {}", crate::grok_bot::version::gate_message(current.as_deref()));
                ImportQueue::blocked(items, current.clone())
            }
        };
        let popup = crate::native_popup::show(WINDOW_LABEL, PAGE, serde_json::json!({ "preview": false }));
        *active = Some(Active { generation, popup, queue });
        generation
    };
    let app = app.clone();
    thread::spawn(move || {
        loop {
            let item = {
                let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
                match active.as_ref() {
                    Some(a) if a.generation == generation => a.queue.current().cloned(),
                    _ => return,
                }
            };
            emit(&app);
            let Some(item) = item else { break };
            let outcome = import_one(&item, generation);
            if let Ok(mut active) = ACTIVE.lock() {
                if let Some(a) = active.as_mut().filter(|a| a.generation == generation) {
                    if a.queue.current() == Some(&item) {
                        a.queue.finish_current(outcome);
                    }
                }
            }
        }
        emit(&app);
        // The popup manager hides it later unless a newer banner superseded it.
        let popup = ACTIVE
            .lock()
            .ok()
            .and_then(|a| a.as_ref().filter(|a| a.generation == generation).map(|a| a.popup));
        if let Some(popup) = popup {
            crate::native_popup::auto_hide(WINDOW_LABEL, popup, Duration::from_secs(SUMMARY_SECS));
        }
    });
    Ok(())
}

/// All auto-import templates, in list order.
pub(crate) fn auto_import_items(app: &AppHandle) -> Vec<QueueItem> {
    let state = app.state::<crate::store::AppState>();
    let Ok(controller) = state.0.lock() else { return Vec::new() };
    crate::bot_templates::auto_import_items(&controller.bot_templates())
}

#[tauri::command]
pub(crate) fn get_bot_import_progress() -> Option<ImportProgress> {
    progress()
}

#[tauri::command]
pub(crate) fn cancel_bot_import(app: AppHandle) {
    cancel(&app);
}

#[tauri::command]
pub(crate) fn dismiss_bot_import(generation: u64, instant: Option<bool>) {
    dismiss(generation, !instant.unwrap_or(false));
}

#[tauri::command]
pub(crate) fn start_bot_import_all(app: AppHandle) -> Result<usize, String> {
    // Manual action: refuse up front (the UI shows the reminder) instead of a blocked banner.
    if let Err(current) = crate::grok_bot::version::import_gate() {
        return Err(crate::grok_bot::version::gate_message(current.as_deref()));
    }
    let items = auto_import_items(&app);
    let count = items.len();
    start(&app, items)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> QueueItem {
        QueueItem { url: format!("https://x.ai/bot/{name}"), name: name.into() }
    }

    #[test]
    fn queue_runs_items_in_order_and_summarizes() {
        let mut queue = ImportQueue::new(vec![item("a"), item("b"), item("c")]);
        assert_eq!(queue.progress().position, 1);
        assert_eq!(queue.current().unwrap().name, "a");
        queue.finish_current(Outcome::Imported);
        assert_eq!(queue.progress().current_name.as_deref(), Some("b"));
        assert_eq!(queue.progress().position, 2);
        queue.finish_current(Outcome::Skipped);
        queue.finish_current(Outcome::Imported);
        let done = queue.progress();
        assert!(done.done && !done.cancelled);
        assert_eq!(done.imported, vec!["a", "c"]);
        assert_eq!(done.skipped, vec!["b"]);
        assert_eq!(done.position, 3);
        assert!(queue.current().is_none());
        queue.finish_current(Outcome::Imported); // no-op once done
        assert_eq!(queue.progress().imported.len(), 2);
    }

    #[test]
    fn cancel_skips_current_and_remaining() {
        let mut queue = ImportQueue::new(vec![item("a"), item("b"), item("c")]);
        queue.finish_current(Outcome::Imported);
        queue.cancel();
        let progress = queue.progress();
        assert!(progress.done && progress.cancelled);
        assert_eq!(progress.imported, vec!["a"]);
        assert_eq!(progress.skipped, vec!["b", "c"]);
        assert!(queue.current().is_none());
        queue.cancel();
        assert_eq!(queue.progress().skipped.len(), 2);
    }

    #[test]
    fn blocked_queue_skips_everything_and_reports_version() {
        let queue = ImportQueue::blocked(vec![item("a"), item("b")], Some("0.74.0".into()));
        let progress = queue.progress();
        assert!(progress.done && !progress.cancelled);
        assert!(queue.current().is_none());
        assert_eq!(progress.skipped, vec!["a", "b"]);
        assert_eq!(progress.blocked_version.as_deref(), Some("0.74.0"));
        let unknown = ImportQueue::blocked(vec![item("a")], None).progress();
        assert_eq!(unknown.blocked_version.as_deref(), Some(""));
    }

    #[test]
    fn detects_new_agent_preferring_name_match() {
        let before: HashSet<String> = ["old".to_string()].into();
        let roster = |rows: &[(&str, &str)]| rows.iter().map(|(id, name)| (id.to_string(), Some(name.to_string()))).collect::<Vec<_>>();
        assert_eq!(detect_new_agent(&before, &roster(&[("old", "小C")]), "小C"), None);
        assert_eq!(detect_new_agent(&before, &roster(&[("old", "x"), ("n1", "Other"), ("n2", "小C")]), "小C").as_deref(), Some("n2"));
        assert_eq!(detect_new_agent(&before, &roster(&[("n1", "Renamed")]), "小C").as_deref(), Some("n1"));
        assert_eq!(detect_new_agent(&before, &[], "小C"), None);
    }
}
