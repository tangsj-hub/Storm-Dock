use std::{
    collections::HashMap,
    path::PathBuf,
    process::Command,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use once_cell::sync::Lazy;

use super::{
    app_bundle_in, launch_failed, not_installed, quit_failed, quit_timeout, terminate_failed,
    DesktopApp,
};
use crate::error::{AppError, Result};

const PROC_ALL_PIDS: u32 = 1;
const KILL_WAIT: Duration = Duration::from_secs(5);
const QUIT_WAIT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(100);

static INSTALL_CACHE: Lazy<Mutex<HashMap<DesktopApp, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

unsafe extern "C" {
    fn proc_listpids(
        type_: u32,
        typeinfo: u32,
        buffer: *mut libc::c_void,
        buffersize: libc::c_int,
    ) -> libc::c_int;
    fn proc_name(pid: libc::c_int, buffer: *mut libc::c_void, buffersize: u32) -> libc::c_int;
    fn proc_pidpath(pid: libc::c_int, buffer: *mut libc::c_void, buffersize: u32) -> libc::c_int;
}

pub(super) fn resolve(app: DesktopApp) -> Result<PathBuf> {
    if let Ok(cache) = INSTALL_CACHE.lock() {
        if let Some(path) = cache.get(&app) {
            if path.is_dir() {
                return Ok(path.clone());
            }
        }
    }
    let found = discover(app).ok_or_else(|| not_installed(app))?;
    if let Ok(mut cache) = INSTALL_CACHE.lock() {
        cache.insert(app, found.clone());
    }
    Ok(found)
}

fn discover(app: DesktopApp) -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    if let Some(path) = app_bundle_in(app.name(), &roots) {
        return Some(path);
    }
    ls_application_path(app)
}

fn ls_application_path(app: DesktopApp) -> Option<PathBuf> {
    let bundle_id = app.macos_bundle_id()?;
    crate::macos_native::app_path_for_bundle_id(bundle_id)
}

pub(super) fn launch(app: DesktopApp) -> Result<()> {
    match Command::new("open").args(["-a", app.name()]).status() {
        Ok(status) if status.success() => Ok(()),
        _ => Err(launch_failed(app)),
    }
}

pub(super) fn is_running(app: DesktopApp) -> bool {
    match app {
        // WorkBuddy's main process is often named Electron; match the app bundle path.
        DesktopApp::Workbuddy => process_path_contains("WorkBuddy.app"),
        _ => process_named(app.name()),
    }
}

fn each_named_pid(name: &str, mut visit: impl FnMut(i32)) {
    let needed = unsafe { proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return;
    }
    let mut pids = vec![0i32; (needed as usize / std::mem::size_of::<i32>()) + 32];
    let filled = unsafe {
        proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if filled <= 0 {
        return;
    }
    let count = (filled as usize) / std::mem::size_of::<i32>();
    let mut buf = [0u8; 256];
    for pid in pids.iter().take(count).copied().filter(|pid| *pid > 0) {
        let len = unsafe { proc_name(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if len <= 0 {
            continue;
        }
        if std::str::from_utf8(&buf[..len as usize]).is_ok_and(|proc| proc == name) {
            visit(pid);
        }
    }
}

fn process_named(name: &str) -> bool {
    let mut found = false;
    each_named_pid(name, |_| found = true);
    found
}

fn signal_named(name: &str, sig: i32) -> bool {
    let mut signaled = false;
    each_named_pid(name, |pid| {
        if unsafe { libc::kill(pid, sig) } == 0 {
            signaled = true;
        }
    });
    signaled
}

fn signal_app(app: DesktopApp, sig: i32) -> bool {
    match app {
        DesktopApp::Workbuddy => signal_path_contains("WorkBuddy.app", sig),
        _ => signal_named(app.name(), sig),
    }
}

fn each_path_pid(needle: &str, mut visit: impl FnMut(i32)) {
    let needed = unsafe { proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return;
    }
    let mut pids = vec![0i32; (needed as usize / std::mem::size_of::<i32>()) + 32];
    let filled = unsafe {
        proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if filled <= 0 {
        return;
    }
    let count = (filled as usize) / std::mem::size_of::<i32>();
    let mut buf = [0u8; 4096];
    for pid in pids.iter().take(count).copied().filter(|pid| *pid > 0) {
        let len = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if len <= 0 {
            continue;
        }
        if std::str::from_utf8(&buf[..len as usize]).is_ok_and(|path| path.contains(needle)) {
            visit(pid);
        }
    }
}

fn process_path_contains(needle: &str) -> bool {
    let mut found = false;
    each_path_pid(needle, |_| found = true);
    found
}

fn signal_path_contains(needle: &str, sig: i32) -> bool {
    let mut signaled = false;
    each_path_pid(needle, |pid| {
        if unsafe { libc::kill(pid, sig) } == 0 {
            signaled = true;
        }
    });
    signaled
}

pub(super) fn terminate(app: DesktopApp) -> Result<()> {
    if !is_running(app) {
        return Ok(());
    }
    // In-process signal instead of `pkill`, so any system UI stays on Storm Dock.
    if signal_app(app, libc::SIGTERM) {
        return Ok(());
    }
    Err(terminate_failed(app))
}

pub(super) fn wait_until_stopped(app: DesktopApp) -> Result<()> {
    wait_while_running(app, KILL_WAIT, || {
        AppError::Message(format!("{} 未在 5 秒内退出。", app.name()))
    })
}

pub(super) fn quit_and_wait(app: DesktopApp) -> Result<()> {
    if !is_running(app) {
        return Ok(());
    }
    // SIGTERM in-process — no osascript Automation prompt naming a helper tool.
    if !signal_app(app, libc::SIGTERM) {
        return Err(quit_failed(app));
    }
    wait_while_running(app, QUIT_WAIT, || quit_timeout(app))
}

fn wait_while_running(
    app: DesktopApp,
    limit: Duration,
    error: impl FnOnce() -> AppError,
) -> Result<()> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if !is_running(app) {
            return Ok(());
        }
        thread::sleep(POLL);
    }
    Err(error())
}
