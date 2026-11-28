#![cfg_attr(all(not(debug_assertions), feature = "desktop"), windows_subsystem = "windows")]

mod ui_bridge;
mod webui_server;

use base64::Engine as _;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ui_bridge::UiHandle;

const CONFIG_FILE: &str = "repo_config.json";
const MANIFEST_FILE: &str = "manifest.json";
const SYNC_STATE_FILE: &str = ".eternum-sync.json";
const MODRINTH_API: &str = "https://api.modrinth.com/v2/version_files";
const USER_AGENT: &str = "Eternum-ModpackManager/1.0";

const EXTRA_DIRS: &[&str] = &[
    "config",
    "defaultconfigs",
    "resourcepacks",
    "shaderpacks",
    "kubejs",
    "datapacks",
];
const EXTRA_ROOT_FILES: &[&str] = &["options.txt", "servers.dat"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    #[serde(default = "default_repo_owner")]
    pub repo_owner: String,
    #[serde(default = "default_repo_name")]
    pub repo_name: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default = "default_pack_version")]
    pub pack_version: String,
    #[serde(default = "default_mc_version")]
    pub minecraft_version: String,
    #[serde(default = "default_loader_type")]
    pub loader_type: String,
    #[serde(default = "default_loader_version")]
    pub loader_version: String,
    #[serde(default = "default_server_name")]
    pub server_name: String,
    #[serde(default = "default_server_ip")]
    pub server_ip: String,
    #[serde(default)]
    pub bundles_extract_map: BTreeMap<String, String>,
}

fn default_repo_owner() -> String {
    "meed0ff".into()
}
fn default_repo_name() -> String {
    "eternum-modpack".into()
}
fn default_branch() -> String {
    "main".into()
}
fn default_pack_version() -> String {
    "1.0".into()
}
fn default_mc_version() -> String {
    "1.21.1".into()
}
fn default_loader_type() -> String {
    "neoforge".into()
}
fn default_loader_version() -> String {
    "21.1.267".into()
}
fn default_server_name() -> String {
    "Eternum".into()
}
fn default_server_ip() -> String {
    "eternum.aurorix.pro".into()
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            repo_owner: default_repo_owner(),
            repo_name: default_repo_name(),
            branch: default_branch(),
            pack_version: default_pack_version(),
            minecraft_version: default_mc_version(),
            loader_type: default_loader_type(),
            loader_version: default_loader_version(),
            server_name: default_server_name(),
            server_ip: default_server_ip(),
            bundles_extract_map: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModpackOverview {
    pub dir: String,
    pub has_config: bool,
    pub config: RepoConfig,
    pub mods_count: usize,
    pub extra_count: usize,
    pub bundles_count: usize,
    pub local_size_bytes: u64,
    pub free_disk_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenProgress {
    pub percent: u32,
    pub status: String,
    pub log: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadStats {
    pub file_name: Option<String>,
    pub file_downloaded: u64,
    pub file_total: u64,
    pub total_downloaded: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
}

#[derive(Debug, Serialize)]
pub struct GenerateSummary {
    pub manifest_path: String,
    pub modrinth_count: usize,
    pub github_count: usize,
    pub extra_count: usize,
    pub bundle_count: usize,
    pub total_files: usize,
}

#[derive(Debug, Serialize)]
pub struct GithubAccessInfo {
    pub login: Option<String>,
    pub can_push: bool,
    pub private_repo: bool,
}

#[derive(Debug, Serialize)]
pub struct PublishSummary {
    pub commit_sha: String,
    pub uploaded_count: usize,
    pub reused_count: usize,
    pub removed_count: usize,
    pub no_changes: bool,
    pub repo_url: String,
}

fn default_provider() -> String {
    "github".into()
}

fn normalize_provider(provider: Option<&str>) -> &'static str {
    match provider.map(|s| s.trim()) {
        Some(p) if p.eq_ignore_ascii_case("gitlab") => "gitlab",
        _ => "github",
    }
}

fn provider_label(provider: &str) -> &'static str {
    if provider.eq_ignore_ascii_case("gitlab") {
        "GitLab"
    } else {
        "GitHub"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub branch: String,
    pub commit_sha: String,
    #[serde(default)]
    pub pack_version: String,
}

#[derive(Debug, Serialize)]
pub struct SyncCheckResult {
    pub status: String,
    pub local_sha: Option<String>,
    pub remote_sha: Option<String>,
    pub remote_message: Option<String>,
    pub remote_author: Option<String>,
    pub remote_date: Option<String>,
    pub message: String,
    pub total_pack_bytes: Option<u64>,
    pub needed_bytes: Option<u64>,
    pub free_disk_bytes: Option<u64>,
    pub enough_disk_space: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct DownloadSummary {
    pub dir: String,
    pub commit_sha: String,
    pub pack_version: String,
    pub downloaded_github: usize,
    pub downloaded_modrinth: usize,
    pub skipped_cached: usize,
    pub removed_stale: usize,
}

#[derive(Debug, Serialize)]
struct ManifestFile {
    path: String,
    source: String,
    url: String,
    sha256: String,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none", rename = "extractTo")]
    extract_to: Option<String>,
}

#[derive(Debug, Serialize)]
struct ServerConfig {
    name: String,
    ip: String,
}

#[derive(Debug, Serialize)]
struct MinecraftConfig {
    version: String,
}

#[derive(Debug, Serialize)]
struct LoaderConfig {
    #[serde(rename = "type")]
    loader_type: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    installer_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct ElybyConfig {
    #[serde(rename = "clientId")]
    client_id: String,
}

#[derive(Debug, Serialize)]
struct Manifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "packVersion")]
    pack_version: String,
    server: ServerConfig,
    minecraft: MinecraftConfig,
    loader: LoaderConfig,
    elyby: ElybyConfig,
    files: Vec<ManifestFile>,
}

struct ModEntry {
    filename: String,
    rel_path: String,
    sha256: String,
    sha512: String,
    size: u64,
}

fn is_modpack_dir(dir: &Path) -> bool {
    dir.join(CONFIG_FILE).exists() || dir.join("mods").is_dir()
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            collect_files_recursive(&path, out)?;
        } else if path.is_file() {
            if path.file_name().and_then(|n| n.to_str()) == Some(".gitkeep") {
                continue;
            }
            out.push(path);
        }
    }
    Ok(())
}

fn make_github_raw_url(config: &RepoConfig, rel_path: &str) -> String {
    let encoded: String = rel_path
        .split('/')
        .map(|seg| urlencoding::encode(seg).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        config.repo_owner.trim(),
        config.repo_name.trim(),
        config.branch.trim(),
        encoded
    )
}

fn compute_hashes(path: &Path) -> Result<(String, String, u64), String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("Не удалось открыть {}: {}", path.display(), e))?;
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut size: u64 = 0;
    let mut buf = [0u8; 65536];

    loop {
        let read = file
            .read(&mut buf)
            .map_err(|e| format!("Ошибка чтения {}: {}", path.display(), e))?;
        if read == 0 {
            break;
        }
        sha256.update(&buf[..read]);
        sha512.update(&buf[..read]);
        size += read as u64;
    }

    Ok((
        hex::encode(sha256.finalize()),
        hex::encode(sha512.finalize()),
        size,
    ))
}

fn build_installer_url(loader_type: &str, mc_version: &str, loader_version: &str) -> Option<String> {
    match loader_type.trim().to_lowercase().as_str() {
        "fabric" => Some(format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
            mc_version.trim(),
            loader_version.trim()
        )),
        "neoforge" => Some(format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            loader_version.trim(),
            loader_version.trim()
        )),
        "forge" => Some(format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}-{}/forge-{}-{}-installer.jar",
            mc_version.trim(),
            loader_version.trim(),
            mc_version.trim(),
            loader_version.trim()
        )),
        _ => None,
    }
}

fn neoforge_prefix_for_mc(mc_version: &str) -> Option<String> {
    let mc = mc_version.trim();
    if let Some(rest) = mc.strip_prefix("1.") {
        // e.g. 1.21.1 -> 21.1., 1.21 -> 21.0., 1.20.4 -> 20.4.
        let mut parts = rest.split('.');
        let minor = parts.next()?;
        let patch = parts.next().unwrap_or("0");
        Some(format!("{}.{}.", minor, patch))
    } else if !mc.is_empty() {
        // e.g. 26.3 -> 26.3.
        Some(format!("{}.", mc))
    } else {
        None
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn detect_initial_dir() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if is_modpack_dir(parent) {
                return parent.to_string_lossy().to_string();
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if is_modpack_dir(&cwd) {
            return cwd.to_string_lossy().to_string();
        }
    }
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join("eternum-modpack");
        if candidate.is_dir() {
            return candidate.to_string_lossy().to_string();
        }
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}

#[cfg_attr(feature = "desktop", tauri::command)]
async fn pick_folder(initial_dir: String) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dlg = New-Object System.Windows.Forms.FolderBrowserDialog
$dlg.Description = "Выберите папку сборки (где лежат mods/ и repo_config.json)"
$dlg.ShowNewFolderButton = $false
if ($env:ETERNUM_INIT_DIR -and (Test-Path $env:ETERNUM_INIT_DIR)) {
    $dlg.SelectedPath = $env:ETERNUM_INIT_DIR
}
if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    Write-Output $dlg.SelectedPath
}
"#;

        let output = tokio::task::spawn_blocking(move || {
            let mut cmd = std::process::Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-STA", "-Command", script])
                .env("ETERNUM_INIT_DIR", initial_dir)
                .creation_flags(CREATE_NO_WINDOW);
            cmd.output()
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Не удалось открыть окно выбора папки: {}", e))?;

        let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if selected.is_empty() {
            Ok(None)
        } else {
            Ok(Some(selected))
        }
    }

    #[cfg(target_os = "linux")]
    {
        let mut init = initial_dir.trim().to_string();
        if !init.is_empty() && !init.ends_with('/') {
            init.push('/');
        }

        let selected = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
            let mut zenity = std::process::Command::new("zenity");
            zenity.args([
                "--file-selection",
                "--directory",
                "--title=Выберите папку сборки (mods/ и repo_config.json)",
            ]);
            if !init.is_empty() && Path::new(init.trim_end_matches('/')).exists() {
                zenity.arg(format!("--filename={}", init));
            }

            match zenity.output() {
                Ok(out) => {
                    if out.status.success() {
                        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        return Ok(if path.is_empty() { None } else { Some(path) });
                    }
                    // Exit code 1 or 5 means user cancelled / closed dialog
                    if matches!(out.status.code(), Some(1) | Some(5)) {
                        return Ok(None);
                    }
                    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    Err(format!("Ошибка zenity: {}", err))
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let mut kdialog = std::process::Command::new("kdialog");
                    kdialog.arg("--getexistingdirectory");
                    if !init.is_empty() && Path::new(init.trim_end_matches('/')).exists() {
                        kdialog.arg(init.trim_end_matches('/'));
                    } else {
                        kdialog.arg(".");
                    }
                    kdialog.args(["--title", "Выберите папку сборки (mods/ и repo_config.json)"]);

                    match kdialog.output() {
                        Ok(out) => {
                            if out.status.success() {
                                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                                Ok(if path.is_empty() { None } else { Some(path) })
                            } else {
                                Ok(None)
                            }
                        }
                        Err(_) => Err(
                            "Не найден zenity или kdialog. Установите zenity или укажите путь к папке вручную."
                                .into(),
                        ),
                    }
                }
                Err(e) => Err(format!("Не удалось запустить zenity: {}", e)),
            }
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(selected)
    }

    #[cfg(target_os = "macos")]
    {
        let selected = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
            let script = if !initial_dir.trim().is_empty() && Path::new(initial_dir.trim()).exists() {
                format!(
                    "POSIX path of (choose folder with prompt \"Выберите папку сборки\" default location POSIX file \"{}\")",
                    initial_dir.trim().replace('"', "\\\"")
                )
            } else {
                "POSIX path of (choose folder with prompt \"Выберите папку сборки\")".to_string()
            };
            let out = std::process::Command::new("osascript")
                .args(["-e", &script])
                .output()
                .map_err(|e| format!("Не удалось открыть Finder: {}", e))?;
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                Ok(if path.is_empty() { None } else { Some(path) })
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| e.to_string())??;

        Ok(selected)
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = initial_dir;
        Err("Выбор папки через проводник не поддерживается на этой ОС.".into())
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn open_folder(dir: String) -> Result<(), String> {
    let path = PathBuf::from(dir);
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    open::that(&path).map_err(|e| e.to_string())
}

fn get_available_disk_space(path: &Path) -> Option<u64> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            break;
        }
    }
    if !probe.exists() {
        probe = PathBuf::from(".");
    }
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let c_path = CString::new(probe.as_os_str().as_bytes()).ok()?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
        if rc == 0 {
            return Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64));
        }
        None
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }
        let wide: Vec<u16> = probe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free_available: u64 = 0;
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok != 0 {
            return Some(free_available);
        }
        None
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

fn format_bytes_ru(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{} Б", bytes)
    } else if b < MB {
        format!("{:.1} КБ", b / KB)
    } else if b < GB {
        format!("{:.1} МБ", b / MB)
    } else {
        format!("{:.2} ГБ", b / GB)
    }
}

fn compute_local_pack_size(base_dir: &Path) -> Result<u64, String> {
    let mut total: u64 = 0;
    let mods_dir = base_dir.join("mods");
    if mods_dir.is_dir() {
        for entry in fs::read_dir(&mods_dir).map_err(|e| e.to_string())?.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
            {
                total += fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    for folder in EXTRA_DIRS {
        let mut files = Vec::new();
        collect_files_recursive(&base_dir.join(folder), &mut files)?;
        for f in files {
            total += fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
        }
    }
    for root_file in EXTRA_ROOT_FILES {
        let p = base_dir.join(root_file);
        if p.is_file() {
            total += fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        }
    }
    let bundles_dir = base_dir.join("bundles");
    if bundles_dir.is_dir() {
        for entry in fs::read_dir(&bundles_dir).map_err(|e| e.to_string())?.flatten() {
            let p = entry.path();
            if p.is_file()
                && p.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
            {
                total += fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    Ok(total)
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn inspect_modpack_dir(dir: String) -> Result<ModpackOverview, String> {
    let base_dir = PathBuf::from(dir.trim());
    if !base_dir.is_dir() {
        return Err(format!("Папка не найдена: {}", base_dir.display()));
    }

    let config_path = base_dir.join(CONFIG_FILE);
    let has_config = config_path.exists();
    let config = if has_config {
        let raw = fs::read_to_string(&config_path)
            .map_err(|e| format!("Не удалось прочитать {}: {}", config_path.display(), e))?;
        serde_json::from_str::<RepoConfig>(&raw)
            .map_err(|e| format!("Ошибка в {}: {}", config_path.display(), e))?
    } else {
        RepoConfig::default()
    };

    let mods_dir = base_dir.join("mods");
    let mods_count = if mods_dir.is_dir() {
        fs::read_dir(&mods_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|entry| entry.path()))
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
            })
            .count()
    } else {
        0
    };

    let mut extra_count = 0usize;
    for folder in EXTRA_DIRS {
        let mut files = Vec::new();
        collect_files_recursive(&base_dir.join(folder), &mut files)?;
        extra_count += files.len();
    }
    for root_file in EXTRA_ROOT_FILES {
        if base_dir.join(root_file).is_file() {
            extra_count += 1;
        }
    }

    let bundles_dir = base_dir.join("bundles");
    let bundles_count = if bundles_dir.is_dir() {
        fs::read_dir(&bundles_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|entry| entry.path()))
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
            })
            .count()
    } else {
        0
    };

    let local_size_bytes = compute_local_pack_size(&base_dir)?;
    let free_disk_bytes = get_available_disk_space(&base_dir);

    Ok(ModpackOverview {
        dir: base_dir.to_string_lossy().to_string(),
        has_config,
        config,
        mods_count,
        extra_count,
        bundles_count,
        local_size_bytes,
        free_disk_bytes,
    })
}

#[cfg_attr(feature = "desktop", tauri::command)]
async fn fetch_loader_versions(loader_type: String, mc_version: String) -> Result<Vec<String>, String> {
    let mc = mc_version.trim();
    if mc.is_empty() {
        return Err("Сначала укажите версию Minecraft.".into());
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    match loader_type.trim().to_lowercase().as_str() {
        "fabric" => {
            let url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}",
                urlencoding::encode(mc)
            );
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Не удалось запросить версии Fabric: {}", e))?;
            if !resp.status().is_success() {
                return Err(format!("Fabric API вернул статус {}", resp.status()));
            }
            let list: Value = resp.json().await.map_err(|e| e.to_string())?;
            let arr = list
                .as_array()
                .ok_or_else(|| "Неожиданный ответ Fabric API".to_string())?;

            let mut stable = Vec::new();
            let mut unstable = Vec::new();
            for item in arr {
                if let Some(ver) = item["loader"]["version"].as_str() {
                    if item["loader"]["stable"].as_bool().unwrap_or(false) {
                        stable.push(ver.to_string());
                    } else {
                        unstable.push(ver.to_string());
                    }
                }
            }
            stable.extend(unstable);
            stable.truncate(40);
            if stable.is_empty() {
                return Err(format!("Для Minecraft {} не найдены версии Fabric.", mc));
            }
            Ok(stable)
        }
        "neoforge" => {
            let url = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
            let resp = client
                .get(url)
                .send()
                .await
                .map_err(|e| format!("Не удалось запросить версии NeoForge: {}", e))?;
            if !resp.status().is_success() {
                return Err(format!("NeoForge API вернул статус {}", resp.status()));
            }
            let json: Value = resp.json().await.map_err(|e| e.to_string())?;
            let versions = json["versions"]
                .as_array()
                .ok_or_else(|| "Неожиданный ответ NeoForge API".to_string())?;

            let prefix = neoforge_prefix_for_mc(mc);
            let mut filtered: Vec<String> = versions
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|v| {
                    if let Some(ref p) = prefix {
                        v.starts_with(p)
                    } else {
                        true
                    }
                })
                .map(|s| s.to_string())
                .collect();

            filtered.reverse();
            let mut release_first: Vec<String> = filtered
                .iter()
                .filter(|v| !v.contains("-beta"))
                .cloned()
                .collect();
            let betas: Vec<String> = filtered
                .into_iter()
                .filter(|v| v.contains("-beta"))
                .collect();
            release_first.extend(betas);
            release_first.truncate(40);

            if release_first.is_empty() {
                return Err(format!("Для Minecraft {} не найдены версии NeoForge.", mc));
            }
            Ok(release_first)
        }
        other => Err(format!(
            "Автоподбор версий доступен только для Fabric и NeoForge (сейчас выбрано: {}). Введите версию вручную.",
            other
        )),
    }
}

async fn cmd_generate_manifest(
    app: UiHandle,
    dir: String,
    mut config: RepoConfig,
) -> Result<GenerateSummary, String> {
    let base_dir = PathBuf::from(dir.trim());
    if !base_dir.is_dir() {
        return Err(format!("Папка сборки не найдена: {}", base_dir.display()));
    }

    config.loader_type = config.loader_type.trim().to_lowercase();
    config.minecraft_version = config.minecraft_version.trim().to_string();
    config.loader_version = config.loader_version.trim().to_string();
    config.pack_version = config.pack_version.trim().to_string();
    config.server_name = config.server_name.trim().to_string();
    config.server_ip = config.server_ip.trim().to_string();
    config.repo_owner = config.repo_owner.trim().to_string();
    config.repo_name = config.repo_name.trim().to_string();
    config.branch = config.branch.trim().to_string();

    if config.minecraft_version.is_empty() || config.loader_type.is_empty() || config.loader_version.is_empty() {
        return Err("Заполните ядро, версию ядра и версию Minecraft.".into());
    }

    let config_path = base_dir.join(CONFIG_FILE);
    // Сохраняем bundles_extract_map из существующего файла, если в форме он не передался
    if config.bundles_extract_map.is_empty() && config_path.exists() {
        if let Ok(raw) = fs::read_to_string(&config_path) {
            if let Ok(existing) = serde_json::from_str::<RepoConfig>(&raw) {
                config.bundles_extract_map = existing.bundles_extract_map;
            }
        }
    }

    let config_pretty = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&config_path, format!("{}\n", config_pretty))
        .map_err(|e| format!("Не удалось сохранить {}: {}", config_path.display(), e))?;

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 10,
            status: "Сканирование папки mods/...".into(),
            log: Some(format!(
                "Сохранен repo_config.json ({} {} | MC {} | сборка {})",
                config.loader_type, config.loader_version, config.minecraft_version, config.pack_version
            )),
        },
    );

    let mods_dir = base_dir.join("mods");
    let config_dir = base_dir.join("config");
    fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;

    let mut mod_paths: Vec<PathBuf> = fs::read_dir(&mods_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        })
        .collect();
    mod_paths.sort();

    let total_mods = mod_paths.len();
    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 25,
            status: format!("Подсчет хешей модов ({} шт.)...", total_mods),
            log: Some(format!("Найдено .jar модов в mods/: {}", total_mods)),
        },
    );

    let mut mods = Vec::with_capacity(total_mods);
    let mut sha512_list = Vec::with_capacity(total_mods);
    for mod_path in mod_paths {
        let filename = mod_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("Некорректное имя файла: {}", mod_path.display()))?
            .to_string();
        let (sha256, sha512, size) = compute_hashes(&mod_path)?;
        sha512_list.push(sha512.clone());
        mods.push(ModEntry {
            rel_path: format!("mods/{}", filename),
            filename,
            sha256,
            sha512,
            size,
        });
    }

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 45,
            status: "Проверка модов в Modrinth API...".into(),
            log: Some("Запрос ссылок CDN к Modrinth API...".into()),
        },
    );

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let mut modrinth_results: HashMap<String, Value> = HashMap::new();
    for chunk in sha512_list.chunks(100) {
        let payload = json!({
            "hashes": chunk,
            "algorithm": "sha512"
        });
        match client.post(MODRINTH_API).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(map) = resp.json::<HashMap<String, Value>>().await {
                    modrinth_results.extend(map);
                }
            }
            Ok(resp) => {
                let _ = app.emit(
                    "gen-progress",
                    GenProgress {
                        percent: 55,
                        status: "Modrinth API недоступен, используем GitHub Raw...".into(),
                        log: Some(format!("⚠ Modrinth API вернул статус {}", resp.status())),
                    },
                );
            }
            Err(err) => {
                let _ = app.emit(
                    "gen-progress",
                    GenProgress {
                        percent: 55,
                        status: "Ошибка запроса к Modrinth API...".into(),
                        log: Some(format!("⚠ Ошибка Modrinth API: {}", err)),
                    },
                );
            }
        }
    }

    let mut files_list = Vec::new();
    let mut modrinth_count = 0usize;
    let mut github_count = 0usize;

    for item in mods {
        let mut modrinth_url: Option<String> = None;
        if let Some(mod_info) = modrinth_results.get(&item.sha512) {
            if let Some(files) = mod_info["files"].as_array() {
                for entry in files {
                    if entry["hashes"]["sha512"].as_str() == Some(item.sha512.as_str()) {
                        if let Some(url) = entry["url"].as_str() {
                            modrinth_url = Some(url.to_string());
                            break;
                        }
                    }
                }
            }
        }

        if let Some(url) = modrinth_url {
            let _ = app.emit(
                "gen-progress",
                GenProgress {
                    percent: 65,
                    status: "Обработка модов...".into(),
                    log: Some(format!("✓ [Modrinth CDN] {}", item.filename)),
                },
            );
            files_list.push(ManifestFile {
                path: item.rel_path,
                source: "modrinth".into(),
                url,
                sha256: item.sha256,
                size: item.size,
                extract_to: None,
            });
            modrinth_count += 1;
        } else {
            let _ = app.emit(
                "gen-progress",
                GenProgress {
                    percent: 65,
                    status: "Обработка модов...".into(),
                    log: Some(format!("• [GitHub Raw] {}", item.filename)),
                },
            );
            let url = make_github_raw_url(&config, &item.rel_path);
            files_list.push(ManifestFile {
                path: item.rel_path,
                source: "github".into(),
                url,
                sha256: item.sha256,
                size: item.size,
                extract_to: None,
            });
            github_count += 1;
        }
    }

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 80,
            status: "Сканирование конфигов, шейдеров, ресурспаков...".into(),
            log: None,
        },
    );

    let mut extra_count = 0usize;
    for folder_name in EXTRA_DIRS {
        let folder_dir = base_dir.join(folder_name);
        fs::create_dir_all(&folder_dir).map_err(|e| e.to_string())?;
        let mut extra_files = Vec::new();
        collect_files_recursive(&folder_dir, &mut extra_files)?;
        for file_path in extra_files {
            let rel = file_path
                .strip_prefix(&base_dir)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let (sha256, _, size) = compute_hashes(&file_path)?;
            files_list.push(ManifestFile {
                url: make_github_raw_url(&config, &rel),
                path: rel,
                source: "github".into(),
                sha256,
                size,
                extract_to: None,
            });
            extra_count += 1;
        }
    }

    for root_file in EXTRA_ROOT_FILES {
        let file_path = base_dir.join(root_file);
        if file_path.is_file() {
            let (sha256, _, size) = compute_hashes(&file_path)?;
            files_list.push(ManifestFile {
                url: make_github_raw_url(&config, root_file),
                path: (*root_file).to_string(),
                source: "github".into(),
                sha256,
                size,
                extract_to: None,
            });
            extra_count += 1;
        }
    }

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 90,
            status: "Сканирование zip-бандлов (bundles/)...".into(),
            log: None,
        },
    );

    let bundles_dir = base_dir.join("bundles");
    fs::create_dir_all(&bundles_dir).map_err(|e| e.to_string())?;
    let mut bundle_paths: Vec<PathBuf> = fs::read_dir(&bundles_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|entry| entry.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        })
        .collect();
    bundle_paths.sort();

    let mut bundle_count = 0usize;
    for bundle_path in bundle_paths {
        let filename = bundle_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("Некорректное имя бандла: {}", bundle_path.display()))?
            .to_string();
        let stem = bundle_path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let rel_path = format!("bundles/{}", filename);
        let (sha256, _, size) = compute_hashes(&bundle_path)?;
        let extract_to = config
            .bundles_extract_map
            .get(&filename)
            .cloned()
            .unwrap_or_else(|| if stem == "root" { String::new() } else { stem });

        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: 94,
                status: "Обработка бандлов...".into(),
                log: Some(format!("📦 [Bundle -> /{}] {}", extract_to, filename)),
            },
        );

        files_list.push(ManifestFile {
            url: make_github_raw_url(&config, &rel_path),
            path: rel_path,
            source: "github".into(),
            sha256,
            size,
            extract_to: Some(extract_to),
        });
        bundle_count += 1;
    }

    let total_files = files_list.len();
    let manifest = Manifest {
        schema_version: 1,
        pack_version: config.pack_version.clone(),
        server: ServerConfig {
            name: config.server_name.clone(),
            ip: config.server_ip.clone(),
        },
        minecraft: MinecraftConfig {
            version: config.minecraft_version.clone(),
        },
        loader: LoaderConfig {
            loader_type: config.loader_type.clone(),
            version: config.loader_version.clone(),
            installer_url: build_installer_url(
                &config.loader_type,
                &config.minecraft_version,
                &config.loader_version,
            ),
        },
        elyby: ElybyConfig {
            client_id: "astralforge1".into(),
        },
        files: files_list,
    };

    let manifest_path = base_dir.join(MANIFEST_FILE);
    let manifest_pretty = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(&manifest_path, manifest_pretty)
        .map_err(|e| format!("Не удалось записать {}: {}", manifest_path.display(), e))?;

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 100,
            status: "Готово! manifest.json успешно сгенерирован.".into(),
            log: Some(format!(
                "🎉 Сохранено в {} (всего записей: {})",
                manifest_path.display(),
                total_files
            )),
        },
    );

    Ok(GenerateSummary {
        manifest_path: manifest_path.to_string_lossy().to_string(),
        modrinth_count,
        github_count,
        extra_count,
        bundle_count,
        total_files,
    })
}

fn token_file_path(provider: &str) -> Option<PathBuf> {
    let filename = if provider.eq_ignore_ascii_case("gitlab") {
        "gitlab_token.txt"
    } else {
        "github_token.txt"
    };
    dirs::config_dir().map(|d| d.join("EternumManifestGen").join(filename))
}

fn load_token_for(provider: &str) -> String {
    let Some(path) = token_file_path(provider) else {
        return String::new();
    };
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn save_token_for(provider: &str, token: &str) -> Result<(), String> {
    let Some(path) = token_file_path(provider) else {
        return Err("Не удалось определить папку настроек пользователя.".into());
    };
    let trimmed = token.trim();
    if trimmed.is_empty() {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, trimmed).map_err(|e| format!("Не удалось сохранить токен: {}", e))
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn load_github_token() -> String {
    load_token_for("github")
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn save_github_token(token: String) -> Result<(), String> {
    save_token_for("github", &token)
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn load_gitlab_token() -> String {
    load_token_for("gitlab")
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn save_gitlab_token(token: String) -> Result<(), String> {
    save_token_for("gitlab", &token)
}

fn github_client_optional(token: &str) -> Result<reqwest::Client, String> {
    use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
    let mut headers = HeaderMap::new();
    let trimmed = token.trim();
    if !trimmed.is_empty() {
        let auth_val = HeaderValue::from_str(&format!("Bearer {}", trimmed))
            .map_err(|_| "Некорректные символы в GitHub токене.".to_string())?;
        headers.insert(AUTHORIZATION, auth_val);
    }
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        HeaderValue::from_static("2022-11-28"),
    );

    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .map_err(|e| e.to_string())
}

fn gitlab_project_id(owner: &str, repo: &str) -> String {
    urlencoding::encode(&format!("{}/{}", owner.trim(), repo.trim())).into_owned()
}

fn gitlab_client_optional(token: &str) -> Result<reqwest::Client, String> {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    let trimmed = token.trim();
    if !trimmed.is_empty() {
        let auth_val = HeaderValue::from_str(trimmed)
            .map_err(|_| "Некорректные символы в GitLab токене.".to_string())?;
        headers.insert("PRIVATE-TOKEN", auth_val);
    }
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .map_err(|e| e.to_string())
}

fn gitlab_client(token: &str) -> Result<reqwest::Client, String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("Введите GitLab токен (Personal/Project Access Token).".into());
    }
    gitlab_client_optional(trimmed)
}

fn map_gitlab_http_error(
    status: reqwest::StatusCode,
    body: &str,
    owner: &str,
    repo: &str,
) -> String {
    match status.as_u16() {
        401 => "Неверный или истёкший GitLab токен (401 Unauthorized).".into(),
        403 => format!(
            "Нет прав на запись в GitLab {}/{} (403 Forbidden). Проверьте роль (Developer/Maintainer) и скопы токена (api или read_repository + write_repository).",
            owner, repo
        ),
        404 => format!(
            "Проект GitLab {}/{} или ветка не найдены, либо у токена нет к нему доступа (404).",
            owner, repo
        ),
        _ => format!("Ошибка GitLab API ({}): {}", status, body),
    }
}

async fn fetch_gitlab_head_info(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<RemoteHeadInfo, String> {
    let pid = gitlab_project_id(owner, repo);
    let url = format!(
        "https://gitlab.com/api/v4/projects/{}/repository/commits/{}",
        pid,
        urlencoding::encode(branch.trim())
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка соединения с GitLab: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_gitlab_http_error(status, &body, owner, repo));
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let commit_sha = json["id"]
        .as_str()
        .ok_or_else(|| "GitLab не вернул SHA последнего коммита.".to_string())?
        .to_string();
    let message = json["title"]
        .as_str()
        .or_else(|| json["message"].as_str())
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let author = json["author_name"].as_str().unwrap_or("").to_string();
    let date = json["committed_date"]
        .as_str()
        .or_else(|| json["authored_date"].as_str())
        .unwrap_or("")
        .to_string();

    Ok(RemoteHeadInfo {
        commit_sha: commit_sha.clone(),
        tree_sha: commit_sha,
        message,
        author,
        date,
    })
}

async fn fetch_gitlab_tree_items(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    ref_sha: &str,
) -> Result<Vec<(String, String, String)>, String> {
    let pid = gitlab_project_id(owner, repo);
    let mut out = Vec::new();
    let mut page = 1u32;

    loop {
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/repository/tree?ref={}&recursive=true&per_page=100&page={}",
            pid,
            urlencoding::encode(ref_sha),
            page
        );
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Ошибка получения дерева файлов GitLab: {}", e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(map_gitlab_http_error(status, &body, owner, repo));
        }

        let next_page = resp
            .headers()
            .get("x-next-page")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let items: Vec<Value> = resp.json().await.map_err(|e| e.to_string())?;
        if items.is_empty() {
            break;
        }
        for item in items {
            if item["type"].as_str() != Some("blob") {
                continue;
            }
            let Some(path) = item["path"].as_str() else {
                continue;
            };
            let Some(sha) = item["id"].as_str() else {
                continue;
            };
            let mode = item["mode"].as_str().unwrap_or("100644");
            out.push((path.to_string(), sha.to_string(), mode.to_string()));
        }

        if next_page.is_empty() {
            break;
        }
        page = next_page.parse::<u32>().unwrap_or(page + 1);
    }

    Ok(out)
}

fn make_gitlab_raw_api_url(owner: &str, repo: &str, ref_sha: &str, rel_path: &str) -> String {
    let pid = gitlab_project_id(owner, repo);
    let encoded_path = urlencoding::encode(rel_path);
    format!(
        "https://gitlab.com/api/v4/projects/{}/repository/files/{}/raw?ref={}",
        pid,
        encoded_path,
        urlencoding::encode(ref_sha)
    )
}

async fn estimate_gitlab_pack_sizes(
    client: &reqwest::Client,
    base_dir: &Path,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    tree_items: &[(String, String, String)],
) -> Option<(u64, u64)> {
    let has_manifest = tree_items.iter().any(|(p, _, _)| p == MANIFEST_FILE);
    if !has_manifest {
        return None;
    }

    let manifest_url = make_gitlab_raw_api_url(owner, repo, commit_sha, MANIFEST_FILE);
    let resp = client.get(&manifest_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let manifest_val = resp.json::<Value>().await.ok()?;
    let files = manifest_val["files"].as_array()?;

    let mut remote_sha_map: HashMap<&str, &str> = HashMap::new();
    for (p, sha, _) in tree_items {
        remote_sha_map.insert(p.as_str(), sha.as_str());
    }

    let mut total_pack_bytes: u64 = 0;
    let mut needed_bytes: u64 = 0;

    for entry in files {
        let Some(rel_path) = entry["path"].as_str() else {
            continue;
        };
        let size = entry["size"].as_u64().unwrap_or(0);
        total_pack_bytes = total_pack_bytes.saturating_add(size);

        let local_path = base_dir.join(rel_path);
        let is_cached = if entry["source"].as_str() == Some("modrinth") {
            let expected_sha256 = entry["sha256"].as_str().unwrap_or("");
            if local_path.is_file() && !expected_sha256.is_empty() {
                if fs::metadata(&local_path).map(|m| m.len()).unwrap_or(u64::MAX) == size {
                    compute_hashes(&local_path)
                        .map(|(sha256, _, _)| sha256.eq_ignore_ascii_case(expected_sha256))
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                false
            }
        } else if let Some(remote_sha) = remote_sha_map.get(rel_path) {
            if local_path.is_file() {
                if fs::metadata(&local_path).map(|m| m.len()).unwrap_or(u64::MAX) == size {
                    fs::read(&local_path)
                        .map(|bytes| &compute_git_blob_sha1(&bytes) == remote_sha)
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if !is_cached {
            needed_bytes = needed_bytes.saturating_add(size);
        }
    }

    Some((total_pack_bytes, needed_bytes))
}

fn read_sync_state(base_dir: &Path) -> Option<SyncState> {
    let path = base_dir.join(SYNC_STATE_FILE);
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<SyncState>(&raw).ok()
}

fn write_sync_state(base_dir: &Path, state: &SyncState) -> Result<(), String> {
    let path = base_dir.join(SYNC_STATE_FILE);
    let pretty = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, format!("{}\n", pretty))
        .map_err(|e| format!("Не удалось сохранить состояние синхронизации: {}", e))
}

struct RemoteHeadInfo {
    commit_sha: String,
    tree_sha: String,
    message: String,
    author: String,
    date: String,
}

async fn fetch_remote_head_info(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<RemoteHeadInfo, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/commits/{}",
        owner.trim(),
        repo.trim(),
        urlencoding::encode(branch.trim())
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка соединения с GitHub: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_github_http_error(status, &body, owner, repo));
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let commit_sha = json["sha"]
        .as_str()
        .ok_or_else(|| "GitHub не вернул SHA последнего коммита.".to_string())?
        .to_string();
    let tree_sha = json["commit"]["tree"]["sha"]
        .as_str()
        .ok_or_else(|| "GitHub не вернул tree SHA коммита.".to_string())?
        .to_string();
    let message = json["commit"]["message"]
        .as_str()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let author = json["commit"]["author"]["name"]
        .as_str()
        .or_else(|| json["author"]["login"].as_str())
        .unwrap_or("")
        .to_string();
    let date = json["commit"]["author"]["date"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(RemoteHeadInfo {
        commit_sha,
        tree_sha,
        message,
        author,
        date,
    })
}

fn compute_local_manifest_total_size(base_dir: &Path) -> Option<u64> {
    let manifest_path = base_dir.join(MANIFEST_FILE);
    if manifest_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&manifest_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(files) = val["files"].as_array() {
                    let sum: u64 = files
                        .iter()
                        .filter_map(|e| e["size"].as_u64())
                        .sum();
                    if sum > 0 {
                        return Some(sum);
                    }
                }
            }
        }
    }
    compute_local_pack_size(base_dir).ok()
}

async fn estimate_remote_pack_sizes(
    client: &reqwest::Client,
    base_dir: &Path,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    tree_sha: &str,
    existing_tree: Option<&Value>,
) -> Option<(u64, u64)> {
    let owned_tree;
    let tree_json = match existing_tree {
        Some(t) => t,
        None => {
            let tree_url = format!(
                "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
                owner, repo, tree_sha
            );
            let resp = client.get(&tree_url).send().await.ok()?;
            if !resp.status().is_success() {
                return None;
            }
            owned_tree = resp.json::<Value>().await.ok()?;
            &owned_tree
        }
    };

    let items = tree_json["tree"].as_array()?;
    let mut total_pack_bytes: u64 = 0;
    let mut needed_bytes: u64 = 0;
    let mut has_manifest = false;

    for item in items {
        if item["type"].as_str() != Some("blob") {
            continue;
        }
        let Some(path) = item["path"].as_str() else {
            continue;
        };
        if path == MANIFEST_FILE {
            has_manifest = true;
        }
        if !is_managed_pack_path(path) {
            continue;
        }
        let size = item["size"].as_u64().unwrap_or(0);
        let remote_sha = item["sha"].as_str().unwrap_or("");
        total_pack_bytes = total_pack_bytes.saturating_add(size);

        let local_path = base_dir.join(path);
        let is_cached = if local_path.is_file() {
            if fs::metadata(&local_path).map(|m| m.len()).unwrap_or(u64::MAX) == size {
                fs::read(&local_path)
                    .map(|bytes| compute_git_blob_sha1(&bytes) == remote_sha)
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        if !is_cached {
            needed_bytes = needed_bytes.saturating_add(size);
        }
    }

    if has_manifest {
        let manifest_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            owner, repo, commit_sha, MANIFEST_FILE
        );
        if let Ok(resp) = client.get(&manifest_url).send().await {
            if resp.status().is_success() {
                if let Ok(manifest_val) = resp.json::<Value>().await {
                    if let Some(files) = manifest_val["files"].as_array() {
                        for entry in files {
                            if entry["source"].as_str() != Some("modrinth") {
                                continue;
                            }
                            let Some(rel_path) = entry["path"].as_str() else {
                                continue;
                            };
                            let size = entry["size"].as_u64().unwrap_or(0);
                            let expected_sha256 = entry["sha256"].as_str().unwrap_or("");
                            total_pack_bytes = total_pack_bytes.saturating_add(size);

                            let local_path = base_dir.join(rel_path);
                            let is_cached = if local_path.is_file() && !expected_sha256.is_empty() {
                                if fs::metadata(&local_path).map(|m| m.len()).unwrap_or(u64::MAX) == size {
                                    compute_hashes(&local_path)
                                        .map(|(sha256, _, _)| sha256.eq_ignore_ascii_case(expected_sha256))
                                        .unwrap_or(false)
                                } else {
                                    false
                                }
                            } else {
                                false
                            };
                            if !is_cached {
                                needed_bytes = needed_bytes.saturating_add(size);
                            }
                        }
                    }
                }
            }
        }
    }

    Some((total_pack_bytes, needed_bytes))
}

#[cfg_attr(feature = "desktop", tauri::command)]
async fn check_remote_sync(
    dir: String,
    owner: String,
    repo: String,
    branch: String,
    token: String,
    provider: Option<String>,
) -> Result<SyncCheckResult, String> {
    let prov = normalize_provider(provider.as_deref());
    let label = provider_label(prov);
    let base_dir = PathBuf::from(dir.trim());
    let free_disk_bytes = get_available_disk_space(&base_dir);
    let owner = owner.trim();
    let repo = repo.trim();
    let branch = branch.trim();
    if owner.is_empty() || repo.is_empty() || branch.is_empty() {
        return Ok(SyncCheckResult {
            status: "uninitialized".into(),
            local_sha: None,
            remote_sha: None,
            remote_message: None,
            remote_author: None,
            remote_date: None,
            message: format!("Укажите владельца, репозиторий и ветку {}.", label),
            total_pack_bytes: compute_local_manifest_total_size(&base_dir),
            needed_bytes: None,
            free_disk_bytes,
            enough_disk_space: None,
        });
    }

    let client = if prov == "gitlab" {
        gitlab_client_optional(&token)?
    } else {
        github_client_optional(&token)?
    };
    let head = match if prov == "gitlab" {
        fetch_gitlab_head_info(&client, owner, repo, branch).await
    } else {
        fetch_remote_head_info(&client, owner, repo, branch).await
    } {
        Ok(h) => h,
        Err(err) => {
            return Ok(SyncCheckResult {
                status: "error".into(),
                local_sha: read_sync_state(&base_dir).map(|s| s.commit_sha),
                remote_sha: None,
                remote_message: None,
                remote_author: None,
                remote_date: None,
                message: err,
                total_pack_bytes: compute_local_manifest_total_size(&base_dir),
                needed_bytes: None,
                free_disk_bytes,
                enough_disk_space: None,
            });
        }
    };

    if let Some(local_state) = read_sync_state(&base_dir) {
        let same_target = local_state.provider.eq_ignore_ascii_case(prov)
            && local_state.repo_owner.eq_ignore_ascii_case(owner)
            && local_state.repo_name.eq_ignore_ascii_case(repo)
            && local_state.branch == branch;

        if same_target && local_state.commit_sha == head.commit_sha {
            return Ok(SyncCheckResult {
                status: "up_to_date".into(),
                local_sha: Some(local_state.commit_sha),
                remote_sha: Some(head.commit_sha),
                remote_message: Some(head.message),
                remote_author: Some(head.author),
                remote_date: Some(head.date),
                message: format!("Сборка актуальна и синхронизирована с {}.", label),
                total_pack_bytes: compute_local_manifest_total_size(&base_dir),
                needed_bytes: Some(0),
                free_disk_bytes,
                enough_disk_space: free_disk_bytes.map(|_| true),
            });
        }

        if same_target {
            let sizes = if prov == "gitlab" {
                match fetch_gitlab_tree_items(&client, owner, repo, &head.commit_sha).await {
                    Ok(items) => {
                        estimate_gitlab_pack_sizes(
                            &client,
                            &base_dir,
                            owner,
                            repo,
                            &head.commit_sha,
                            &items,
                        )
                        .await
                    }
                    Err(_) => None,
                }
            } else {
                estimate_remote_pack_sizes(
                    &client,
                    &base_dir,
                    owner,
                    repo,
                    &head.commit_sha,
                    &head.tree_sha,
                    None,
                )
                .await
            };
            let total_pack_bytes = sizes.map(|(t, _)| t);
            let needed_bytes = sizes.map(|(_, n)| n);
            let enough_disk_space = match (free_disk_bytes, needed_bytes) {
                (Some(free), Some(need)) => Some(free >= need),
                _ => None,
            };

            return Ok(SyncCheckResult {
                status: "update_required".into(),
                local_sha: Some(local_state.commit_sha),
                remote_sha: Some(head.commit_sha),
                remote_message: Some(head.message),
                remote_author: Some(head.author),
                remote_date: Some(head.date),
                message: format!(
                    "На {} обнаружено обновление сборки! Скачайте его перед продолжением работы.",
                    label
                ),
                total_pack_bytes,
                needed_bytes,
                free_disk_bytes,
                enough_disk_space,
            });
        }
    }

    // Если .eternum-sync.json ещё нет или сменилась платформа, проверяем manifest.json в удалённом дереве.
    let (tree_json_opt, gitlab_tree_opt) = if prov == "gitlab" {
        let items = fetch_gitlab_tree_items(&client, owner, repo, &head.commit_sha)
            .await
            .ok();
        (None, items)
    } else {
        let tree_url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            owner, repo, head.tree_sha
        );
        let tree_resp = client.get(&tree_url).send().await;
        let tree_json_opt: Option<Value> = match tree_resp {
            Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
            _ => None,
        };
        (tree_json_opt, None)
    };

    let remote_manifest_sha = if prov == "gitlab" {
        gitlab_tree_opt.as_ref().and_then(|items| {
            items
                .iter()
                .find(|(p, _, _)| p == MANIFEST_FILE)
                .map(|(_, sha, _)| sha.clone())
        })
    } else {
        tree_json_opt.as_ref().and_then(|tree_json| {
            tree_json["tree"].as_array().and_then(|arr| {
                arr.iter()
                    .find(|item| item["path"].as_str() == Some(MANIFEST_FILE))
                    .and_then(|item| item["sha"].as_str())
                    .map(|s| s.to_string())
            })
        })
    };

    let Some(remote_m_sha) = remote_manifest_sha else {
        return Ok(SyncCheckResult {
            status: "uninitialized".into(),
            local_sha: None,
            remote_sha: Some(head.commit_sha),
            remote_message: Some(head.message),
            remote_author: Some(head.author),
            remote_date: Some(head.date),
            message: format!(
                "Репозиторий {} пока не содержит manifest.json — можно выполнить первую публикацию.",
                label
            ),
            total_pack_bytes: compute_local_manifest_total_size(&base_dir),
            needed_bytes: Some(0),
            free_disk_bytes,
            enough_disk_space: free_disk_bytes.map(|_| true),
        });
    };

    let local_manifest_path = base_dir.join(MANIFEST_FILE);
    if local_manifest_path.is_file() {
        if let Ok(bytes) = fs::read(&local_manifest_path) {
            if compute_git_blob_sha1(&bytes) == remote_m_sha {
                let pack_ver = fs::read_to_string(base_dir.join(CONFIG_FILE))
                    .ok()
                    .and_then(|s| serde_json::from_str::<RepoConfig>(&s).ok())
                    .map(|c| c.pack_version)
                    .unwrap_or_default();
                let _ = write_sync_state(
                    &base_dir,
                    &SyncState {
                        provider: prov.to_string(),
                        repo_owner: owner.to_string(),
                        repo_name: repo.to_string(),
                        branch: branch.to_string(),
                        commit_sha: head.commit_sha.clone(),
                        pack_version: pack_ver,
                    },
                );
                return Ok(SyncCheckResult {
                    status: "up_to_date".into(),
                    local_sha: Some(head.commit_sha.clone()),
                    remote_sha: Some(head.commit_sha),
                    remote_message: Some(head.message),
                    remote_author: Some(head.author),
                    remote_date: Some(head.date),
                    message: format!("Локальная сборка совпадает с версией на {}.", label),
                    total_pack_bytes: compute_local_manifest_total_size(&base_dir),
                    needed_bytes: Some(0),
                    free_disk_bytes,
                    enough_disk_space: free_disk_bytes.map(|_| true),
                });
            }
        }
    }

    let sizes = if prov == "gitlab" {
        match gitlab_tree_opt.as_ref() {
            Some(items) => {
                estimate_gitlab_pack_sizes(&client, &base_dir, owner, repo, &head.commit_sha, items)
                    .await
            }
            None => None,
        }
    } else {
        estimate_remote_pack_sizes(
            &client,
            &base_dir,
            owner,
            repo,
            &head.commit_sha,
            &head.tree_sha,
            tree_json_opt.as_ref(),
        )
        .await
    };
    let total_pack_bytes = sizes.map(|(t, _)| t);
    let needed_bytes = sizes.map(|(_, n)| n);
    let enough_disk_space = match (free_disk_bytes, needed_bytes) {
        (Some(free), Some(need)) => Some(free >= need),
        _ => None,
    };

    Ok(SyncCheckResult {
        status: "update_required".into(),
        local_sha: None,
        remote_sha: Some(head.commit_sha),
        remote_message: Some(head.message),
        remote_author: Some(head.author),
        remote_date: Some(head.date),
        message: format!(
            "На {} уже есть опубликованная сборка, но локальная папка не синхронизирована. Скачайте обновление перед продолжением.",
            label
        ),
        total_pack_bytes,
        needed_bytes,
        free_disk_bytes,
        enough_disk_space,
    })
}

async fn download_raw_bytes(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Ошибка загрузки {}: {}", url, e))?;
    if !resp.status().is_success() {
        return Err(format!("Ошибка HTTP {} при скачивании {}", resp.status(), url));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Ошибка чтения ответа {}: {}", url, e))
}

async fn download_raw_bytes_stream(
    app: &UiHandle,
    client: &reqwest::Client,
    url: &str,
    file_name: &str,
    known_size: u64,
    total_downloaded_so_far: u64,
    total_bytes: u64,
    smooth_speed_bps: &mut u64,
) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Ошибка загрузки {}: {}", url, e))?;
    if !resp.status().is_success() {
        return Err(format!("Ошибка HTTP {} при скачивании {}", resp.status(), url));
    }

    let file_total = resp.content_length().unwrap_or(known_size);
    let mut buf = Vec::with_capacity(file_total as usize);
    let mut stream = resp.bytes_stream();
    let mut file_downloaded: u64 = 0;

    let _ = app.emit(
        "download-stats",
        DownloadStats {
            file_name: Some(file_name.to_string()),
            file_downloaded: 0,
            file_total,
            total_downloaded: total_downloaded_so_far,
            total_bytes,
            speed_bps: *smooth_speed_bps,
        },
    );

    let mut window_start = Instant::now();
    let mut window_bytes: u64 = 0;
    let mut last_emit = Instant::now();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("Ошибка чтения ответа {}: {}", url, e))?;
        let len = chunk.len() as u64;
        buf.extend_from_slice(&chunk);
        file_downloaded += len;
        window_bytes += len;

        if last_emit.elapsed() >= Duration::from_millis(120) {
            let dt = window_start.elapsed().as_secs_f64().max(0.001);
            let inst_speed = (window_bytes as f64 / dt) as u64;
            *smooth_speed_bps = if *smooth_speed_bps == 0 {
                inst_speed
            } else {
                ((*smooth_speed_bps as f64 * 0.6) + (inst_speed as f64 * 0.4)) as u64
            };
            let _ = app.emit(
                "download-stats",
                DownloadStats {
                    file_name: Some(file_name.to_string()),
                    file_downloaded,
                    file_total: file_total.max(file_downloaded),
                    total_downloaded: total_downloaded_so_far + file_downloaded,
                    total_bytes: total_bytes.max(total_downloaded_so_far + file_downloaded),
                    speed_bps: *smooth_speed_bps,
                },
            );
            last_emit = Instant::now();
            if window_start.elapsed() >= Duration::from_millis(500) {
                window_bytes = 0;
                window_start = Instant::now();
            }
        }
    }

    let dt = window_start.elapsed().as_secs_f64().max(0.001);
    if window_bytes > 0 {
        let inst_speed = (window_bytes as f64 / dt) as u64;
        *smooth_speed_bps = if *smooth_speed_bps == 0 {
            inst_speed
        } else {
            ((*smooth_speed_bps as f64 * 0.6) + (inst_speed as f64 * 0.4)) as u64
        };
    }

    let _ = app.emit(
        "download-stats",
        DownloadStats {
            file_name: Some(file_name.to_string()),
            file_downloaded,
            file_total: file_total.max(file_downloaded),
            total_downloaded: total_downloaded_so_far + file_downloaded,
            total_bytes: total_bytes.max(total_downloaded_so_far + file_downloaded),
            speed_bps: *smooth_speed_bps,
        },
    );

    Ok(buf)
}

fn collect_managed_local_files(base_dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for root_file in EXTRA_ROOT_FILES {
        if base_dir.join(root_file).is_file() {
            out.push((*root_file).to_string());
        }
    }
    let mut dirs_to_scan: Vec<&str> = vec!["mods", "bundles"];
    dirs_to_scan.extend_from_slice(EXTRA_DIRS);

    for d in dirs_to_scan {
        let dir_path = base_dir.join(d);
        if !dir_path.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_files_recursive(&dir_path, &mut files)?;
        for f in files {
            if let Ok(rel) = f.strip_prefix(base_dir) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(out)
}

async fn cmd_download_modpack_from_github(
    app: UiHandle,
    dir: String,
    owner: String,
    repo: String,
    branch: String,
    token: String,
    provider: Option<String>,
) -> Result<DownloadSummary, String> {
    let prov = normalize_provider(provider.as_deref());
    let label = provider_label(prov);
    let base_dir = PathBuf::from(dir.trim());
    let owner = owner.trim();
    let repo = repo.trim();
    let branch = branch.trim();
    if owner.is_empty() || repo.is_empty() || branch.is_empty() {
        return Err(format!("Укажите владельца, репозиторий и ветку {}.", label));
    }

    fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Не удалось создать папку {}: {}", base_dir.display(), e))?;

    let gh_client = if prov == "gitlab" {
        gitlab_client_optional(&token)?
    } else {
        github_client_optional(&token)?
    };
    let cdn_client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 5,
            status: format!("Подключение к {} ({}/{})...", label, owner, repo),
            log: Some(format!(
                "☁ Получение данных сборки из {} {}/{} ({})",
                label, owner, repo, branch
            )),
        },
    );

    let head = if prov == "gitlab" {
        fetch_gitlab_head_info(&gh_client, owner, repo, branch).await?
    } else {
        fetch_remote_head_info(&gh_client, owner, repo, branch).await?
    };

    let mut github_tree_files: Vec<(String, String, u64)> = Vec::new(); // (rel_path, blob_sha, size)
    if prov == "gitlab" {
        let tree_items = fetch_gitlab_tree_items(&gh_client, owner, repo, &head.commit_sha).await?;
        for (path, sha, _) in tree_items {
            if is_managed_pack_path(&path) {
                github_tree_files.push((path, sha, 0));
            }
        }
    } else {
        let tree_url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            owner, repo, head.tree_sha
        );
        let tree_resp = gh_client
            .get(&tree_url)
            .send()
            .await
            .map_err(|e| format!("Ошибка получения дерева файлов: {}", e))?;
        if !tree_resp.status().is_success() {
            let status = tree_resp.status();
            let body = tree_resp.text().await.unwrap_or_default();
            return Err(map_github_http_error(status, &body, owner, repo));
        }
        let tree_json: Value = tree_resp.json().await.map_err(|e| e.to_string())?;

        if let Some(items) = tree_json["tree"].as_array() {
            for item in items {
                if item["type"].as_str() != Some("blob") {
                    continue;
                }
                let Some(path) = item["path"].as_str() else {
                    continue;
                };
                let Some(sha) = item["sha"].as_str() else {
                    continue;
                };
                let size = item["size"].as_u64().unwrap_or(0);
                if is_managed_pack_path(path) {
                    github_tree_files.push((path.to_string(), sha.to_string(), size));
                }
            }
        }
    }

    if github_tree_files.is_empty() {
        return Err("В указанном репозитории пока нет файлов сборки.".into());
    }

    let make_commit_raw_url = |rel_path: &str| -> String {
        if prov == "gitlab" {
            return make_gitlab_raw_api_url(owner, repo, &head.commit_sha, rel_path);
        }
        let encoded: String = rel_path
            .split('/')
            .map(|seg| urlencoding::encode(seg).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            owner, repo, head.commit_sha, encoded
        )
    };

    let mut downloaded_github = 0usize;
    let mut downloaded_modrinth = 0usize;
    let mut skipped_cached = 0usize;

    let mut expected_paths: HashSet<String> = HashSet::new();
    let mut gh_to_download: Vec<(String, u64)> = Vec::new();
    let mut total_pack_bytes: u64 = 0;

    for (rel_path, remote_blob_sha, size) in &github_tree_files {
        expected_paths.insert(rel_path.clone());
        total_pack_bytes = total_pack_bytes.saturating_add(*size);
        let local_path = base_dir.join(rel_path);
        let is_cached = if local_path.is_file() {
            fs::read(&local_path)
                .map(|bytes| &compute_git_blob_sha1(&bytes) == remote_blob_sha)
                .unwrap_or(false)
        } else {
            false
        };
        if is_cached {
            skipped_cached += 1;
        } else {
            gh_to_download.push((rel_path.clone(), *size));
        }
    }

    // Если manifest.json нужно скачать, скачиваем его первым, чтобы заранее знать размер всех Modrinth-модов и проверить место на диске
    let mut modrinth_entries: Vec<(String, String, String, u64)> = Vec::new();
    let mut pack_version = String::new();
    let mut manifest_val_opt: Option<Value> = None;
    let manifest_in_gh_download = gh_to_download.iter().any(|(p, _)| p == MANIFEST_FILE);
    if manifest_in_gh_download {
        let m_url = make_commit_raw_url(MANIFEST_FILE);
        let m_bytes = download_raw_bytes(&gh_client, &m_url).await?;
        let m_path = base_dir.join(MANIFEST_FILE);
        if let Some(parent) = m_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&m_path, &m_bytes)
            .map_err(|e| format!("Не удалось записать {}: {}", m_path.display(), e))?;
        downloaded_github += 1;
        gh_to_download.retain(|(p, _)| p != MANIFEST_FILE);
        if let Ok(raw) = String::from_utf8(m_bytes) {
            manifest_val_opt = serde_json::from_str::<Value>(&raw).ok();
        }
    } else if base_dir.join(MANIFEST_FILE).is_file() {
        if let Ok(raw) = fs::read_to_string(base_dir.join(MANIFEST_FILE)) {
            manifest_val_opt = serde_json::from_str::<Value>(&raw).ok();
        }
    }

    let mut manifest_size_map: HashMap<String, u64> = HashMap::new();
    if let Some(manifest_val) = manifest_val_opt {
        if let Some(pv) = manifest_val["packVersion"].as_str() {
            pack_version = pv.to_string();
        }
        if let Some(files) = manifest_val["files"].as_array() {
            for entry in files {
                let Some(rel_path) = entry["path"].as_str() else {
                    continue;
                };
                let size = entry["size"].as_u64().unwrap_or(0);
                manifest_size_map.insert(rel_path.to_string(), size);
                expected_paths.insert(rel_path.to_string());
                if entry["source"].as_str() == Some("modrinth") {
                    if let Some(url) = entry["url"].as_str() {
                        let sha256 = entry["sha256"].as_str().unwrap_or("").to_string();
                        total_pack_bytes = total_pack_bytes.saturating_add(size);
                        modrinth_entries.push((rel_path.to_string(), url.to_string(), sha256, size));
                    }
                }
            }
        }
    }

    for (rel_path, size) in &mut gh_to_download {
        if *size == 0 {
            if let Some(&mapped_size) = manifest_size_map.get(rel_path) {
                *size = mapped_size;
                total_pack_bytes = total_pack_bytes.saturating_add(mapped_size);
            }
        }
    }

    let mut mr_to_download: Vec<(String, String, String, u64)> = Vec::new();
    for (rel_path, url, expected_sha256, size) in modrinth_entries {
        let local_path = base_dir.join(&rel_path);
        let is_cached = if local_path.is_file() && !expected_sha256.is_empty() {
            compute_hashes(&local_path)
                .map(|(sha256, _, _)| sha256.eq_ignore_ascii_case(&expected_sha256))
                .unwrap_or(false)
        } else {
            false
        };
        if is_cached {
            skipped_cached += 1;
        } else {
            mr_to_download.push((rel_path, url, expected_sha256, size));
        }
    }

    let gh_download_bytes: u64 = gh_to_download.iter().map(|(_, s)| *s).sum();
    let mr_download_bytes: u64 = mr_to_download.iter().map(|(_, _, _, s)| *s).sum();
    let total_download_bytes = gh_download_bytes.saturating_add(mr_download_bytes);

    if let Some(free_bytes) = get_available_disk_space(&base_dir) {
        if free_bytes < total_download_bytes {
            return Err(format!(
                "Недостаточно свободного места на диске! Нужно: {}, доступно: {}.",
                format_bytes_ru(total_download_bytes),
                format_bytes_ru(free_bytes)
            ));
        }
        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: 8,
                status: format!(
                    "Подготовка загрузки ({} из {})...",
                    format_bytes_ru(total_download_bytes),
                    format_bytes_ru(total_pack_bytes)
                ),
                log: Some(format!(
                    "💾 Вес сборки: {}, нужно скачать: {}, свободно на диске: {}",
                    format_bytes_ru(total_pack_bytes),
                    format_bytes_ru(total_download_bytes),
                    format_bytes_ru(free_bytes)
                )),
            },
        );
    }

    let mut total_downloaded_bytes: u64 = 0;
    let mut smooth_speed_bps: u64 = 0;

    // 1. Скачиваем файлы из GitHub-дерева
    let total_gh = gh_to_download.len();
    for (idx, (rel_path, known_size)) in gh_to_download.iter().enumerate() {
        let local_path = base_dir.join(rel_path);
        let pct = if total_download_bytes > 0 {
            10 + (((total_downloaded_bytes as f64 / total_download_bytes as f64) * 80.0) as u32)
        } else {
            10 + ((idx as f32 / total_gh.max(1) as f32) * 35.0) as u32
        };
        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: pct.min(92),
                status: format!("Скачивание из {} ({}/{}): {}", label, idx + 1, total_gh, rel_path),
                log: Some(format!("⬇ {}: {} ({})", label, rel_path, format_bytes_ru(*known_size))),
            },
        );

        let url = make_commit_raw_url(rel_path);
        let bytes = download_raw_bytes_stream(
            &app,
            &gh_client,
            &url,
            rel_path,
            *known_size,
            total_downloaded_bytes,
            total_download_bytes,
            &mut smooth_speed_bps,
        )
        .await?;
        total_downloaded_bytes = total_downloaded_bytes.saturating_add(bytes.len() as u64);

        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&local_path, &bytes)
            .map_err(|e| format!("Не удалось записать {}: {}", local_path.display(), e))?;
        downloaded_github += 1;
    }

    if pack_version.is_empty() {
        pack_version = fs::read_to_string(base_dir.join(CONFIG_FILE))
            .ok()
            .and_then(|s| serde_json::from_str::<RepoConfig>(&s).ok())
            .map(|c| c.pack_version)
            .unwrap_or_default();
    }

    // 2. Докачиваем моды с Modrinth CDN
    let total_mr = mr_to_download.len();
    for (idx, (rel_path, url, expected_sha256, known_size)) in mr_to_download.iter().enumerate() {
        let local_path = base_dir.join(rel_path);
        let pct = if total_download_bytes > 0 {
            10 + (((total_downloaded_bytes as f64 / total_download_bytes as f64) * 80.0) as u32)
        } else {
            45 + ((idx as f32 / total_mr.max(1) as f32) * 45.0) as u32
        };
        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: pct.min(92),
                status: format!("Скачивание с Modrinth ({}/{}): {}", idx + 1, total_mr, rel_path),
                log: Some(format!("⬇ Modrinth: {} ({})", rel_path, format_bytes_ru(*known_size))),
            },
        );

        let bytes = download_raw_bytes_stream(
            &app,
            &cdn_client,
            url,
            rel_path,
            *known_size,
            total_downloaded_bytes,
            total_download_bytes,
            &mut smooth_speed_bps,
        )
        .await?;
        total_downloaded_bytes = total_downloaded_bytes.saturating_add(bytes.len() as u64);

        if !expected_sha256.is_empty() {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected_sha256) {
                return Err(format!(
                    "Несовпадение SHA-256 для {}: ожидался {}, получен {}",
                    rel_path, expected_sha256, actual
                ));
            }
        }
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&local_path, &bytes)
            .map_err(|e| format!("Не удалось записать {}: {}", local_path.display(), e))?;
        downloaded_modrinth += 1;
    }

    // 3. Удаляем локальные устаревшие файлы сборки, которых больше нет в удалённой версии
    let mut removed_stale = 0usize;
    for local_rel in collect_managed_local_files(&base_dir)? {
        if !expected_paths.contains(&local_rel) {
            let full = base_dir.join(&local_rel);
            if full.is_file() && fs::remove_file(&full).is_ok() {
                removed_stale += 1;
            }
        }
    }

    write_sync_state(
        &base_dir,
        &SyncState {
            provider: prov.to_string(),
            repo_owner: owner.to_string(),
            repo_name: repo.to_string(),
            branch: branch.to_string(),
            commit_sha: head.commit_sha.clone(),
            pack_version: pack_version.clone(),
        },
    )?;

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 100,
            status: format!("Сборка успешно синхронизирована с {}!", label),
            log: Some(format!(
                "✅ Готово! Коммит {}: скачано с {} {}, с Modrinth {}, из кэша {}, удалено устаревших {}",
                &head.commit_sha[..7.min(head.commit_sha.len())],
                label,
                downloaded_github,
                downloaded_modrinth,
                skipped_cached,
                removed_stale
            )),
        },
    );

    Ok(DownloadSummary {
        dir: base_dir.to_string_lossy().to_string(),
        commit_sha: head.commit_sha,
        pack_version,
        downloaded_github,
        downloaded_modrinth,
        skipped_cached,
        removed_stale,
    })
}

fn github_client(token: &str) -> Result<reqwest::Client, String> {
    use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
    let mut headers = HeaderMap::new();
    let auth_val = HeaderValue::from_str(&format!("Bearer {}", token.trim()))
        .map_err(|_| "Некорректные символы в GitHub токене.".to_string())?;
    headers.insert(AUTHORIZATION, auth_val);
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        HeaderValue::from_static("2022-11-28"),
    );

    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .map_err(|e| e.to_string())
}

fn map_github_http_error(status: reqwest::StatusCode, body: &str, owner: &str, repo: &str) -> String {
    match status.as_u16() {
        401 => "Неверный или истёкший GitHub токен (401 Unauthorized).".into(),
        403 => format!(
            "Нет прав на запись в {}/{} (403 Forbidden). Проверьте, что у токена есть право Contents: Read and write и пользователь добавлен в Collaborators.",
            owner, repo
        ),
        404 => format!(
            "Репозиторий {}/{} или ветка не найдены, либо у токена нет к нему доступа (404).",
            owner, repo
        ),
        _ => format!("Ошибка GitHub API ({}): {}", status, body),
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
async fn check_github_access(
    owner: String,
    repo: String,
    token: String,
    provider: Option<String>,
) -> Result<GithubAccessInfo, String> {
    let prov = normalize_provider(provider.as_deref());
    let label = provider_label(prov);
    let trimmed_token = token.trim();
    if trimmed_token.is_empty() {
        return Err(format!("Введите {} токен.", label));
    }
    let owner = owner.trim();
    let repo = repo.trim();

    if prov == "gitlab" {
        let client = gitlab_client(trimmed_token)?;
        let pid = gitlab_project_id(owner, repo);
        let repo_url = format!("https://gitlab.com/api/v4/projects/{}", pid);
        let resp = client
            .get(&repo_url)
            .send()
            .await
            .map_err(|e| format!("Ошибка соединения с GitLab: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(map_gitlab_http_error(status, &body, owner, repo));
        }

        let repo_json: Value = resp.json().await.map_err(|e| e.to_string())?;
        let proj_level = repo_json["permissions"]["project_access"]["access_level"]
            .as_u64()
            .unwrap_or(0);
        let group_level = repo_json["permissions"]["group_access"]["access_level"]
            .as_u64()
            .unwrap_or(0);
        let can_push = proj_level.max(group_level) >= 30;
        let private_repo = repo_json["visibility"].as_str() == Some("private");

        let login = match client.get("https://gitlab.com/api/v4/user").send().await {
            Ok(u_resp) if u_resp.status().is_success() => u_resp
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| v["username"].as_str().map(|s| s.to_string())),
            _ => None,
        };

        let _ = save_token_for("gitlab", trimmed_token);

        return Ok(GithubAccessInfo {
            login,
            can_push,
            private_repo,
        });
    }

    let client = github_client(trimmed_token)?;

    let repo_url = format!("https://api.github.com/repos/{}/{}", owner, repo);
    let resp = client
        .get(&repo_url)
        .send()
        .await
        .map_err(|e| format!("Ошибка соединения с GitHub: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(map_github_http_error(status, &body, owner, repo));
    }

    let repo_json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let perms = &repo_json["permissions"];
    let can_push = perms["push"].as_bool().unwrap_or(false)
        || perms["admin"].as_bool().unwrap_or(false)
        || perms["maintain"].as_bool().unwrap_or(false);
    let private_repo = repo_json["private"].as_bool().unwrap_or(false);

    let login = match client.get("https://api.github.com/user").send().await {
        Ok(u_resp) if u_resp.status().is_success() => u_resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v["login"].as_str().map(|s| s.to_string())),
        _ => None,
    };

    let _ = save_token_for("github", trimmed_token);

    Ok(GithubAccessInfo {
        login,
        can_push,
        private_repo,
    })
}

fn compute_git_blob_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    let header = format!("blob {}\0", bytes.len());
    hasher.update(header.as_bytes());
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn is_managed_pack_path(path: &str) -> bool {
    if path == "manifest.json" || path == "repo_config.json" || EXTRA_ROOT_FILES.contains(&path) {
        return true;
    }
    if path.ends_with("/.gitkeep") || path == ".gitkeep" {
        return false;
    }
    path.starts_with("mods/")
        || path.starts_with("bundles/")
        || EXTRA_DIRS
            .iter()
            .any(|dir| path.starts_with(&format!("{}/", dir)))
}

async fn publish_to_gitlab_inner(
    app: &UiHandle,
    base_dir: &Path,
    config: &RepoConfig,
    manifest_val: &Value,
    token: &str,
    commit_message: Option<String>,
) -> Result<PublishSummary, String> {
    let owner = config.repo_owner.trim();
    let repo = config.repo_name.trim();
    let branch = config.branch.trim();
    let client = gitlab_client(token)?;
    let pid = gitlab_project_id(owner, repo);

    let mut desired_rel_paths: Vec<String> = vec![
        MANIFEST_FILE.to_string(),
        CONFIG_FILE.to_string(),
    ];
    if let Some(files) = manifest_val["files"].as_array() {
        for entry in files {
            if entry["source"].as_str() == Some("github") {
                if let Some(rel) = entry["path"].as_str() {
                    desired_rel_paths.push(rel.to_string());
                }
            }
        }
    }
    let desired_set: HashSet<String> = desired_rel_paths.iter().cloned().collect();

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 15,
            status: format!("Подключение к GitLab ({}/{})...", owner, repo),
            log: Some(format!("☁ Синхронизация с GitLab: {}/{} ({})", owner, repo, branch)),
        },
    );

    let proj_url = format!("https://gitlab.com/api/v4/projects/{}", pid);
    let proj_resp = client
        .get(&proj_url)
        .send()
        .await
        .map_err(|e| format!("Ошибка соединения с GitLab: {}", e))?;
    if !proj_resp.status().is_success() {
        let status = proj_resp.status();
        let body = proj_resp.text().await.unwrap_or_default();
        return Err(map_gitlab_http_error(status, &body, owner, repo));
    }

    let head_commit_url = format!(
        "https://gitlab.com/api/v4/projects/{}/repository/commits/{}",
        pid,
        urlencoding::encode(branch)
    );
    let head_resp = client
        .get(&head_commit_url)
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса ветки {} на GitLab: {}", branch, e))?;

    let head_sha_opt: Option<String> = if head_resp.status().as_u16() == 404 {
        None
    } else if head_resp.status().is_success() {
        let json: Value = head_resp.json().await.map_err(|e| e.to_string())?;
        json["id"].as_str().map(|s| s.to_string())
    } else {
        let status = head_resp.status();
        let body = head_resp.text().await.unwrap_or_default();
        return Err(map_gitlab_http_error(status, &body, owner, repo));
    };

    if let (Some(head_sha), Some(local_sync)) = (head_sha_opt.as_ref(), read_sync_state(base_dir)) {
        if local_sync.provider.eq_ignore_ascii_case("gitlab")
            && local_sync.repo_owner.eq_ignore_ascii_case(owner)
            && local_sync.repo_name.eq_ignore_ascii_case(repo)
            && local_sync.branch == branch
            && local_sync.commit_sha != *head_sha
        {
            return Err(format!(
                "На GitLab есть более новый коммит ({}), чем локально ({}). Сначала скачайте обновление!",
                &head_sha[..7.min(head_sha.len())],
                &local_sync.commit_sha[..7.min(local_sync.commit_sha.len())]
            ));
        }
    }

    let mut remote_blobs: HashMap<String, String> = HashMap::new();
    if let Some(ref head_sha) = head_sha_opt {
        let items = fetch_gitlab_tree_items(&client, owner, repo, head_sha).await?;
        for (path, sha, _) in items {
            if is_managed_pack_path(&path) {
                remote_blobs.insert(path, sha);
            }
        }
    }

    let total_desired = desired_rel_paths.len().max(1);
    let mut uploaded_count = 0usize;
    let mut reused_count = 0usize;
    let mut removed_count = 0usize;
    let mut content_actions: Vec<(Value, usize)> = Vec::new();
    let mut final_actions: Vec<Value> = Vec::new();

    for (idx, rel_path) in desired_rel_paths.iter().enumerate() {
        let local_path = base_dir.join(rel_path);
        let bytes = fs::read(&local_path)
            .map_err(|e| format!("Не найден файл для загрузки {}: {}", local_path.display(), e))?;

        if bytes.len() > 95 * 1024 * 1024 {
            return Err(format!(
                "Файл {} превышает лимит загрузки (~95 MB).",
                rel_path
            ));
        }

        let local_blob_sha = compute_git_blob_sha1(&bytes);
        if remote_blobs.get(rel_path) == Some(&local_blob_sha) {
            reused_count += 1;
            continue;
        }

        let pct = 30 + ((idx as f32 / total_desired as f32) * 50.0) as u32;
        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: pct,
                status: format!("Подготовка для GitLab ({}/{}): {}", idx + 1, total_desired, rel_path),
                log: Some(format!("⬆ Подготовка {}", rel_path)),
            },
        );

        let action_type = if remote_blobs.contains_key(rel_path) {
            "update"
        } else {
            "create"
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let b64_len = b64.len();
        let entry = json!({
            "action": action_type,
            "file_path": rel_path,
            "content": b64,
            "encoding": "base64"
        });
        uploaded_count += 1;

        if rel_path == MANIFEST_FILE || rel_path == CONFIG_FILE {
            final_actions.push(entry);
        } else {
            content_actions.push((entry, b64_len));
        }
    }

    for remote_path in remote_blobs.keys() {
        if !desired_set.contains(remote_path) {
            final_actions.push(json!({
                "action": "delete",
                "file_path": remote_path
            }));
            removed_count += 1;
        }
    }

    let repo_url = format!("https://gitlab.com/{}/{}", owner, repo);
    if uploaded_count == 0 && removed_count == 0 {
        if let Some(head_sha) = head_sha_opt {
            let _ = write_sync_state(
                base_dir,
                &SyncState {
                    provider: "gitlab".into(),
                    repo_owner: owner.to_string(),
                    repo_name: repo.to_string(),
                    branch: branch.to_string(),
                    commit_sha: head_sha.clone(),
                    pack_version: config.pack_version.clone(),
                },
            );
            let _ = app.emit(
                "gen-progress",
                GenProgress {
                    percent: 100,
                    status: "На GitLab уже актуальная версия!".into(),
                    log: Some("✓ Изменений для отправки на GitLab нет.".into()),
                },
            );
            return Ok(PublishSummary {
                commit_sha: head_sha,
                uploaded_count: 0,
                reused_count,
                removed_count: 0,
                no_changes: true,
                repo_url,
            });
        }
    }

    let base_msg = commit_message
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            format!(
                "Update modpack {} (MC {} {} {})",
                config.pack_version, config.minecraft_version, config.loader_type, config.loader_version
            )
        });

    // Разбиваем крупные пачки файлов на коммиты до ~15 МБ, а manifest/удаления отправляем финальным коммитом.
    const MAX_BATCH_B64_BYTES: usize = 15 * 1024 * 1024;
    let mut batches: Vec<Vec<Value>> = Vec::new();
    let mut cur_batch: Vec<Value> = Vec::new();
    let mut cur_bytes: usize = 0;

    for (entry, sz) in content_actions {
        if !cur_batch.is_empty() && cur_bytes + sz > MAX_BATCH_B64_BYTES {
            batches.push(cur_batch);
            cur_batch = Vec::new();
            cur_bytes = 0;
        }
        cur_batch.push(entry);
        cur_bytes += sz;
    }
    if !cur_batch.is_empty() {
        let final_est: usize = final_actions
            .iter()
            .filter_map(|v| v["content"].as_str().map(|s| s.len()))
            .sum();
        if cur_bytes + final_est <= MAX_BATCH_B64_BYTES {
            cur_batch.extend(final_actions);
            batches.push(cur_batch);
        } else {
            batches.push(cur_batch);
            if !final_actions.is_empty() {
                batches.push(final_actions);
            }
        }
    } else if !final_actions.is_empty() {
        batches.push(final_actions);
    }

    let commits_api_url = format!("https://gitlab.com/api/v4/projects/{}/repository/commits", pid);
    let total_batches = batches.len().max(1);
    let mut new_commit_sha = String::new();

    for (b_idx, actions) in batches.into_iter().enumerate() {
        let pct = 80 + (((b_idx + 1) as f32 / total_batches as f32) * 18.0) as u32;
        let msg = if total_batches > 1 && b_idx + 1 < total_batches {
            format!("{} [part {}/{}]", base_msg, b_idx + 1, total_batches)
        } else {
            base_msg.clone()
        };
        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: pct.min(98),
                status: format!("Отправка коммита на GitLab ({}/{})...", b_idx + 1, total_batches),
                log: Some(format!(
                    "📤 Отправка пакета {}/{} на GitLab ({} действий)...",
                    b_idx + 1,
                    total_batches,
                    actions.len()
                )),
            },
        );

        let resp = client
            .post(&commits_api_url)
            .json(&json!({
                "branch": branch,
                "commit_message": msg,
                "actions": actions
            }))
            .send()
            .await
            .map_err(|e| format!("Ошибка создания коммита на GitLab: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(map_gitlab_http_error(status, &body, owner, repo));
        }
        let created: Value = resp.json().await.map_err(|e| e.to_string())?;
        new_commit_sha = created["id"]
            .as_str()
            .ok_or_else(|| "GitLab не вернул ID созданного коммита.".to_string())?
            .to_string();
    }

    let _ = write_sync_state(
        base_dir,
        &SyncState {
            provider: "gitlab".into(),
            repo_owner: owner.to_string(),
            repo_name: repo.to_string(),
            branch: branch.to_string(),
            commit_sha: new_commit_sha.clone(),
            pack_version: config.pack_version.clone(),
        },
    );

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 100,
            status: "Обновление опубликовано на GitLab!".into(),
            log: Some(format!(
                "🚀 Опубликовано на GitLab! Коммит {} (загружено: {}, без изменений: {}, удалено: {})",
                &new_commit_sha[..7.min(new_commit_sha.len())],
                uploaded_count,
                reused_count,
                removed_count
            )),
        },
    );

    Ok(PublishSummary {
        commit_sha: new_commit_sha,
        uploaded_count,
        reused_count,
        removed_count,
        no_changes: false,
        repo_url,
    })
}

async fn cmd_publish_to_github(
    app: UiHandle,
    dir: String,
    token: String,
    commit_message: Option<String>,
    provider: Option<String>,
) -> Result<PublishSummary, String> {
    let prov = normalize_provider(provider.as_deref());
    let label = provider_label(prov);
    let trimmed_token = token.trim();
    if trimmed_token.is_empty() {
        return Err(format!("Укажите {} токен для загрузки обновления на сервер.", label));
    }
    save_token_for(prov, trimmed_token)?;

    let base_dir = PathBuf::from(dir.trim());
    let config_path = base_dir.join(CONFIG_FILE);
    let manifest_path = base_dir.join(MANIFEST_FILE);

    if !config_path.exists() || !manifest_path.exists() {
        return Err("Сначала сгенерируйте manifest.json и repo_config.json.".into());
    }

    let config: RepoConfig = serde_json::from_str(
        &fs::read_to_string(&config_path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Ошибка чтения repo_config.json: {}", e))?;

    let manifest_val: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Ошибка чтения manifest.json: {}", e))?;

    if prov == "gitlab" {
        return publish_to_gitlab_inner(
            &app,
            &base_dir,
            &config,
            &manifest_val,
            trimmed_token,
            commit_message,
        )
        .await;
    }

    let owner = config.repo_owner.trim();
    let repo = config.repo_name.trim();
    let branch = config.branch.trim();
    let client = github_client(trimmed_token)?;

    // Собираем файлы, которые должны лежать на GitHub
    let mut desired_rel_paths: Vec<String> = vec![
        MANIFEST_FILE.to_string(),
        CONFIG_FILE.to_string(),
    ];
    if let Some(files) = manifest_val["files"].as_array() {
        for entry in files {
            if entry["source"].as_str() == Some("github") {
                if let Some(rel) = entry["path"].as_str() {
                    desired_rel_paths.push(rel.to_string());
                }
            }
        }
    }
    let desired_set: HashSet<String> = desired_rel_paths.iter().cloned().collect();

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 15,
            status: format!("Подключение к GitHub ({}/{})...", owner, repo),
            log: Some(format!("☁ Синхронизация с GitHub: {}/{} ({})", owner, repo, branch)),
        },
    );

    // 1. Текущий HEAD ветки
    let ref_url = format!(
        "https://api.github.com/repos/{}/{}/git/ref/heads/{}",
        owner,
        repo,
        urlencoding::encode(branch)
    );
    let ref_resp = client
        .get(&ref_url)
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса ветки {}: {}", branch, e))?;
    if !ref_resp.status().is_success() {
        let status = ref_resp.status();
        let body = ref_resp.text().await.unwrap_or_default();
        return Err(map_github_http_error(status, &body, owner, repo));
    }
    let ref_json: Value = ref_resp.json().await.map_err(|e| e.to_string())?;
    let head_sha = ref_json["object"]["sha"]
        .as_str()
        .ok_or_else(|| "Не удалось получить SHA последнего коммита ветки.".to_string())?
        .to_string();

    if let Some(local_sync) = read_sync_state(&base_dir) {
        if local_sync.provider.eq_ignore_ascii_case("github")
            && local_sync.repo_owner.eq_ignore_ascii_case(owner)
            && local_sync.repo_name.eq_ignore_ascii_case(repo)
            && local_sync.branch == branch
            && local_sync.commit_sha != head_sha
        {
            return Err(format!(
                "На GitHub есть более новый коммит ({}), чем локально ({}). Сначала скачайте обновление!",
                &head_sha[..7.min(head_sha.len())],
                &local_sync.commit_sha[..7.min(local_sync.commit_sha.len())]
            ));
        }
    }

    // 2. Дерево последнего коммита
    let commit_url = format!(
        "https://api.github.com/repos/{}/{}/git/commits/{}",
        owner, repo, head_sha
    );
    let commit_json: Value = client
        .get(&commit_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let base_tree_sha = commit_json["tree"]["sha"]
        .as_str()
        .ok_or_else(|| "Не удалось получить tree SHA.".to_string())?
        .to_string();

    let tree_url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        owner, repo, base_tree_sha
    );
    let tree_json: Value = client
        .get(&tree_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mut remote_blobs: HashMap<String, (String, String)> = HashMap::new(); // path -> (sha, mode)
    let mut new_tree_entries: Vec<Value> = Vec::new();
    let mut removed_count = 0usize;

    if let Some(items) = tree_json["tree"].as_array() {
        for item in items {
            if item["type"].as_str() != Some("blob") {
                continue;
            }
            let Some(path) = item["path"].as_str() else {
                continue;
            };
            let Some(sha) = item["sha"].as_str() else {
                continue;
            };
            let mode = item["mode"].as_str().unwrap_or("100644");

            if is_managed_pack_path(path) {
                if desired_set.contains(path) {
                    remote_blobs.insert(path.to_string(), (sha.to_string(), mode.to_string()));
                } else {
                    removed_count += 1;
                }
            } else {
                new_tree_entries.push(json!({
                    "path": path,
                    "mode": mode,
                    "type": "blob",
                    "sha": sha,
                }));
            }
        }
    }

    // 3. Проверяем/загружаем локальные файлы
    let total_desired = desired_rel_paths.len().max(1);
    let mut uploaded_count = 0usize;
    let mut reused_count = 0usize;

    for (idx, rel_path) in desired_rel_paths.iter().enumerate() {
        let local_path = base_dir.join(rel_path);
        let bytes = fs::read(&local_path)
            .map_err(|e| format!("Не найден файл для загрузки {}: {}", local_path.display(), e))?;

        if bytes.len() > 95 * 1024 * 1024 {
            return Err(format!(
                "Файл {} превышает лимит GitHub API (~95 MB).",
                rel_path
            ));
        }

        let local_blob_sha = compute_git_blob_sha1(&bytes);
        if let Some((remote_sha, remote_mode)) = remote_blobs.get(rel_path) {
            if remote_sha == &local_blob_sha {
                new_tree_entries.push(json!({
                    "path": rel_path,
                    "mode": remote_mode,
                    "type": "blob",
                    "sha": remote_sha,
                }));
                reused_count += 1;
                continue;
            }
        }

        let pct = 30 + ((idx as f32 / total_desired as f32) * 55.0) as u32;
        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: pct,
                status: format!("Загрузка на GitHub ({}/{}): {}", idx + 1, total_desired, rel_path),
                log: Some(format!("⬆ Загрузка {}", rel_path)),
            },
        );

        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let blob_url = format!("https://api.github.com/repos/{}/{}/git/blobs", owner, repo);
        let blob_resp = client
            .post(&blob_url)
            .json(&json!({
                "content": b64,
                "encoding": "base64"
            }))
            .send()
            .await
            .map_err(|e| format!("Ошибка загрузки {}: {}", rel_path, e))?;

        if !blob_resp.status().is_success() {
            let status = blob_resp.status();
            let body = blob_resp.text().await.unwrap_or_default();
            return Err(map_github_http_error(status, &body, owner, repo));
        }
        let blob_json: Value = blob_resp.json().await.map_err(|e| e.to_string())?;
        let new_sha = blob_json["sha"]
            .as_str()
            .ok_or_else(|| format!("GitHub не вернул blob SHA для {}", rel_path))?;

        new_tree_entries.push(json!({
            "path": rel_path,
            "mode": "100644",
            "type": "blob",
            "sha": new_sha,
        }));
        uploaded_count += 1;
    }

    let repo_url = format!("https://github.com/{}/{}", owner, repo);
    if uploaded_count == 0 && removed_count == 0 {
        let _ = write_sync_state(
            &base_dir,
            &SyncState {
                provider: "github".into(),
                repo_owner: owner.to_string(),
                repo_name: repo.to_string(),
                branch: branch.to_string(),
                commit_sha: head_sha.clone(),
                pack_version: config.pack_version.clone(),
            },
        );
        let _ = app.emit(
            "gen-progress",
            GenProgress {
                percent: 100,
                status: "На GitHub уже актуальная версия!".into(),
                log: Some("✓ Изменений для отправки на GitHub нет.".into()),
            },
        );
        return Ok(PublishSummary {
            commit_sha: head_sha,
            uploaded_count: 0,
            reused_count,
            removed_count: 0,
            no_changes: true,
            repo_url,
        });
    }

    // 4. Создаём новое дерево, коммит и обновляем ветку
    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 90,
            status: "Публикация коммита на GitHub...".into(),
            log: Some(format!(
                "Создание коммита (обновлено: {}, удалено устаревших: {})...",
                uploaded_count, removed_count
            )),
        },
    );

    let create_tree_url = format!("https://api.github.com/repos/{}/{}/git/trees", owner, repo);
    let create_tree_resp = client
        .post(&create_tree_url)
        .json(&json!({ "tree": new_tree_entries }))
        .send()
        .await
        .map_err(|e| format!("Ошибка создания дерева Git: {}", e))?;
    if !create_tree_resp.status().is_success() {
        let status = create_tree_resp.status();
        let body = create_tree_resp.text().await.unwrap_or_default();
        return Err(map_github_http_error(status, &body, owner, repo));
    }
    let created_tree: Value = create_tree_resp.json().await.map_err(|e| e.to_string())?;
    let new_tree_sha = created_tree["sha"]
        .as_str()
        .ok_or_else(|| "GitHub не вернул SHA нового дерева.".to_string())?;

    let msg = commit_message
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            format!(
                "Update modpack {} (MC {} {} {})",
                config.pack_version, config.minecraft_version, config.loader_type, config.loader_version
            )
        });

    let create_commit_url = format!("https://api.github.com/repos/{}/{}/git/commits", owner, repo);
    let create_commit_resp = client
        .post(&create_commit_url)
        .json(&json!({
            "message": msg,
            "tree": new_tree_sha,
            "parents": [head_sha]
        }))
        .send()
        .await
        .map_err(|e| format!("Ошибка создания коммита: {}", e))?;
    if !create_commit_resp.status().is_success() {
        let status = create_commit_resp.status();
        let body = create_commit_resp.text().await.unwrap_or_default();
        return Err(map_github_http_error(status, &body, owner, repo));
    }
    let created_commit: Value = create_commit_resp.json().await.map_err(|e| e.to_string())?;
    let new_commit_sha = created_commit["sha"]
        .as_str()
        .ok_or_else(|| "GitHub не вернул SHA коммита.".to_string())?
        .to_string();

    let patch_ref_url = format!(
        "https://api.github.com/repos/{}/{}/git/refs/heads/{}",
        owner,
        repo,
        urlencoding::encode(branch)
    );
    let patch_resp = client
        .patch(&patch_ref_url)
        .json(&json!({
            "sha": new_commit_sha,
            "force": false
        }))
        .send()
        .await
        .map_err(|e| format!("Ошибка обновления ветки {}: {}", branch, e))?;
    if !patch_resp.status().is_success() {
        let status = patch_resp.status();
        let body = patch_resp.text().await.unwrap_or_default();
        return Err(map_github_http_error(status, &body, owner, repo));
    }

    let _ = write_sync_state(
        &base_dir,
        &SyncState {
            provider: "github".into(),
            repo_owner: owner.to_string(),
            repo_name: repo.to_string(),
            branch: branch.to_string(),
            commit_sha: new_commit_sha.clone(),
            pack_version: config.pack_version.clone(),
        },
    );

    let _ = app.emit(
        "gen-progress",
        GenProgress {
            percent: 100,
            status: "Обновление опубликовано на GitHub!".into(),
            log: Some(format!(
                "🚀 Опубликовано! Коммит {} (загружено: {}, без изменений: {}, удалено: {})",
                &new_commit_sha[..7.min(new_commit_sha.len())],
                uploaded_count,
                reused_count,
                removed_count
            )),
        },
    );

    Ok(PublishSummary {
        commit_sha: new_commit_sha,
        uploaded_count,
        reused_count,
        removed_count,
        no_changes: false,
        repo_url,
    })
}

#[cfg(feature = "desktop")]
mod tauri_commands {
    use super::*;
    use tauri::AppHandle;

    #[tauri::command]
    pub async fn generate_manifest(
        app: AppHandle,
        dir: String,
        config: RepoConfig,
    ) -> Result<GenerateSummary, String> {
        cmd_generate_manifest(UiHandle::from_tauri(app), dir, config).await
    }

    #[tauri::command]
    pub async fn download_modpack_from_github(
        app: AppHandle,
        dir: String,
        owner: String,
        repo: String,
        branch: String,
        token: String,
        provider: Option<String>,
    ) -> Result<DownloadSummary, String> {
        cmd_download_modpack_from_github(
            UiHandle::from_tauri(app),
            dir,
            owner,
            repo,
            branch,
            token,
            provider,
        )
        .await
    }

    #[tauri::command]
    pub async fn publish_to_github(
        app: AppHandle,
        dir: String,
        token: String,
        commit_message: Option<String>,
        provider: Option<String>,
    ) -> Result<PublishSummary, String> {
        cmd_publish_to_github(
            UiHandle::from_tauri(app),
            dir,
            token,
            commit_message,
            provider,
        )
        .await
    }
}

fn dispatch_web_command(
    cmd: String,
    args: Value,
    ui: UiHandle,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
    Box::pin(async move {
        match cmd.as_str() {
            "detect_initial_dir" => Ok(Value::String(detect_initial_dir())),
            "pick_folder" => {
                let initial_dir = args["initialDir"]
                    .as_str()
                    .or_else(|| args["initial_dir"].as_str())
                    .unwrap_or("")
                    .to_string();
                let res = pick_folder(initial_dir).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "open_folder" => {
                let dir = args["dir"].as_str().unwrap_or("").to_string();
                open_folder(dir)?;
                Ok(Value::Null)
            }
            "inspect_modpack_dir" => {
                let dir = args["dir"].as_str().unwrap_or("").to_string();
                let res = inspect_modpack_dir(dir)?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "fetch_loader_versions" => {
                let loader_type = args["loaderType"]
                    .as_str()
                    .or_else(|| args["loader_type"].as_str())
                    .unwrap_or("")
                    .to_string();
                let mc_version = args["mcVersion"]
                    .as_str()
                    .or_else(|| args["mc_version"].as_str())
                    .unwrap_or("")
                    .to_string();
                let res = fetch_loader_versions(loader_type, mc_version).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "generate_manifest" => {
                let dir = args["dir"].as_str().unwrap_or("").to_string();
                let config: RepoConfig = serde_json::from_value(args["config"].clone())
                    .map_err(|e| format!("Некорректный RepoConfig: {}", e))?;
                let res = cmd_generate_manifest(ui, dir, config).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "load_github_token" => Ok(Value::String(load_github_token())),
            "save_github_token" => {
                let token = args["token"].as_str().unwrap_or("").to_string();
                save_github_token(token)?;
                Ok(Value::Null)
            }
            "load_gitlab_token" => Ok(Value::String(load_gitlab_token())),
            "save_gitlab_token" => {
                let token = args["token"].as_str().unwrap_or("").to_string();
                save_gitlab_token(token)?;
                Ok(Value::Null)
            }
            "check_github_access" => {
                let owner = args["owner"].as_str().unwrap_or("").to_string();
                let repo = args["repo"].as_str().unwrap_or("").to_string();
                let token = args["token"].as_str().unwrap_or("").to_string();
                let provider = args["provider"].as_str().map(|s| s.to_string());
                let res = check_github_access(owner, repo, token, provider).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "check_remote_sync" => {
                let dir = args["dir"].as_str().unwrap_or("").to_string();
                let owner = args["owner"].as_str().unwrap_or("").to_string();
                let repo = args["repo"].as_str().unwrap_or("").to_string();
                let branch = args["branch"].as_str().unwrap_or("").to_string();
                let token = args["token"].as_str().unwrap_or("").to_string();
                let provider = args["provider"].as_str().map(|s| s.to_string());
                let res = check_remote_sync(dir, owner, repo, branch, token, provider).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "download_modpack_from_github" => {
                let dir = args["dir"].as_str().unwrap_or("").to_string();
                let owner = args["owner"].as_str().unwrap_or("").to_string();
                let repo = args["repo"].as_str().unwrap_or("").to_string();
                let branch = args["branch"].as_str().unwrap_or("").to_string();
                let token = args["token"].as_str().unwrap_or("").to_string();
                let provider = args["provider"].as_str().map(|s| s.to_string());
                let res = cmd_download_modpack_from_github(
                    ui, dir, owner, repo, branch, token, provider,
                )
                .await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "publish_to_github" => {
                let dir = args["dir"].as_str().unwrap_or("").to_string();
                let token = args["token"].as_str().unwrap_or("").to_string();
                let commit_message = args["commitMessage"]
                    .as_str()
                    .or_else(|| args["commit_message"].as_str())
                    .map(|s| s.to_string());
                let provider = args["provider"].as_str().map(|s| s.to_string());
                let res = cmd_publish_to_github(ui, dir, token, commit_message, provider).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            _ => Err(format!("Неизвестная команда: {}", cmd)),
        }
    })
}

fn main() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let force_web = std::env::args().any(|a| a == "--web")
        || std::env::var_os("ETERNUM_WEBUI").is_some();

    #[cfg(feature = "desktop")]
    if !force_web && ui_bridge::has_webview2_runtime() {
        tauri::Builder::default()
            .invoke_handler(tauri::generate_handler![
                detect_initial_dir,
                pick_folder,
                open_folder,
                inspect_modpack_dir,
                fetch_loader_versions,
                tauri_commands::generate_manifest,
                load_github_token,
                save_github_token,
                load_gitlab_token,
                save_gitlab_token,
                check_github_access,
                check_remote_sync,
                tauri_commands::download_modpack_from_github,
                tauri_commands::publish_to_github
            ])
            .run(tauri::generate_context!())
            .expect("Ошибка запуска Eternum Modpack Manager");
        return;
    }

    let _ = force_web;
    webui_server::run_webui_blocking(
        "Eternum Modpack Manager",
        Arc::new(|cmd, args, ui| dispatch_web_command(cmd, args, ui)),
    );
}
