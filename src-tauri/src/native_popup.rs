//! Native banner popups (switcher reminder / preview, Bot模版 import progress).
//!
//! Architecture: one long-lived actor thread owns every popup. Callers send
//! commands over a channel ([`show`], [`hide`], [`auto_hide`]); the actor drains
//! everything queued, folds it into the pure [`Machine`] (latest wins), then
//! performs the minimal window ops. 20 rapid clicks = one payload emit + at most
//! one show + one animation.
//!
//! Windows are created once (pre-warmed hidden at startup), never closed: hide
//! instead of close, show instead of build, vibrancy applied once at creation.
//! Each show gets a monotonically increasing generation; pages answer with it, so
//! responses from a superseded popup are ignored and hides are generation-guarded.
//!
//! macOS: transparent borderless window + window-vibrancy (Popover, radius 16),
//! top-right under the menu bar. Windows: the page draws a solid toast card,
//! bottom-right above the taskbar. Labels start with [`LABEL_PREFIX`] so theme
//! syncing skips them.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use once_cell::sync::{Lazy, OnceCell};
use serde_json::Value;
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub(crate) const LABEL_PREFIX: &str = "popup-";
/// Emitted to one popup window with an [`Envelope`] on every show.
pub(crate) const PAYLOAD_EVENT: &str = "popup-payload";
const WIDTH: f64 = 356.0;
const HEIGHT: f64 = 112.0;
const MARGIN: f64 = 12.0;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const RADIUS: f64 = 16.0;
/// Slide distance from the right, like a macOS banner. Offset 0 = on screen.
const SLIDE: f64 = 48.0;
const SLIDE_IN_MS: u64 = 180;
const SLIDE_OUT_MS: u64 = 140;
const FRAME_MS: u64 = 16;
/// Offset a settled popup jumps to when shown again (then slides back).
const NUDGE: f64 = 14.0;
/// After hover ends, at least this long before an auto-hide fires.
const MIN_RESUME_MS: u64 = 1500;
/// While the pointer rests on a popup whose timer is due, look again this often.
const HOVER_RECHECK_MS: u64 = 1000;

pub(crate) fn is_popup_label(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct Envelope {
    pub generation: u64,
    pub payload: Value,
}

// ---------------------------------------------------------------------------
// Pure state machine (no Tauri), unit-tested below.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cmd {
    Prewarm { label: &'static str, page: &'static str },
    /// `auto_hide_ms`: hide (and report [`Effect::Expired`]) that long after this show.
    Show { label: &'static str, page: &'static str, generation: u64, payload: Value, auto_hide_ms: Option<u64> },
    /// `generation: None` hides whatever is shown. `animate: false` after a swipe.
    Hide { label: &'static str, generation: Option<u64>, animate: bool },
    AutoHide { label: &'static str, generation: u64, after_ms: u64 },
    /// Pause / resume the auto-hide timer (pointer hover, native confirm open).
    Pause { label: &'static str, generation: u64, reason: PauseReason, on: bool },
    /// The window vanished behind our back; re-create on next show.
    Lost { label: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PauseReason {
    Hover,
    Hold,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Effect {
    Create { label: &'static str, page: &'static str },
    Emit { label: &'static str, envelope: Envelope },
    Move { label: &'static str, offset: f64 },
    Show { label: &'static str },
    Hide { label: &'static str },
    /// The auto-hide timer of `generation` fired (owner treats it as "ignore").
    Expired { label: &'static str, generation: u64 },
}

#[derive(Debug, Clone, Copy)]
struct Anim {
    from: f64,
    to: f64,
    start: u64,
    dur: u64,
}

#[derive(Debug)]
struct Slot {
    page: &'static str,
    created: bool,
    generation: u64,
    payload: Option<Value>,
    dirty: bool,
    want_visible: bool,
    shown: bool,
    offset: f64,
    anim: Option<Anim>,
    hide_at: Option<u64>,
    /// Time left on a paused auto-hide timer.
    remaining: Option<u64>,
    hover: bool,
    hold: bool,
}

impl Slot {
    fn new(page: &'static str) -> Self {
        Self {
            page,
            created: false,
            generation: 0,
            payload: None,
            dirty: false,
            want_visible: false,
            shown: false,
            offset: SLIDE,
            anim: None,
            hide_at: None,
            remaining: None,
            hover: false,
            hold: false,
        }
    }

    fn paused(&self) -> bool {
        self.hover || self.hold
    }

    fn arm(&mut self, ms: u64, now: u64) {
        if self.paused() {
            self.hide_at = None;
            self.remaining = Some(ms);
        } else {
            self.hide_at = Some(now + ms);
            self.remaining = None;
        }
    }

    /// Single animation per window: a new target retargets from where it is now.
    fn retarget(&mut self, to: f64, full_ms: u64, now: u64) {
        if (self.offset - to).abs() < f64::EPSILON {
            self.anim = None;
            return;
        }
        let dur = ((full_ms as f64) * (self.offset - to).abs() / SLIDE).round().max(1.0) as u64;
        self.anim = Some(Anim { from: self.offset, to, start: now, dur });
    }

    fn start_hide(&mut self, animate: bool, now: u64) {
        self.hide_at = None;
        self.remaining = None;
        // Swipe / hide must not leave pause flags stuck: the next show would arm a
        // paused timer, and a late mouseleave could resume a bogus remaining time.
        self.hover = false;
        self.hold = false;
        if !self.want_visible {
            return;
        }
        self.want_visible = false;
        if animate && self.shown {
            self.retarget(SLIDE, SLIDE_OUT_MS, now);
        } else {
            self.anim = None;
            self.offset = SLIDE;
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Machine {
    slots: BTreeMap<&'static str, Slot>,
}

impl Machine {
    pub(crate) fn apply(&mut self, cmd: Cmd, now: u64) {
        match cmd {
            Cmd::Prewarm { label, page } => {
                self.slots.entry(label).or_insert_with(|| Slot::new(page));
            }
            Cmd::Show { label, page, generation, payload, auto_hide_ms } => {
                let slot = self.slots.entry(label).or_insert_with(|| Slot::new(page));
                if generation < slot.generation {
                    return;
                }
                slot.generation = generation;
                slot.payload = Some(payload);
                slot.dirty = true;
                slot.hide_at = None;
                slot.remaining = None;
                if let Some(ms) = auto_hide_ms {
                    slot.arm(ms, now);
                }
                if !slot.want_visible {
                    slot.want_visible = true;
                    // Frontend swipe may have left the window far off-screen; always
                    // re-enter from the normal slide-in offset.
                    slot.anim = None;
                    slot.offset = SLIDE;
                    slot.retarget(0.0, SLIDE_IN_MS, now);
                } else if slot.shown && slot.anim.is_none() {
                    // Already settled on screen: a short nudge so a repeated show is noticeable.
                    slot.offset = NUDGE;
                    slot.retarget(0.0, SLIDE_IN_MS, now);
                }
            }
            Cmd::Hide { label, generation, animate } => {
                let Some(slot) = self.slots.get_mut(label) else { return };
                if generation.is_some_and(|g| g != slot.generation) {
                    return;
                }
                slot.start_hide(animate, now);
            }
            Cmd::AutoHide { label, generation, after_ms } => {
                let Some(slot) = self.slots.get_mut(label) else { return };
                if generation == slot.generation && slot.want_visible {
                    slot.arm(after_ms, now);
                }
            }
            Cmd::Pause { label, generation, reason, on } => {
                let Some(slot) = self.slots.get_mut(label) else { return };
                if generation != slot.generation {
                    return;
                }
                let was = slot.paused();
                match reason {
                    PauseReason::Hover => slot.hover = on,
                    PauseReason::Hold => slot.hold = on,
                }
                match (was, slot.paused()) {
                    (false, true) => {
                        if let Some(at) = slot.hide_at.take() {
                            slot.remaining = Some(at.saturating_sub(now));
                        }
                    }
                    (true, false) => {
                        if let Some(left) = slot.remaining.take() {
                            slot.hide_at = Some(now + left.max(MIN_RESUME_MS));
                        }
                    }
                    _ => {}
                }
            }
            Cmd::Lost { label } => {
                if let Some(slot) = self.slots.get_mut(label) {
                    slot.created = false;
                    slot.shown = false;
                    slot.dirty = slot.payload.is_some();
                    slot.offset = SLIDE;
                    if slot.want_visible {
                        slot.retarget(0.0, SLIDE_IN_MS, now);
                    }
                }
            }
        }
    }

    /// Effects to perform now, already coalesced (one emit/move per window).
    pub(crate) fn flush(&mut self, now: u64) -> Vec<Effect> {
        let mut out = Vec::new();
        for (&label, slot) in &mut self.slots {
            if slot.hide_at.is_some_and(|t| now >= t) {
                slot.start_hide(true, now);
                out.push(Effect::Expired { label, generation: slot.generation });
            }
            if !slot.created {
                out.push(Effect::Create { label, page: slot.page });
                slot.created = true;
            }
            if slot.dirty {
                slot.dirty = false;
                if let Some(payload) = &slot.payload {
                    out.push(Effect::Emit {
                        label,
                        envelope: Envelope { generation: slot.generation, payload: payload.clone() },
                    });
                }
            }
            let mut moved = false;
            if let Some(a) = slot.anim {
                let k = (now.saturating_sub(a.start) as f64 / a.dur as f64).min(1.0);
                let eased = if a.to < a.from { 1.0 - (1.0 - k).powi(3) } else { k * k };
                slot.offset = a.from + (a.to - a.from) * eased;
                moved = true;
                if k >= 1.0 {
                    slot.anim = None;
                }
            }
            if slot.want_visible && !slot.shown {
                out.push(Effect::Move { label, offset: slot.offset });
                out.push(Effect::Show { label });
                slot.shown = true;
            } else if moved && slot.shown {
                out.push(Effect::Move { label, offset: slot.offset });
            }
            if !slot.want_visible && slot.shown && slot.anim.is_none() {
                out.push(Effect::Hide { label });
                slot.shown = false;
                slot.offset = SLIDE;
            }
        }
        out
    }

    /// Windows whose auto-hide is due at `now`.
    pub(crate) fn due(&self, now: u64) -> Vec<(&'static str, u64)> {
        self.slots
            .iter()
            .filter(|(_, s)| s.hide_at.is_some_and(|t| now >= t))
            .map(|(&l, s)| (l, s.generation))
            .collect()
    }

    /// How long the actor may block: a frame while animating, else the next auto-hide.
    pub(crate) fn next_wake(&self, now: u64) -> Option<u64> {
        let mut wake: Option<u64> = None;
        for slot in self.slots.values() {
            let due = if slot.anim.is_some() {
                Some(FRAME_MS)
            } else {
                slot.hide_at.map(|t| t.saturating_sub(now))
            };
            if let Some(due) = due {
                wake = Some(wake.map_or(due, |w| w.min(due)));
            }
        }
        wake
    }
}

// ---------------------------------------------------------------------------
// Public API (cheap: an atomic, a small map insert and a channel send).
// ---------------------------------------------------------------------------

static GENERATION: AtomicU64 = AtomicU64::new(0);
static SENDER: OnceCell<mpsc::Sender<Cmd>> = OnceCell::new();
static APP: OnceCell<AppHandle> = OnceCell::new();
/// Latest envelope per window, pulled by the page on first mount.
static SNAPSHOTS: Lazy<Mutex<HashMap<&'static str, Envelope>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn send(cmd: Cmd) {
    if let Some(sender) = SENDER.get() {
        let _ = sender.send(cmd);
    }
}

/// Show `label` (or update it if already up) with `payload`; returns its generation.
pub(crate) fn show(label: &'static str, page: &'static str, payload: impl serde::Serialize) -> u64 {
    show_with(label, page, payload, None)
}

/// Like [`show`], auto-hiding `auto_hide` after this show (paused while hovered / held);
/// the callback registered with [`on_expire`] then gets the generation.
pub(crate) fn show_with(
    label: &'static str,
    page: &'static str,
    payload: impl serde::Serialize,
    auto_hide: Option<Duration>,
) -> u64 {
    debug_assert!(is_popup_label(label), "popup labels must start with {LABEL_PREFIX}");
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let payload = serde_json::to_value(payload).unwrap_or(Value::Null);
    {
        let mut snapshots = SNAPSHOTS.lock().unwrap_or_else(|e| e.into_inner());
        if snapshots.get(label).is_none_or(|s| s.generation < generation) {
            snapshots.insert(label, Envelope { generation, payload: payload.clone() });
        }
    }
    let auto_hide_ms = auto_hide.map(|d| d.as_millis() as u64);
    send(Cmd::Show { label, page, generation, payload, auto_hide_ms });
    generation
}

/// Idempotent; with `Some(generation)` only hides if that show is still current.
pub(crate) fn hide(label: &'static str, generation: Option<u64>, animate: bool) {
    send(Cmd::Hide { label, generation, animate });
}

/// Hide `after` from now unless a newer show supersedes `generation` first.
pub(crate) fn auto_hide(label: &'static str, generation: u64, after: Duration) {
    send(Cmd::AutoHide { label, generation, after_ms: after.as_millis() as u64 });
}

pub(crate) fn pause(label: &'static str, generation: u64, reason: PauseReason, on: bool) {
    send(Cmd::Pause { label, generation, reason, on });
}

type ExpireHooks = HashMap<&'static str, fn(u64)>;
static EXPIRE_HOOKS: Lazy<Mutex<ExpireHooks>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Called (on the actor thread) when `label`'s auto-hide fires for a generation.
pub(crate) fn on_expire(label: &'static str, hook: fn(u64)) {
    EXPIRE_HOOKS.lock().unwrap_or_else(|e| e.into_inner()).insert(label, hook);
}

/// Pointer entered / left the popup: pauses its auto-hide like a macOS banner.
#[tauri::command]
pub(crate) fn popup_hover(window: WebviewWindow, generation: u64, hovering: bool) {
    if let Some(&label) = SNAPSHOTS.lock().ok().and_then(|s| s.keys().find(|l| **l == window.label()).copied()).as_ref() {
        pause(label, generation, PauseReason::Hover, hovering);
    }
}

#[tauri::command]
pub(crate) fn get_popup_payload(window: WebviewWindow) -> Option<Envelope> {
    SNAPSHOTS.lock().ok()?.get(window.label()).cloned()
}

/// Start the actor and pre-warm the given windows (hidden).
pub(crate) fn init(app: &AppHandle, prewarm: &[(&'static str, &'static str)]) {
    let (tx, rx) = mpsc::channel();
    if SENDER.set(tx).is_err() {
        return;
    }
    let _ = APP.set(app.clone());
    for &(label, page) in prewarm {
        send(Cmd::Prewarm { label, page });
    }
    let app = app.clone();
    thread::Builder::new()
        .name("native-popup".into())
        .spawn(move || run(app, rx))
        .ok();
}

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

struct Live {
    window: WebviewWindow,
    home: (f64, f64),
}

fn run(app: AppHandle, rx: mpsc::Receiver<Cmd>) {
    let epoch = Instant::now();
    let now = || epoch.elapsed().as_millis() as u64;
    let mut machine = Machine::default();
    let mut live: HashMap<&'static str, Live> = HashMap::new();
    loop {
        let first = match machine.next_wake(now()) {
            None => match rx.recv() {
                Ok(cmd) => Some(cmd),
                Err(_) => return,
            },
            Some(ms) => match rx.recv_timeout(Duration::from_millis(ms)) {
                Ok(cmd) => Some(cmd),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        let t = now();
        if let Some(cmd) = first {
            machine.apply(cmd, t);
            // Coalesce everything already queued (latest wins in the machine).
            while let Ok(cmd) = rx.try_recv() {
                machine.apply(cmd, t);
            }
        }
        // A non-key window gets no mouseenter on macOS, so check the pointer only
        // when a timer is due: still over the popup = defer a little (no polling).
        for (label, generation) in machine.due(t) {
            if live.get(label).is_some_and(|l| pointer_over(&app, &l.window)) {
                machine.apply(Cmd::AutoHide { label, generation, after_ms: HOVER_RECHECK_MS }, t);
            }
        }
        for effect in machine.flush(t) {
            if let Some(lost) = perform(&app, &mut live, effect) {
                live.remove(lost);
                machine.apply(Cmd::Lost { label: lost }, t);
            }
        }
    }
}

/// Returns the label if its window is gone.
fn perform(app: &AppHandle, live: &mut HashMap<&'static str, Live>, effect: Effect) -> Option<&'static str> {
    match effect {
        Effect::Create { label, page } => {
            if let Some(created) = create(app, label, page) {
                live.insert(label, created);
                None
            } else {
                Some(label)
            }
        }
        Effect::Emit { label, envelope } => {
            let _ = app.emit_to(EventTarget::webview_window(label), PAYLOAD_EVENT, envelope);
            None
        }
        Effect::Move { label, offset } => {
            let entry = live.get(label)?;
            let (x, y) = entry.home;
            entry
                .window
                .set_position(tauri::LogicalPosition::new(x + offset, y))
                .err()
                .map(|_| label)
        }
        Effect::Show { label } => {
            let entry = live.get_mut(label)?;
            // Re-anchor each time it appears (monitor layout may have changed).
            if let Some(home) = home_position(app) {
                entry.home = home;
            }
            show_without_focus(&entry.window).err().map(|_| label)
        }
        Effect::Hide { label } => {
            let entry = live.get(label)?;
            // Snap back to the parked off-screen slot so a swipe's setPosition
            // cannot leak into the next show.
            let (x, y) = entry.home;
            let _ = entry.window.set_position(tauri::LogicalPosition::new(x + SLIDE, y));
            let _ = entry.window.hide();
            None
        }
        Effect::Expired { label, generation } => {
            let hook = EXPIRE_HOOKS.lock().ok().and_then(|h| h.get(label).copied());
            if let Some(hook) = hook {
                hook(generation);
            }
            None
        }
    }
}

/// Never take key / focus from the main window or other apps.
/// macOS: `orderFrontRegardless` instead of Tauri's `show()` (makeKeyAndOrderFront).
/// Windows: `SW_SHOWNOACTIVATE` (the window is also WS_EX_NOACTIVATE via focusable(false)).
fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let target = window.clone();
        window.run_on_main_thread(move || {
            if let Ok(ptr) = target.ns_window() {
                // SAFETY: Tauri hands out the live NSWindow*; we are on the main thread.
                let ns: &objc2_app_kit::NSWindow = unsafe { &*(ptr as *const objc2_app_kit::NSWindow) };
                ns.setHidesOnDeactivate(false);
                ns.orderFrontRegardless();
            }
        })
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
        let hwnd = window.hwnd()?;
        // SAFETY: valid HWND owned by this window.
        unsafe { ShowWindow(hwnd.0 as _, SW_SHOWNOACTIVATE) };
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        window.show()
    }
}

fn pointer_over(app: &AppHandle, window: &WebviewWindow) -> bool {
    let (Ok(cursor), Ok(pos), Ok(size)) = (app.cursor_position(), window.outer_position(), window.outer_size()) else {
        return false;
    };
    let (x, y) = (cursor.x, cursor.y);
    x >= f64::from(pos.x)
        && y >= f64::from(pos.y)
        && x < f64::from(pos.x) + f64::from(size.width)
        && y < f64::from(pos.y) + f64::from(size.height)
}

fn home_position(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok()??;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let pos = area.position.to_logical::<f64>(scale);
    let size = area.size.to_logical::<f64>(scale);
    let x = pos.x + size.width - WIDTH - MARGIN;
    let y = if cfg!(target_os = "macos") {
        pos.y + MARGIN
    } else {
        pos.y + size.height - HEIGHT - MARGIN
    };
    Some((x, y))
}

/// Build the hidden window on the main thread (vibrancy needs it) and wait.
fn create(app: &AppHandle, label: &'static str, page: &'static str) -> Option<Live> {
    let home = home_position(app).unwrap_or((0.0, 0.0));
    if let Some(window) = app.get_webview_window(label) {
        return Some(Live { window, home });
    }
    let (tx, rx) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let built = WebviewWindowBuilder::new(&handle, label, WebviewUrl::App(page.into()))
            .title("Storm Dock")
            .inner_size(WIDTH, HEIGHT)
            .position(home.0 + SLIDE, home.1)
            .visible(false)
            .decorations(false)
            .transparent(true)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            // Non-activating: showing never steals focus; the first click still hits the button.
            .focusable(false)
            .accept_first_mouse(true)
            // macOS: native shadow follows the vibrancy mask. Windows: an undecorated
            // transparent window's shadow draws a square frame.
            .shadow(cfg!(target_os = "macos"))
            .build()
            .ok();
        #[cfg(target_os = "macos")]
        if let Some(window) = &built {
            use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
            let _ = apply_vibrancy(
                window,
                NSVisualEffectMaterial::Popover,
                Some(NSVisualEffectState::Active),
                Some(RADIUS),
            );
        }
        let _ = tx.send(built);
    })
    .ok()?;
    let window = rx.recv_timeout(Duration::from_secs(10)).ok().flatten()?;
    Some(Live { window, home })
}


/// Debug builds only: `touch /tmp/storm-dock-preview-import` shows the import
/// banner with sample data (no import runs); it auto-hides after 8 s.
/// `touch /tmp/storm-dock-hide-popups` hides every popup.
#[cfg(debug_assertions)]
pub(crate) fn spawn_dev_import_preview_trigger() {
    thread::spawn(move || loop {
        let hide_flag = std::path::Path::new("/tmp/storm-dock-hide-popups");
        if hide_flag.exists() {
            let _ = std::fs::remove_file(hide_flag);
            hide(crate::switcher::runtime::POPUP_LABEL, None, true);
            hide(crate::bot_import_queue::WINDOW_LABEL, None, true);
        }
        let swipe_flag = std::path::Path::new("/tmp/storm-dock-swipe-dismiss");
        if swipe_flag.exists() {
            let _ = std::fs::remove_file(swipe_flag);
            if let Some(app) = APP.get() {
                if let Some(window) = app.get_webview_window(crate::switcher::runtime::POPUP_LABEL) {
                    // Match a finished swipe: window already dragged off-screen, then instant dismiss.
                    if let (Ok(pos), Ok(scale)) = (window.outer_position(), window.scale_factor()) {
                        let x = f64::from(pos.x) / scale;
                        let y = f64::from(pos.y) / scale;
                        let _ = window.set_position(tauri::LogicalPosition::new(x + 460.0, y));
                    }
                    let _ = window.eval("window.dispatchEvent(new Event('storm-dock-swipe-dismiss'))");
                } else {
                    hide(crate::switcher::runtime::POPUP_LABEL, None, false);
                }
            } else {
                hide(crate::switcher::runtime::POPUP_LABEL, None, false);
            }
        }
        let flag = std::path::Path::new("/tmp/storm-dock-preview-import");
        if flag.exists() {
            let _ = std::fs::remove_file(flag);
            if !crate::bot_import_queue::is_running() {
                let generation = show(
                    crate::bot_import_queue::WINDOW_LABEL,
                    crate::bot_import_queue::PAGE,
                    serde_json::json!({ "preview": true }),
                );
                auto_hide(crate::bot_import_queue::WINDOW_LABEL, generation, Duration::from_secs(8));
            }
        }
        thread::sleep(Duration::from_secs(1));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const L: &str = "popup-test";
    const P: &str = "test.html";

    fn show_cmd(generation: u64, n: i64) -> Cmd {
        Cmd::Show { label: L, page: P, generation, payload: json!(n), auto_hide_ms: None }
    }
    fn show_auto(generation: u64, ms: u64) -> Cmd {
        Cmd::Show { label: L, page: P, generation, payload: json!(generation), auto_hide_ms: Some(ms) }
    }
    fn expired(effects: &[Effect]) -> Vec<u64> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Expired { generation, .. } => Some(*generation),
                _ => None,
            })
            .collect()
    }
    fn emits(effects: &[Effect]) -> Vec<u64> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Emit { envelope, .. } => Some(envelope.generation),
                _ => None,
            })
            .collect()
    }
    fn count(effects: &[Effect], f: impl Fn(&Effect) -> bool) -> usize {
        effects.iter().filter(|e| f(e)).count()
    }
    /// Run frames until no animation is pending; returns all effects.
    fn settle(m: &mut Machine, mut now: u64) -> (Vec<Effect>, u64) {
        let mut all = Vec::new();
        while m.slots.values().any(|s| s.anim.is_some()) {
            now += FRAME_MS;
            all.extend(m.flush(now));
        }
        (all, now)
    }
    fn offset(m: &Machine) -> f64 {
        m.slots[L].offset
    }

    #[test]
    fn every_popup_label_shares_the_prefix() {
        assert!(is_popup_label(crate::switcher::runtime::POPUP_LABEL));
        assert!(is_popup_label(crate::bot_import_queue::WINDOW_LABEL));
        assert!(!is_popup_label("main"));
    }

    #[test]
    fn prewarm_creates_hidden_once() {
        let mut m = Machine::default();
        m.apply(Cmd::Prewarm { label: L, page: P }, 0);
        assert_eq!(m.flush(0), vec![Effect::Create { label: L, page: P }]);
        m.apply(Cmd::Prewarm { label: L, page: P }, 1);
        m.apply(show_cmd(1, 1), 1);
        let fx = m.flush(1);
        assert_eq!(count(&fx, |e| matches!(e, Effect::Create { .. })), 0);
        assert_eq!(count(&fx, |e| matches!(e, Effect::Show { .. })), 1);
    }

    #[test]
    fn rapid_shows_coalesce_to_one_emit_and_one_show() {
        let mut m = Machine::default();
        m.apply(Cmd::Prewarm { label: L, page: P }, 0);
        m.flush(0);
        for g in 1..=20 {
            m.apply(show_cmd(g, g as i64), 5);
        }
        let fx = m.flush(5);
        assert_eq!(emits(&fx), vec![20]);
        assert_eq!(count(&fx, |e| matches!(e, Effect::Show { .. })), 1);
        // First frame is off-screen, then it slides in.
        assert_eq!(fx[1], Effect::Move { label: L, offset: SLIDE });
        let (rest, _) = settle(&mut m, 5);
        assert_eq!(count(&rest, |e| matches!(e, Effect::Show { .. } | Effect::Hide { .. } | Effect::Emit { .. })), 0);
        assert_eq!(offset(&m), 0.0);
    }

    #[test]
    fn show_while_visible_only_updates_payload() {
        let mut m = Machine::default();
        m.apply(show_cmd(1, 1), 0);
        m.flush(0);
        let (_, t) = settle(&mut m, 0);
        m.apply(show_cmd(2, 2), t);
        let fx = m.flush(t);
        assert_eq!(fx[0], Effect::Emit { label: L, envelope: Envelope { generation: 2, payload: json!(2) } });
        assert_eq!(count(&fx, |e| matches!(e, Effect::Show { .. } | Effect::Create { .. } | Effect::Hide { .. })), 0);
        // Small nudge back into place, never off-screen.
        let (rest, _) = settle(&mut m, t);
        assert!(fx.iter().chain(&rest).all(|e| !matches!(e, Effect::Move { offset, .. } if *offset > NUDGE)));
        assert_eq!(offset(&m), 0.0);
    }

    #[test]
    fn show_mid_slide_in_does_not_restart_animation() {
        let mut m = Machine::default();
        m.apply(show_cmd(1, 1), 0);
        m.flush(0);
        m.flush(90);
        let mid = offset(&m);
        assert!(mid > 0.0 && mid < SLIDE);
        m.apply(show_cmd(2, 2), 95);
        let fx = m.flush(95);
        assert!(offset(&m) <= mid, "kept moving in, never jumped back");
        assert_eq!(count(&fx, |e| matches!(e, Effect::Show { .. })), 0);
    }

    #[test]
    fn stale_or_older_commands_are_ignored() {
        let mut m = Machine::default();
        m.apply(show_cmd(5, 5), 0);
        m.apply(show_cmd(3, 3), 0); // out-of-order older show
        assert_eq!(emits(&m.flush(0)), vec![5]);
        settle(&mut m, 0);
        m.apply(Cmd::Hide { label: L, generation: Some(4), animate: true }, 500);
        assert!(m.flush(500).is_empty());
        assert!(m.slots[L].want_visible);
    }

    #[test]
    fn hide_is_idempotent_and_animates_out() {
        let mut m = Machine::default();
        m.apply(show_cmd(1, 1), 0);
        m.flush(0);
        let (_, t) = settle(&mut m, 0);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: true }, t);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: true }, t);
        m.apply(Cmd::Hide { label: L, generation: None, animate: true }, t);
        let first = m.flush(t);
        let (rest, _) = settle(&mut m, t);
        let all: Vec<_> = first.into_iter().chain(rest).collect();
        assert_eq!(count(&all, |e| matches!(e, Effect::Hide { .. })), 1);
        assert_eq!(all.last(), Some(&Effect::Hide { label: L }));
        assert_eq!(offset(&m), SLIDE);
    }

    #[test]
    fn instant_hide_after_swipe_skips_animation() {
        let mut m = Machine::default();
        m.apply(show_cmd(1, 1), 0);
        m.flush(0);
        let (_, t) = settle(&mut m, 0);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: false }, t);
        assert_eq!(m.flush(t), vec![Effect::Hide { label: L }]);
        assert_eq!(m.next_wake(t), None);
    }

    #[test]
    fn show_during_hide_animation_retargets_back_in() {
        let mut m = Machine::default();
        m.apply(show_cmd(1, 1), 0);
        m.flush(0);
        let (_, t) = settle(&mut m, 0);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: true }, t);
        m.flush(t);
        m.flush(t + 70);
        let mid = offset(&m);
        assert!(mid > 0.0 && mid < SLIDE);
        m.apply(show_cmd(2, 2), t + 75);
        let fx = m.flush(t + 75);
        assert_eq!(emits(&fx), vec![2]);
        assert_eq!(count(&fx, |e| matches!(e, Effect::Hide { .. } | Effect::Show { .. })), 0);
        let (rest, _) = settle(&mut m, t + 75);
        assert_eq!(count(&rest, |e| matches!(e, Effect::Hide { .. })), 0);
        assert_eq!(offset(&m), 0.0);
        assert!(m.slots[L].shown);
    }

    #[test]
    fn auto_hide_is_generation_guarded() {
        let mut m = Machine::default();
        m.apply(show_cmd(1, 1), 0);
        m.flush(0);
        let (_, t) = settle(&mut m, 0);
        m.apply(Cmd::AutoHide { label: L, generation: 1, after_ms: 5000 }, t);
        assert_eq!(m.next_wake(t), Some(5000));
        // A newer show supersedes the pending auto-hide.
        m.apply(show_cmd(2, 2), t + 1000);
        m.flush(t + 1000);
        let (_, t2) = settle(&mut m, t + 1000);
        assert_eq!(m.next_wake(t2), None);
        // A stale auto-hide for gen 1 is ignored.
        m.apply(Cmd::AutoHide { label: L, generation: 1, after_ms: 10 }, t2);
        assert_eq!(m.next_wake(t2), None);
        // The current one fires.
        m.apply(Cmd::AutoHide { label: L, generation: 2, after_ms: 10 }, t2);
        assert_eq!(expired(&m.flush(t2 + 10)), vec![2]);
        let (rest, _) = settle(&mut m, t2 + 10);
        assert_eq!(rest.last(), Some(&Effect::Hide { label: L }));
    }

    #[test]
    fn lost_window_is_recreated_on_next_frame() {
        let mut m = Machine::default();
        m.apply(show_cmd(1, 1), 0);
        m.flush(0);
        m.apply(Cmd::Lost { label: L }, 10);
        let fx = m.flush(10);
        assert_eq!(fx[0], Effect::Create { label: L, page: P });
        assert_eq!(emits(&fx), vec![1]);
        assert_eq!(count(&fx, |e| matches!(e, Effect::Show { .. })), 1);
    }

    fn assert_reshows(m: &mut Machine, t: u64, generation: u64) {
        m.apply(show_cmd(generation, generation as i64), t);
        let fx = m.flush(t);
        assert_eq!(emits(&fx), vec![generation]);
        assert!(fx.contains(&Effect::Move { label: L, offset: SLIDE }), "{fx:?}");
        assert!(fx.contains(&Effect::Show { label: L }), "{fx:?}");
        let (rest, _) = settle(m, t);
        assert_eq!(rest.last(), Some(&Effect::Move { label: L, offset: 0.0 }), "{rest:?}");
    }

    #[test]
    fn reshows_after_animated_hide_completes() {
        let mut m = Machine::default();
        m.apply(Cmd::Prewarm { label: L, page: P }, 0);
        m.flush(0);
        assert_reshows(&mut m, 10, 1);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: true }, 1000);
        m.flush(1000);
        let (out, t) = settle(&mut m, 1000);
        assert_eq!(out.last(), Some(&Effect::Hide { label: L }));
        assert_reshows(&mut m, t + 500, 2);
    }

    #[test]
    fn reshows_after_instant_hide() {
        let mut m = Machine::default();
        assert_reshows(&mut m, 0, 1);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: false }, 1000);
        assert_eq!(m.flush(1000), vec![Effect::Hide { label: L }]);
        assert_reshows(&mut m, 2000, 2);
    }

    #[test]
    fn auto_hide_restarts_on_each_new_show() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.flush(0);
        settle(&mut m, 0);
        m.apply(show_auto(2, 5000), 4000);
        m.flush(4000);
        settle(&mut m, 4000);
        assert!(expired(&m.flush(5500)).is_empty(), "old timer must not fire");
        let fx = m.flush(9000);
        assert_eq!(expired(&fx), vec![2]);
        let (rest, _) = settle(&mut m, 9000);
        assert_eq!(rest.last(), Some(&Effect::Hide { label: L }));
    }

    #[test]
    fn hover_pauses_and_resumes_with_remaining_time() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.flush(0);
        settle(&mut m, 0);
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hover, on: true }, 2000);
        assert_eq!(m.next_wake(2000), None, "no timer while hovered");
        assert!(expired(&m.flush(60_000)).is_empty());
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hover, on: false }, 60_000);
        assert_eq!(m.next_wake(60_000), Some(3000));
        assert_eq!(expired(&m.flush(63_000)), vec![1]);
    }

    #[test]
    fn hold_and_hover_both_must_end_and_stale_pause_is_ignored() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.flush(0);
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hold, on: true }, 4900);
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hover, on: true }, 4900);
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hold, on: false }, 6000);
        assert!(m.slots[L].hide_at.is_none());
        m.apply(Cmd::Pause { label: L, generation: 7, reason: PauseReason::Hover, on: false }, 6000);
        assert!(m.slots[L].hide_at.is_none(), "stale generation ignored");
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hover, on: false }, 6000);
        // 100 ms were left; resume is clamped to a minimum.
        assert_eq!(m.slots[L].hide_at, Some(6000 + MIN_RESUME_MS));
    }

    #[test]
    fn show_while_hovered_arms_paused_timer() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hover, on: true }, 10);
        m.apply(show_auto(2, 5000), 20);
        assert!(m.slots[L].hide_at.is_none());
        m.apply(Cmd::Pause { label: L, generation: 2, reason: PauseReason::Hover, on: false }, 100);
        assert_eq!(m.slots[L].hide_at, Some(5100));
    }

    #[test]
    fn due_lists_expiring_timers_and_rearm_defers() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.flush(0);
        assert!(m.due(4999).is_empty());
        assert_eq!(m.due(5000), vec![(L, 1)]);
        m.apply(Cmd::AutoHide { label: L, generation: 1, after_ms: 1000 }, 5000);
        assert!(expired(&m.flush(5000)).is_empty());
        assert_eq!(expired(&m.flush(6000)), vec![1]);
    }

    #[test]
    fn instant_hide_clears_hover_and_reshow_arms_fresh_timer() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.flush(0);
        settle(&mut m, 0);
        m.apply(Cmd::Pause { label: L, generation: 1, reason: PauseReason::Hover, on: true }, 1000);
        assert!(m.slots[L].hover);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: false }, 1500);
        m.flush(1500);
        assert!(!m.slots[L].hover, "swipe/instant hide must clear hover");
        assert!(!m.slots[L].hold);
        assert_eq!(m.slots[L].offset, SLIDE);
        m.apply(show_auto(2, 5000), 2000);
        let fx = m.flush(2000);
        assert!(fx.contains(&Effect::Show { label: L }), "{fx:?}");
        assert!(fx.contains(&Effect::Move { label: L, offset: SLIDE }), "{fx:?}");
        assert_eq!(m.slots[L].hide_at, Some(7000));
        assert!(expired(&m.flush(6999)).is_empty());
        assert_eq!(expired(&m.flush(7000)), vec![2]);
    }

    #[test]
    fn show_after_swipe_like_hide_always_reenters_from_slide() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.flush(0);
        let (_, t) = settle(&mut m, 0);
        // Simulate frontend leaving the window far to the right; actor offset still 0.
        m.slots.get_mut(L).unwrap().offset = 460.0;
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: false }, t);
        m.flush(t);
        assert_eq!(m.slots[L].offset, SLIDE);
        m.apply(show_auto(2, 5000), t + 10);
        let fx = m.flush(t + 10);
        assert_eq!(fx.iter().find_map(|e| match e {
            Effect::Move { offset, .. } => Some(*offset),
            _ => None,
        }), Some(SLIDE));
        let (rest, _) = settle(&mut m, t + 10);
        assert_eq!(rest.last(), Some(&Effect::Move { label: L, offset: 0.0 }));
        assert_eq!(m.slots[L].hide_at, Some(t + 10 + 5000));
    }

    #[test]
    fn manual_hide_cancels_auto_hide() {
        let mut m = Machine::default();
        m.apply(show_auto(1, 5000), 0);
        m.flush(0);
        m.apply(Cmd::Hide { label: L, generation: Some(1), animate: false }, 100);
        let (fx, _) = settle(&mut m, 100);
        assert!(expired(&fx).is_empty());
        assert!(expired(&m.flush(10_000)).is_empty());
    }
}
