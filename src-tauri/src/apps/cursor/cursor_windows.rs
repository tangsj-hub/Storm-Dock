//! Windows process control for Cursor desktop.
//!
//! Do **not** shell out to `tasklist` / `taskkill` / `cmd /C start` from the GUI
//! process: those are console-subsystem tools and each spawn flashes a terminal
//! window. Account switch + force-restart used to call `tasklist` in a tight
//! wait loop, which is why Windows opened many consoles while macOS looked fine.
//!
//! This module mirrors the Grok Bot Windows helpers: Toolhelp snapshots,
//! `CreateProcessW`, and `TerminateProcess`.

use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::error::{AppError, Result};

const EXE_NAME: &str = "Cursor.exe";
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const MAX_PATH: usize = 260;
const INVALID_HANDLE: isize = -1;
const PROCESS_TERMINATE: u32 = 0x0001;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

static EXE_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

#[repr(C)]
struct ProcessEntry32W {
    dw_size: u32,
    cnt_usage: u32,
    th32_process_id: u32,
    th32_default_heap_id: usize,
    th32_module_id: u32,
    cnt_threads: u32,
    th32_parent_process_id: u32,
    pc_pri_class_base: i32,
    dw_flags: u32,
    sz_exe_file: [u16; MAX_PATH],
}

#[repr(C)]
struct StartupInfoW {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    dw_x: u32,
    dw_y: u32,
    dw_x_size: u32,
    dw_y_size: u32,
    dw_x_count_chars: u32,
    dw_y_count_chars: u32,
    dw_fill_attribute: u32,
    dw_flags: u32,
    w_show_window: u16,
    cb_reserved2: u16,
    lp_reserved2: *mut u8,
    h_std_input: *mut core::ffi::c_void,
    h_std_output: *mut core::ffi::c_void,
    h_std_error: *mut core::ffi::c_void,
}

#[repr(C)]
struct ProcessInformation {
    h_process: *mut core::ffi::c_void,
    h_thread: *mut core::ffi::c_void,
    dw_process_id: u32,
    dw_thread_id: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> *mut core::ffi::c_void;
    fn Process32FirstW(snapshot: *mut core::ffi::c_void, entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snapshot: *mut core::ffi::c_void, entry: *mut ProcessEntry32W) -> i32;
    fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> *mut core::ffi::c_void;
    fn TerminateProcess(process: *mut core::ffi::c_void, exit_code: u32) -> i32;
    fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    fn CreateProcessW(
        application_name: *const u16,
        command_line: *mut u16,
        process_attributes: *mut core::ffi::c_void,
        thread_attributes: *mut core::ffi::c_void,
        inherit_handles: i32,
        creation_flags: u32,
        environment: *mut core::ffi::c_void,
        current_directory: *const u16,
        startup_info: *mut StartupInfoW,
        process_information: *mut ProcessInformation,
    ) -> i32;
}

fn process_name_matches(wide: &[u16], expected: &str) -> bool {
    let end = wide.iter().position(|&unit| unit == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end]).eq_ignore_ascii_case(expected)
}

fn process_ids() -> Vec<u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot as isize == INVALID_HANDLE {
        return Vec::new();
    }
    let mut entry = ProcessEntry32W {
        dw_size: std::mem::size_of::<ProcessEntry32W>() as u32,
        cnt_usage: 0,
        th32_process_id: 0,
        th32_default_heap_id: 0,
        th32_module_id: 0,
        cnt_threads: 0,
        th32_parent_process_id: 0,
        pc_pri_class_base: 0,
        dw_flags: 0,
        sz_exe_file: [0; MAX_PATH],
    };
    let mut ids = Vec::new();
    unsafe {
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if process_name_matches(&entry.sz_exe_file, EXE_NAME) {
                    ids.push(entry.th32_process_id);
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    ids
}

pub(super) fn is_running() -> bool {
    !process_ids().is_empty()
}

fn wide_os(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_quoted(path: &Path) -> Vec<u16> {
    let mut out: Vec<u16> = "\"".encode_utf16().collect();
    out.extend(path.as_os_str().encode_wide());
    out.extend("\"\0".encode_utf16());
    out
}

fn candidate_exes() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        out.push(local.join("Programs/cursor/Cursor.exe"));
        out.push(local.join("Programs/Cursor/Cursor.exe"));
    }
    if let Some(pf) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
        out.push(pf.join("Cursor/Cursor.exe"));
        out.push(pf.join("cursor/Cursor.exe"));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from) {
        out.push(pf86.join("Cursor/Cursor.exe"));
    }
    out
}

fn find_exe() -> Result<PathBuf> {
    let mut cache = EXE_PATH
        .lock()
        .map_err(|_| AppError::Message("Cursor 安装路径缓存不可用。".into()))?;
    if let Some(path) = cache.as_ref() {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    let found = candidate_exes()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| AppError::Message("无法启动 Cursor，请确认应用已安装。".into()))?;
    *cache = Some(found.clone());
    Ok(found)
}

fn create_detached(exe: &Path, flags: u32) -> Result<()> {
    let app = wide_os(exe);
    let mut command = wide_quoted(exe);
    let directory = exe
        .parent()
        .map(wide_os)
        .unwrap_or_else(|| wide_os(Path::new(".")));
    let mut startup = unsafe { std::mem::zeroed::<StartupInfoW>() };
    startup.cb = std::mem::size_of::<StartupInfoW>() as u32;
    let mut info = ProcessInformation {
        h_process: std::ptr::null_mut(),
        h_thread: std::ptr::null_mut(),
        dw_process_id: 0,
        dw_thread_id: 0,
    };
    let ok = unsafe {
        CreateProcessW(
            app.as_ptr(),
            command.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            flags,
            std::ptr::null_mut(),
            directory.as_ptr(),
            &mut startup,
            &mut info,
        )
    };
    if ok == 0 {
        return Err(AppError::Message("无法启动 Cursor，请确认应用已安装。".into()));
    }
    unsafe {
        if !info.h_thread.is_null() {
            CloseHandle(info.h_thread);
        }
        if !info.h_process.is_null() {
            CloseHandle(info.h_process);
        }
    }
    Ok(())
}

pub(super) fn launch() -> Result<()> {
    let exe = find_exe()?;
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB;
    if create_detached(&exe, flags).is_ok() {
        return Ok(());
    }
    create_detached(&exe, DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
}

pub(super) fn terminate() -> Result<()> {
    let ids = process_ids();
    if ids.is_empty() {
        return Ok(());
    }
    let mut killed_any = false;
    for pid in ids {
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if handle.is_null() {
            continue;
        }
        let ok = unsafe { TerminateProcess(handle, 1) };
        unsafe {
            CloseHandle(handle);
        }
        if ok != 0 {
            killed_any = true;
        }
    }
    if killed_any {
        Ok(())
    } else {
        Err(AppError::Message("无法结束 Cursor 进程。".into()))
    }
}
