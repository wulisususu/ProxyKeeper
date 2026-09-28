#![windows_subsystem = "windows"]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use clipboard_win::get_clipboard_string;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM, HANDLE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_INFO, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIIF_INFO, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DispatchMessageW, DestroyMenu,
    GetCursorPos, GetMessageW, GetSystemMetrics, LoadIconW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, CreateIconFromResourceEx, HMENU,
    HICON, IDI_APPLICATION, LR_DEFAULTCOLOR, MF_STRING, MSG, SM_CXSMICON, SW_SHOWNORMAL,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CONTEXTMENU,
    WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
};
use winreg::enums::*;
use winreg::{HKEY, RegKey};

const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const PROXY_ENABLE: u32 = 1;
const PROXY_SERVER: &str = "127.0.0.1:7897";

const DEFAULT_OVERRIDE_ITEMS: &str = "localhost;127.*;192.168.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;www.openkylin.top;115.190.225.138";

const REG_NOTIFY_CHANGE_LAST_SET: u32 = 0x0004;
const INTERNET_OPTION_REFRESH: u32 = 37;
const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;

const TRAY_CALLBACK: u32 = 0x8001;
const TRAY_ID: u32 = 1;
const MENU_OPEN_CONFIG: usize = 1;
const MENU_EXIT: usize = 2;

static APPLY_LOCK: Mutex<()> = Mutex::new(());
static MAIN_HWND: OnceLock<isize> = OnceLock::new();

#[link(name = "wininet")]
extern "system" {
    fn InternetSetOptionW(
        h: *mut std::ffi::c_void,
        opt: u32,
        buf: *mut std::ffi::c_void,
        len: u32,
    ) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegNotifyChangeKeyValue(
        hkey: isize,
        watch_subtree: i32,
        filter: u32,
        event: isize,
        asynchronous: i32,
    ) -> i32;
}

fn log(msg: &str) {
    if let Some(dir) = exe_dir() {
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("proxy-keeper.log"))
        {
            let t = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = writeln!(f, "[{t}] {msg}");
        }
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

fn config_path() -> Option<PathBuf> {
    exe_dir().map(|d| d.join("bypass.txt"))
}

fn ensure_config() {
    if let Some(p) = config_path() {
        if !p.exists() {
            let content = DEFAULT_OVERRIDE_ITEMS.split(';').collect::<Vec<_>>().join("\r\n");
            let _ = std::fs::write(&p, content + "\r\n");
        }
    }
}

fn load_override() -> String {
    if let Some(p) = config_path() {
        if let Ok(s) = std::fs::read_to_string(&p) {
            let items: Vec<&str> = s
                .lines()
                .map(str::trim)
                .map(|l| l.trim_end_matches(';'))
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter(|l| l.is_ascii() && !l.contains(' '))
                .collect();
            if !items.is_empty() {
                return format!("{};<local>", items.join(";"));
            }
        }
    }
    format!("{DEFAULT_OVERRIDE_ITEMS};<local>")
}

fn refresh_wininet() {
    unsafe {
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null_mut(), 0);
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null_mut(), 0);
    }
}

fn current_matches(key: &RegKey) -> bool {
    let enable: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    let server: String = key.get_value("ProxyServer").unwrap_or_default();
    let over: String = key.get_value("ProxyOverride").unwrap_or_default();
    enable == PROXY_ENABLE && server == PROXY_SERVER && over == load_override()
}

fn apply() -> Result<(), Box<dyn std::error::Error>> {
    let _g = APPLY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(SUBKEY, KEY_SET_VALUE)?;
    key.set_value("ProxyEnable", &PROXY_ENABLE)?;
    key.set_value("ProxyServer", &PROXY_SERVER)?;
    key.set_value("ProxyOverride", &load_override())?;
    refresh_wininet();
    Ok(())
}

fn file_mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn watch_registry() {
    std::thread::spawn(|| {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(key) = hkcu.open_subkey_with_flags(SUBKEY, KEY_QUERY_VALUE | KEY_NOTIFY) else {
            log("open key failed");
            return;
        };
        let handle: HKEY = key.raw_handle();
        loop {
            let rc = unsafe { RegNotifyChangeKeyValue(handle, 0, REG_NOTIFY_CHANGE_LAST_SET, 0, 0) };
            if rc != 0 {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            std::thread::sleep(Duration::from_millis(300));
            if !current_matches(&key) {
                log("proxy settings changed externally, restoring");
                if let Err(e) = apply() {
                    log(&format!("restore failed: {e}"));
                }
            }
        }
    });
}

fn watch_config() {
    std::thread::spawn(|| {
        let Some(p) = config_path() else { return };
        let mut last = file_mtime(&p);
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let cur = file_mtime(&p);
            if cur != last {
                last = cur;
                log("bypass.txt changed, re-applying");
                if let Err(e) = apply() {
                    log(&format!("re-apply after config change failed: {e}"));
                }
            }
        }
    });
}

fn extract_host(input: &str) -> Option<String> {
    let mut s = input.trim();
    if let Some(pos) = s.find("://") {
        s = &s[pos + 3..];
    }
    s = s.split(['/', '?', '#']).next()?;
    s = s.rsplit('@').next()?;
    let s = s.trim();
    if s.is_empty() || s.contains(' ') {
        return None;
    }
    let host = if let Some(rest) = s.strip_prefix('[') {
        rest.split(']').next()?
    } else {
        s.split(':').next()?
    };
    if host.is_empty() || !host.is_ascii() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

fn add_bypass(host: &str) -> Result<bool, Box<dyn std::error::Error>> {
    ensure_config();
    let p = config_path().ok_or("no config path")?;
    let existing: Vec<String> = std::fs::read_to_string(&p)?
        .lines()
        .map(str::trim)
        .map(|l| l.trim_end_matches(';'))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_ascii_lowercase())
        .collect();
    let host_lc = host.to_ascii_lowercase();
    if existing.contains(&host_lc) {
        return Ok(false);
    }
    let mut f = OpenOptions::new().append(true).open(&p)?;
    writeln!(f, "{host}")?;
    Ok(true)
}

fn process_clipboard() -> String {
    let text = match get_clipboard_string() {
        Ok(t) => t,
        Err(_) => return "剪贴板中没有文本".into(),
    };
    match extract_host(&text) {
        Some(host) => match add_bypass(&host) {
            Ok(true) => {
                let _ = apply();
                format!("已添加直连: {host}")
            }
            Ok(false) => format!("已存在: {host}"),
            Err(e) => format!("写入失败: {e}"),
        },
        None => "无法从剪贴板内容识别地址".into(),
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_str16(dst: &mut [u16], src: &str) {
    for (i, c) in src.encode_utf16().take(dst.len() - 1).enumerate() {
        dst[i] = c;
    }
}

fn balloon(msg: &str) {
    if let Some(&h) = MAIN_HWND.get() {
        let mut nid = NOTIFYICONDATAW::default();
        nid.hWnd = HWND(h as *mut _);
        nid.uID = TRAY_ID;
        nid.uFlags = NIF_INFO;
        set_str16(&mut nid.szInfo, msg);
        set_str16(&mut nid.szInfoTitle, "ProxyKeeper");
        nid.dwInfoFlags = NIIF_INFO;
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }
}

fn open_config() {
    if let Some(p) = config_path() {
        let path = wide(&p.to_string_lossy());
        unsafe {
            ShellExecuteW(
                HWND::default(),
                PCWSTR::from_raw(wide("open").as_ptr()),
                PCWSTR::from_raw(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == TRAY_CALLBACK {
        let mouse = lparam.0 as u32 & 0xFFFF;
        if mouse == WM_LBUTTONUP || mouse == WM_LBUTTONDBLCLK {
            let result = process_clipboard();
            balloon(&result);
        } else if mouse == WM_RBUTTONUP || mouse == WM_CONTEXTMENU {
            show_menu(hwnd);
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else { return };
    let item_open = wide("打开 bypass.txt");
    let item_exit = wide("退出");
    let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN_CONFIG, PCWSTR::from_raw(item_open.as_ptr()));
    let _ = AppendMenuW(menu, MF_STRING, MENU_EXIT, PCWSTR::from_raw(item_exit.as_ptr()));
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
    match cmd.0 as usize {
        MENU_OPEN_CONFIG => open_config(),
        MENU_EXIT => {
            let mut nid = NOTIFYICONDATAW::default();
            nid.hWnd = hwnd;
            nid.uID = TRAY_ID;
            nid.uFlags = NIF_ICON;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            PostQuitMessage(0);
        }
        _ => {}
    }
}

fn load_app_icon() -> Option<HICON> {
    let data: &[u8] = include_bytes!("../assets/icon.ico");
    let count = u16::from_le_bytes([data[4], data[5]]) as usize;
    let want = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as usize;
    let mut best: Option<(usize, usize, usize)> = None;
    for i in 0..count {
        let e = &data[6 + i * 16..6 + i * 16 + 16];
        let w = if e[0] == 0 { 256 } else { e[0] as usize };
        let Ok(len) = <[u8; 4]>::try_from(&e[8..12]) else { continue };
        let Ok(off) = <[u8; 4]>::try_from(&e[12..16]) else { continue };
        let len = u32::from_le_bytes(len) as usize;
        let off = u32::from_le_bytes(off) as usize;
        let diff = w.abs_diff(want);
        if best.is_none() || diff < best.unwrap().0 {
            best = Some((diff, off, len));
        }
    }
    let (_, off, len) = best?;
    unsafe {
        CreateIconFromResourceEx(
            &data[off..off + len],
            true,
            0x00030000,
            0,
            0,
            LR_DEFAULTCOLOR,
        )
        .ok()
    }
}

fn add_tray_icon(hwnd: HWND, hinstance: HINSTANCE) {
    unsafe {
        let icon = load_app_icon().unwrap_or_else(|| LoadIconW(None, IDI_APPLICATION).unwrap_or_default());
        let mut nid = NOTIFYICONDATAW::default();
        nid.hWnd = hwnd;
        nid.uID = TRAY_ID;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = TRAY_CALLBACK;
        nid.hIcon = icon;
        set_str16(&mut nid.szTip, "ProxyKeeper 点击添加直连地址");
        Shell_NotifyIconW(NIM_ADD, &nid);
    }
    let _ = hinstance;
}

fn run_gui() {
    unsafe {
        let hinstance = HINSTANCE(GetModuleHandleW(None).unwrap().0);
        let class_name = wide("ProxyKeeperWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
            hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR::from_raw(class_name.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            HWND::default(),
            HMENU::default(),
            hinstance,
            None,
        ) else {
            log("create window failed");
            return;
        };
        let _ = MAIN_HWND.set(hwnd.0 as isize);
        add_tray_icon(hwnd, hinstance);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        log("exit");
    }
}

fn run_add_mode(arg: Option<&String>) {
    let result = match arg
        .map(|s| s.clone())
        .or_else(|| get_clipboard_string().ok())
        .as_deref()
        .and_then(extract_host)
    {
        Some(host) => match add_bypass(&host) {
            Ok(true) => {
                let _ = apply();
                format!("已添加直连: {host}")
            }
            Ok(false) => format!("已存在: {host}"),
            Err(e) => format!("写入失败: {e}"),
        },
        None => "无法识别地址".into(),
    };
    log(&format!("add mode: {result}"));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "add" {
        run_add_mode(args.get(2));
        return;
    }

    let Ok(_mutex) = (unsafe {
        CreateMutexW(None, false, PCWSTR::from_raw(wide("ProxyKeeper_Instance").as_ptr()))
    }) else {
        return;
    };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }

    ensure_config();
    if let Err(e) = apply() {
        log(&format!("initial apply failed: {e}"));
    }
    watch_registry();
    watch_config();
    log("started");
    run_gui();
}
