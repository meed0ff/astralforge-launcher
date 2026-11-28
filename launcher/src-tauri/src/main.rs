#![cfg_attr(all(not(debug_assertions), feature = "desktop"), windows_subsystem = "windows")]

mod downloader;
mod elyby;
mod game_launcher;
mod manifest;
mod server_ping;
mod ui_bridge;
mod webui_server;

use downloader::ProgressPayload;
use elyby::ElybyUser;
use game_launcher::{JavaInstallationInfo, RunningGameState};
use manifest::Manifest;
use serde_json::Value;
use server_ping::ServerStatus;
use std::fs;
use std::sync::Arc;
use ui_bridge::UiHandle;

fn default_mirror_source() -> String {
    "github".into()
}

fn default_true() -> bool {
    true
}

#[derive(serde::Deserialize)]
pub struct LaunchRequest {
    pub nickname: String,
    #[serde(rename = "authType")]
    pub auth_type: String,
    pub uuid: String,
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "ramGb")]
    pub ram_gb: u32,
    #[serde(rename = "javaPath")]
    pub java_path: String,
    #[serde(default = "default_true", rename = "strictJavaCheck")]
    pub strict_java_check: bool,
    #[serde(rename = "manifestUrl")]
    pub manifest_url: String,
    #[serde(default = "default_mirror_source", rename = "mirrorSource")]
    pub mirror_source: String,
}

async fn cmd_check_server_status(ip: String, port: u16) -> Result<ServerStatus, String> {
    server_ping::ping_server(&ip, port).await
}

async fn cmd_start_elyby_auth() -> Result<ElybyUser, String> {
    elyby::login_via_elyby().await
}

async fn cmd_fetch_elyby_skin(username: String) -> Result<String, String> {
    use base64::Engine as _;
    let url = format!(
        "https://skinsystem.ely.by/skins/{}.png",
        urlencoding::encode(&username)
    );
    let client = reqwest::Client::builder()
        .user_agent("Eternum-Launcher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Skin not found: {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/png;base64,{}", b64))
}

#[derive(serde::Serialize)]
pub struct ManifestSummary {
    pub pack_version: Option<String>,
    pub server_name: String,
    pub server_ip: String,
    pub minecraft_version: String,
    pub loader_type: String,
    pub loader_version: String,
    pub files_count: usize,
    pub total_pack_bytes: u64,
    pub needed_bytes: u64,
    pub free_disk_bytes: Option<u64>,
    pub enough_disk_space: Option<bool>,
}

async fn try_fetch_manifest_once(
    client: &reqwest::Client,
    url: &str,
) -> Result<(Manifest, String), String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Не удалось загрузить манифест ({}): {}", url, e))?;
    if !resp.status().is_success() {
        return Err(format!("Статус ответа манифеста ({}): {}", url, resp.status()));
    }
    let manifest: Manifest = resp
        .json()
        .await
        .map_err(|e| format!("Ошибка разбора манифеста: {}", e))?;
    Ok((manifest, url.to_string()))
}

async fn fetch_manifest_json(
    client: &reqwest::Client,
    manifest_url: &str,
    mirror_source: &str,
) -> Result<(Manifest, String), String> {
    let (primary, fallback) = downloader::resolve_mirror_urls(manifest_url, mirror_source);

    match try_fetch_manifest_once(client, &primary).await {
        Ok(res) => Ok(res),
        Err(primary_err) => {
            if let Some(fb) = fallback {
                try_fetch_manifest_once(client, &fb).await.map_err(|fb_err| {
                    format!("{}\nРезервное зеркало: {}", primary_err, fb_err)
                })
            } else {
                Err(primary_err)
            }
        }
    }
}

async fn cmd_fetch_manifest_info(
    manifest_url: String,
    mirror_source: Option<String>,
) -> Result<ManifestSummary, String> {
    let client = reqwest::Client::builder()
        .user_agent("Eternum-Launcher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let mirror = mirror_source.as_deref().unwrap_or("github");
    let (manifest, _) = fetch_manifest_json(&client, &manifest_url, mirror).await?;

    let game_dir = game_launcher::get_game_dir();
    let (total_pack_bytes, needed_bytes) = downloader::estimate_sync_sizes(&game_dir, &manifest.files);
    let free_disk_bytes = downloader::get_available_disk_space(&game_dir);
    let enough_disk_space = free_disk_bytes.map(|free| free >= needed_bytes);

    Ok(ManifestSummary {
        pack_version: manifest.pack_version,
        server_name: manifest.server.name,
        server_ip: manifest.server.ip,
        minecraft_version: manifest.minecraft.version,
        loader_type: manifest.loader.loader_type,
        loader_version: manifest.loader.version,
        files_count: manifest.files.len(),
        total_pack_bytes,
        needed_bytes,
        free_disk_bytes,
        enough_disk_space,
    })
}

fn cmd_open_game_folder() -> Result<(), String> {
    let path = game_launcher::get_game_dir();
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    open::that(&path).map_err(|e| e.to_string())
}

async fn cmd_detect_java_installations(
    required_major: Option<u32>,
) -> Result<Vec<JavaInstallationInfo>, String> {
    let game_dir = game_launcher::get_game_dir();
    Ok(game_launcher::scan_java_installations(&game_dir, required_major).await)
}

async fn cmd_install_java_runtime(app: UiHandle, major: u32) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("Eternum-Launcher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let game_dir = game_launcher::get_game_dir();
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    game_launcher::install_temurin_java(&app, &client, &game_dir, major).await
}

async fn cmd_start_sync_and_launch(
    app: UiHandle,
    state: RunningGameState,
    request: LaunchRequest,
) -> Result<(), String> {
    if state.is_running() {
        return Err("Игра уже запущена!".into());
    }

    let client = reqwest::Client::builder()
        .user_agent("Eternum-Launcher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let (primary_manifest_url, _) =
        downloader::resolve_mirror_urls(&request.manifest_url, &request.mirror_source);
    let source_label = if request.mirror_source.eq_ignore_ascii_case("gitlab") {
        "GitLab"
    } else {
        "GitHub"
    };

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 5,
            status: format!("Загрузка манифеста сборки ({})...", source_label),
            log: Some(format!("Запрос манифеста: {}", primary_manifest_url)),
        },
    );

    let (manifest, used_manifest_url) =
        fetch_manifest_json(&client, &request.manifest_url, &request.mirror_source).await?;
    if used_manifest_url != primary_manifest_url {
        let _ = app.emit(
            "download-progress",
            ProgressPayload {
                percent: 10,
                status: "Переключение на резервное зеркало манифеста...".into(),
                log: Some(format!("Используется резервный URL: {}", used_manifest_url)),
            },
        );
    }

    let game_dir = game_launcher::get_game_dir();
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 15,
            status: "Сверка модов и конфигураций...".into(),
            log: Some(format!("В манифесте файлов: {}", manifest.files.len())),
        },
    );

    downloader::sync_files(&app, &client, &game_dir, &manifest.files, &request.mirror_source)
        .await?;

    let resolved_java = game_launcher::ensure_loader_installed(
        &app,
        &client,
        &game_dir,
        &request.java_path,
        request.strict_java_check,
        &manifest,
    )
    .await?;

    let authlib_path = if request.auth_type == "elyby" {
        Some(game_launcher::ensure_authlib_injector(&client, &game_dir).await?)
    } else {
        None
    };

    game_launcher::launch_minecraft(
        &app,
        state,
        &game_dir,
        &resolved_java,
        request.ram_gb,
        &request.nickname,
        &request.auth_type,
        &request.uuid,
        &request.access_token,
        &manifest.server.ip,
        &manifest.minecraft.version,
        &manifest.loader.loader_type,
        &manifest.loader.version,
        authlib_path.as_deref(),
    )
    .await?;

    Ok(())
}

#[cfg(feature = "desktop")]
mod tauri_commands {
    use super::*;
    use tauri::AppHandle;

    #[tauri::command]
    pub async fn check_server_status(ip: String, port: u16) -> Result<ServerStatus, String> {
        cmd_check_server_status(ip, port).await
    }

    #[tauri::command]
    pub async fn start_elyby_auth() -> Result<ElybyUser, String> {
        cmd_start_elyby_auth().await
    }

    #[tauri::command]
    pub async fn fetch_elyby_skin(username: String) -> Result<String, String> {
        cmd_fetch_elyby_skin(username).await
    }

    #[tauri::command]
    pub async fn fetch_manifest_info(
        manifest_url: String,
        mirror_source: Option<String>,
    ) -> Result<ManifestSummary, String> {
        cmd_fetch_manifest_info(manifest_url, mirror_source).await
    }

    #[tauri::command]
    pub fn open_game_folder() -> Result<(), String> {
        cmd_open_game_folder()
    }

    #[tauri::command]
    pub async fn detect_java_installations(
        required_major: Option<u32>,
    ) -> Result<Vec<JavaInstallationInfo>, String> {
        cmd_detect_java_installations(required_major).await
    }

    #[tauri::command]
    pub async fn install_java_runtime(app: AppHandle, major: u32) -> Result<String, String> {
        cmd_install_java_runtime(UiHandle::from_tauri(app), major).await
    }

    #[tauri::command]
    pub async fn stop_game(state: tauri::State<'_, RunningGameState>) -> Result<(), String> {
        state.stop()
    }

    #[tauri::command]
    pub async fn start_sync_and_launch(
        app: AppHandle,
        state: tauri::State<'_, RunningGameState>,
        request: LaunchRequest,
    ) -> Result<(), String> {
        cmd_start_sync_and_launch(UiHandle::from_tauri(app), state.inner().clone(), request).await
    }
}

fn dispatch_web_command(
    cmd: String,
    args: Value,
    ui: UiHandle,
    state: RunningGameState,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send>> {
    Box::pin(async move {
        match cmd.as_str() {
            "check_server_status" => {
                let ip = args["ip"].as_str().unwrap_or("").to_string();
                let port = args["port"].as_u64().unwrap_or(25565) as u16;
                let res = cmd_check_server_status(ip, port).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "start_elyby_auth" => {
                let res = cmd_start_elyby_auth().await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "fetch_elyby_skin" => {
                let username = args["username"].as_str().unwrap_or("").to_string();
                let res = cmd_fetch_elyby_skin(username).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "fetch_manifest_info" => {
                let manifest_url = args["manifestUrl"]
                    .as_str()
                    .or_else(|| args["manifest_url"].as_str())
                    .unwrap_or("")
                    .to_string();
                let mirror_source = args["mirrorSource"]
                    .as_str()
                    .or_else(|| args["mirror_source"].as_str())
                    .map(|s| s.to_string());
                let res = cmd_fetch_manifest_info(manifest_url, mirror_source).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "open_game_folder" => {
                cmd_open_game_folder()?;
                Ok(Value::Null)
            }
            "detect_java_installations" => {
                let req = args["requiredMajor"]
                    .as_u64()
                    .or_else(|| args["required_major"].as_u64())
                    .map(|v| v as u32);
                let res = cmd_detect_java_installations(req).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "install_java_runtime" => {
                let major = args["major"].as_u64().unwrap_or(21) as u32;
                let res = cmd_install_java_runtime(ui, major).await?;
                serde_json::to_value(res).map_err(|e| e.to_string())
            }
            "stop_game" => {
                state.stop()?;
                Ok(Value::Null)
            }
            "start_sync_and_launch" => {
                let req_val = args.get("request").cloned().unwrap_or(args);
                let req: LaunchRequest = serde_json::from_value(req_val)
                    .map_err(|e| format!("Некорректные параметры запуска: {}", e))?;
                cmd_start_sync_and_launch(ui, state, req).await?;
                Ok(Value::Null)
            }
            _ => Err(format!("Неизвестная команда: {}", cmd)),
        }
    })
}

fn main() {
    if elyby::try_forward_deep_link() {
        return;
    }
    elyby::register_uri_scheme();

    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let force_web = std::env::args().any(|a| a == "--web")
        || std::env::var_os("ETERNUM_WEBUI").is_some();

    #[cfg(feature = "desktop")]
    if !force_web && ui_bridge::has_webview2_runtime() {
        tauri::Builder::default()
            .manage(RunningGameState::default())
            .invoke_handler(tauri::generate_handler![
                tauri_commands::check_server_status,
                tauri_commands::start_elyby_auth,
                tauri_commands::fetch_elyby_skin,
                tauri_commands::fetch_manifest_info,
                tauri_commands::open_game_folder,
                tauri_commands::detect_java_installations,
                tauri_commands::install_java_runtime,
                tauri_commands::start_sync_and_launch,
                tauri_commands::stop_game
            ])
            .run(tauri::generate_context!())
            .expect("Ошибка запуска приложения Tauri");
        return;
    }

    let _ = force_web;
    webui_server::run_webui_blocking(
        "Eternum Launcher",
        Arc::new(|cmd, args, ui, state| dispatch_web_command(cmd, args, ui, state)),
    );
}
