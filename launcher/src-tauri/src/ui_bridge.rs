use serde::Serialize;
use serde_json::json;
use tokio::sync::broadcast;

#[derive(Clone)]
pub enum UiHandle {
    #[cfg(feature = "desktop")]
    Tauri(tauri::AppHandle),
    Web(broadcast::Sender<String>),
}

impl UiHandle {
    #[cfg(feature = "desktop")]
    pub fn from_tauri(app: tauri::AppHandle) -> Self {
        Self::Tauri(app)
    }

    pub fn from_web(tx: broadcast::Sender<String>) -> Self {
        Self::Web(tx)
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String> {
        match self {
            #[cfg(feature = "desktop")]
            Self::Tauri(app) => {
                use tauri::Emitter;
                app.emit(event, payload).map_err(|e| e.to_string())
            }
            Self::Web(tx) => {
                let val = serde_json::to_value(payload).map_err(|e| e.to_string())?;
                let msg = serde_json::to_string(&json!({
                    "event": event,
                    "payload": val
                }))
                .map_err(|e| e.to_string())?;
                let _ = tx.send(msg);
                Ok(())
            }
        }
    }
}

/// Checks whether Microsoft Edge WebView2 Runtime is installed (or bundled beside exe).
#[cfg(feature = "desktop")]
pub fn has_webview2_runtime() -> bool {
    #[cfg(not(windows))]
    {
        true
    }
    #[cfg(windows)]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join("WebView2").join("msedgewebview2.exe").is_file() {
                    return true;
                }
            }
        }

        const SUBKEYS: &[&str] = &[
            r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
            r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ];
        const HKEY_CURRENT_USER: isize = -2147483647;
        const HKEY_LOCAL_MACHINE: isize = -2147483646;

        for &hive in &[HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
            for subkey in SUBKEYS {
                if let Some(pv) = reg_read_sz(hive, subkey, "pv") {
                    if !pv.trim().is_empty() && pv.trim() != "0.0.0.0" {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(all(windows, feature = "desktop"))]
fn reg_read_sz(hive: isize, subkey: &str, value_name: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    const KEY_READ: u32 = 0x20019;
    const REG_SZ: u32 = 1;
    const REG_EXPAND_SZ: u32 = 2;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hkey: isize,
            lp_sub_key: *const u16,
            ul_options: u32,
            sam_desired: u32,
            phk_result: *mut isize,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: isize,
            lp_value_name: *const u16,
            lp_reserved: *mut u32,
            lp_type: *mut u32,
            lp_data: *mut u8,
            lpcb_data: *mut u32,
        ) -> i32;
        fn RegCloseKey(hkey: isize) -> i32;
    }

    let subkey_w: Vec<u16> = OsStr::new(subkey).encode_wide().chain(Some(0)).collect();
    let val_w: Vec<u16> = OsStr::new(value_name).encode_wide().chain(Some(0)).collect();
    let mut hkey: isize = 0;

    unsafe {
        if RegOpenKeyExW(hive, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return None;
        }
        let mut reg_type: u32 = 0;
        let mut size: u32 = 0;
        let status = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            std::ptr::null_mut(),
            &mut size,
        );
        if status != 0 || size == 0 || (reg_type != REG_SZ && reg_type != REG_EXPAND_SZ) {
            RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u16; (size as usize + 1) / 2];
        let status2 = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            buf.as_mut_ptr() as *mut u8,
            &mut size,
        );
        RegCloseKey(hkey);
        if status2 != 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}
