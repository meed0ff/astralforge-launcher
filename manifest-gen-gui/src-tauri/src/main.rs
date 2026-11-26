#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

const CONFIG_FILE: &str = "repo_config.json";
const MANIFEST_FILE: &str = "manifest.json";
const MODRINTH_API: &str = "https://api.modrinth.com/v2/version_files";
const USER_AGENT: &str = "Eternum-ManifestGen/1.0";

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
}

#[derive(Debug, Clone, Serialize)]
pub struct GenProgress {
    pub percent: u32,
    pub status: String,
    pub log: Option<String>,
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

#[tauri::command]
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

#[tauri::command]
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

    #[cfg(not(windows))]
    {
        let _ = initial_dir;
        Err("Выбор папки через проводник поддерживается только на Windows.".into())
    }
}

#[tauri::command]
fn open_folder(dir: String) -> Result<(), String> {
    let path = PathBuf::from(dir);
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    open::that(&path).map_err(|e| e.to_string())
}

#[tauri::command]
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

    Ok(ModpackOverview {
        dir: base_dir.to_string_lossy().to_string(),
        has_config,
        config,
        mods_count,
        extra_count,
        bundles_count,
    })
}

#[tauri::command]
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

#[tauri::command]
async fn generate_manifest(
    app: AppHandle,
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

fn github_token_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("EternumManifestGen").join("github_token.txt"))
}

#[tauri::command]
fn load_github_token() -> String {
    let Some(path) = github_token_path() else {
        return String::new();
    };
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[tauri::command]
fn save_github_token(token: String) -> Result<(), String> {
    let Some(path) = github_token_path() else {
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

#[tauri::command]
async fn check_github_access(
    owner: String,
    repo: String,
    token: String,
) -> Result<GithubAccessInfo, String> {
    let trimmed_token = token.trim();
    if trimmed_token.is_empty() {
        return Err("Введите GitHub токен (PAT).".into());
    }
    let owner = owner.trim();
    let repo = repo.trim();
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

    let _ = save_github_token(trimmed_token.to_string());

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

#[tauri::command]
async fn publish_to_github(
    app: AppHandle,
    dir: String,
    token: String,
    commit_message: Option<String>,
) -> Result<PublishSummary, String> {
    let trimmed_token = token.trim();
    if trimmed_token.is_empty() {
        return Err("Укажите GitHub токен для загрузки обновления на сервер.".into());
    }
    save_github_token(trimmed_token.to_string())?;

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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            detect_initial_dir,
            pick_folder,
            open_folder,
            inspect_modpack_dir,
            fetch_loader_versions,
            generate_manifest,
            load_github_token,
            save_github_token,
            check_github_access,
            publish_to_github
        ])
        .run(tauri::generate_context!())
        .expect("Ошибка запуска Eternum Manifest Generator");
}
