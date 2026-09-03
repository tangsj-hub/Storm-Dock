use std::{
    fs,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::{
    encrypt_os_crypt_windows, parse_lnk_target, write_json_file, QUIT_TIMEOUT,
};
use crate::error::{AppError, Result};

const EXE_NAME: &str = "Grok Bot.exe";
const SHORTCUT_NAME: &str = "Grok Bot.lnk";
const PROCESS_NAME: &str = "Grok Bot.exe";
const DPAPI_PREFIX: &[u8] = b"DPAPI";
const TH32CS_SNAPPROCESS: u32 = 0x00000002;
const MAX_PATH: usize = 260;
const WM_CLOSE: u32 = 0x0010;
const PROCESS_SYNCHRONIZE: u32 = 0x0010_0000;
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 0x0000_0102;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const INVALID_HANDLE: isize = -1;

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
    fn OpenProcess(
        access: u32,
        inherit: i32,
        process_id: u32,
    ) -> *mut core::ffi::c_void;
    fn WaitForSingleObject(handle: *mut core::ffi::c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    fn LocalFree(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
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

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(
        callback: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
        lparam: isize,
    ) -> i32;
    fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, process_id: *mut u32) -> u32;
    fn PostMessageW(hwnd: *mut core::ffi::c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
}

pub(crate) fn data_path() -> Result<PathBuf> {
    user_data_dir().map(|dir| dir.join("sand-secrets.json"))
}

fn user_data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|dir| dir.join("Grok Bot"))
        .ok_or_else(|| AppError::Message("无法读取用户目录。".into()))
}

fn local_state_path() -> Result<PathBuf> {
    user_data_dir().map(|dir| dir.join("Local State"))
}

pub(crate) fn ensure_installed() -> Result<()> {
    find_exe().map(|_| ())
}

fn find_exe() -> Result<PathBuf> {
    let mut cache = EXE_PATH
        .lock()
        .map_err(|_| AppError::Message("Grok Bot 安装路径缓存不可用。".into()))?;
    if let Some(path) = cache.as_ref() {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    let found = discover_exe()?;
    *cache = Some(found.clone());
    Ok(found)
}

fn discover_exe() -> Result<PathBuf> {
    if let Some(path) = candidate_exes().into_iter().find(|path| path.is_file()) {
        return Ok(path);
    }
    for shortcut in shortcut_paths() {
        if let Some(target) = read_lnk_target(&shortcut) {
            if target.is_file() {
                return Ok(target);
            }
        }
    }
    Err(AppError::Message("未安装 Grok Bot。".into()))
}

fn candidate_exes() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        paths.push(local.join(r"Programs\Grok Bot").join(EXE_NAME));
        paths.push(local.join("Grok Bot").join(EXE_NAME));
    }
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(key) {
            paths.push(PathBuf::from(root).join("Grok Bot").join(EXE_NAME));
        }
    }
    paths
}

fn shortcut_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        paths.push(
            PathBuf::from(appdata)
                .join(r"Microsoft\Windows\Start Menu\Programs")
                .join(SHORTCUT_NAME),
        );
    }
    if let Some(program_data) = std::env::var_os("ProgramData") {
        paths.push(
            PathBuf::from(program_data)
                .join(r"Microsoft\Windows\Start Menu\Programs")
                .join(SHORTCUT_NAME),
        );
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join("Desktop").join(SHORTCUT_NAME));
    }
    if let Some(public) = std::env::var_os("PUBLIC") {
        paths.push(PathBuf::from(public).join("Desktop").join(SHORTCUT_NAME));
    }
    paths
}

fn read_lnk_target(path: &Path) -> Option<PathBuf> {
    parse_lnk_target(&fs::read(path).ok()?)
}

pub(crate) fn encrypt_account_fields(
    access: &str,
    refresh: &str,
    profile: &str,
) -> Result<(String, String, String)> {
    let key = os_crypt_key()?;
    Ok((
        encrypt_os_crypt_windows(access, &key),
        encrypt_os_crypt_windows(refresh, &key),
        encrypt_os_crypt_windows(profile, &key),
    ))
}

fn os_crypt_key() -> Result<[u8; 32]> {
    let path = local_state_path()?;
    match fs::read(&path) {
        Ok(bytes) => {
            let mut root: Value = serde_json::from_slice(&bytes)
                .map_err(|_| AppError::Message("Grok Bot Local State 格式无效。".into()))?;
            if let Some(key) = read_encrypted_key(&root)? {
                return Ok(key);
            }
            let key = generate_os_crypt_key()?;
            write_encrypted_key(&mut root, &key)?;
            write_json_file(&path, &root)?;
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = generate_os_crypt_key()?;
            let mut root = serde_json::json!({});
            write_encrypted_key(&mut root, &key)?;
            write_json_file(&path, &root)?;
            Ok(key)
        }
        Err(error) => Err(error.into()),
    }
}

fn read_encrypted_key(root: &Value) -> Result<Option<[u8; 32]>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let Some(encoded) = root
        .pointer("/os_crypt/encrypted_key")
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let raw = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::Message("Grok Bot OSCrypt 密钥格式无效。".into()))?;
    if !raw.starts_with(DPAPI_PREFIX) {
        return Err(AppError::Message(
            "Grok Bot 使用了无法适配的 Windows 加密方式。".into(),
        ));
    }
    let unprotected = dpapi_unprotect(&raw[DPAPI_PREFIX.len()..])?;
    let key: [u8; 32] = unprotected
        .try_into()
        .map_err(|_| AppError::Message("Grok Bot OSCrypt 密钥长度无效。".into()))?;
    Ok(Some(key))
}

fn write_encrypted_key(root: &mut Value, key: &[u8; 32]) -> Result<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let mut payload = DPAPI_PREFIX.to_vec();
    payload.extend(dpapi_protect(key)?);
    if !root.is_object() {
        *root = serde_json::json!({});
    }
    root["os_crypt"]["encrypted_key"] = Value::String(STANDARD.encode(payload));
    Ok(())
}

fn generate_os_crypt_key() -> Result<[u8; 32]> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut key = [0u8; 32];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| AppError::Message("无法生成 Grok Bot 加密密钥。".into()))?;
    Ok(key)
}

pub(super) fn process_name_matches(wide: &[u16], expected: &str) -> bool {
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
                if process_name_matches(&entry.sz_exe_file, PROCESS_NAME) {
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

pub(crate) fn is_running() -> bool {
    !process_ids().is_empty()
}

pub(crate) fn launch() -> Result<()> {
    let exe = find_exe()?;
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB;
    if create_detached(&exe, flags).is_ok() {
        return Ok(());
    }
    create_detached(&exe, DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
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
        return Err(AppError::Message("无法启动 Grok Bot。".into()));
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

fn wide_os(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn wide_quoted(path: &Path) -> Vec<u16> {
    let raw = path.to_string_lossy().replace('"', "\"\"");
    format!("\"{raw}\"")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

struct CloseTargets {
    pids: Vec<u32>,
    hwnds: Vec<*mut core::ffi::c_void>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
    let targets = unsafe { &mut *(lparam as *mut CloseTargets) };
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if targets.pids.contains(&pid) {
        targets.hwnds.push(hwnd);
    }
    1
}

pub(crate) fn quit_and_wait() -> Result<()> {
    let pids = process_ids();
    if pids.is_empty() {
        return Ok(());
    }
    let mut handles = Vec::new();
    for pid in &pids {
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, *pid) };
        if !handle.is_null() && handle as isize != INVALID_HANDLE {
            handles.push(handle);
        }
    }
    let mut targets = CloseTargets {
        pids,
        hwnds: Vec::new(),
    };
    unsafe {
        EnumWindows(enum_windows_proc, &mut targets as *mut CloseTargets as isize);
        for hwnd in targets.hwnds {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
    if handles.is_empty() {
        return if is_running() {
            Err(AppError::Message("Grok Bot 未能接受正常退出请求。".into()))
        } else {
            Ok(())
        };
    }
    let deadline = Instant::now() + QUIT_TIMEOUT;
    let mut remaining = handles;
    while !remaining.is_empty() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let wait_ms = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
        let handle = remaining[0];
        let status = unsafe { WaitForSingleObject(handle, wait_ms) };
        match status {
            WAIT_OBJECT_0 => {
                unsafe {
                    CloseHandle(handle);
                }
                remaining.remove(0);
            }
            WAIT_TIMEOUT => break,
            _ => {
                unsafe {
                    CloseHandle(handle);
                }
                remaining.remove(0);
            }
        }
    }
    for handle in remaining {
        unsafe {
            CloseHandle(handle);
        }
    }
    if Instant::now() < deadline {
        let leftover = deadline.saturating_duration_since(Instant::now());
        wait_until_gone(leftover);
    }
    if is_running() {
        Err(AppError::Message(
            "Grok Bot 未在等待时间内退出，未修改登录会话。".into(),
        ))
    } else {
        Ok(())
    }
}

fn wait_until_gone(limit: Duration) {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if !is_running() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>> {
    dpapi(plain, true)
}

#[cfg(test)]
pub(super) fn dpapi_unprotect_for_test(blob: &[u8]) -> Result<Vec<u8>> {
    dpapi_unprotect(blob)
}

#[cfg(test)]
pub(super) fn dpapi_protect_for_test(plain: &[u8]) -> Result<Vec<u8>> {
    dpapi_protect(plain)
}

fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>> {
    dpapi(blob, false)
}

fn dpapi(input: &[u8], protect: bool) -> Result<Vec<u8>> {
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            description: *const u16,
            optional_entropy: *const DataBlob,
            reserved: *mut core::ffi::c_void,
            prompt: *mut core::ffi::c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            description: *mut *mut u16,
            optional_entropy: *const DataBlob,
            reserved: *mut core::ffi::c_void,
            prompt: *mut core::ffi::c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }

    let input_blob = DataBlob {
        cb_data: input.len() as u32,
        pb_data: input.as_ptr() as *mut u8,
    };
    let mut output = DataBlob {
        cb_data: 0,
        pb_data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &input_blob,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input_blob,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(AppError::Message(
            "无法使用 Windows DPAPI 处理 Grok Bot 密钥。".into(),
        ));
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) }.to_vec();
    unsafe {
        LocalFree(output.pb_data.cast());
    }
    Ok(bytes)
}
