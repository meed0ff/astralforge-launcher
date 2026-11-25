#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod downloader;
mod elyby;
mod game_launcher;
mod manifest;
mod server_ping;

use downloader::ProgressPayload;
use elyby::ElybyUser;
use manifest::Manifest;
use server_ping::ServerStatus;
use std::fs;
use tauri::{AppHandle, Emitter};

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
    #[serde(rename = "manifestUrl")]
    pub manifest_url: String,
}

#[tauri::command]
async fn check_server_status(ip: String, port: u16) -> Result<ServerStatus, String> {
    server_ping::ping_server(&ip, port).await
}

#[tauri::command]
async fn start_elyby_auth() -> Result<ElybyUser, String> {
    elyby::login_via_elyby().await
}

#[tauri::command]
async fn fetch_elyby_skin(username: String) -> Result<String, String> {
    use base64::Engine as _;
    let url = format!(
        "https://skinsystem.ely.by/skins/{}.png",
        urlencoding::encode(&username)
    );
    let client = reqwest::Client::builder()
        .user_agent("AstralForge-Launcher/1.0")
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

#[tauri::command]
fn open_game_folder() -> Result<(), String> {
    let path = game_launcher::get_game_dir();
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    open::that(&path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_sync_and_launch(app: AppHandle, request: LaunchRequest) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("AstralForge-Launcher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 5,
            status: "Загрузка манифеста сборки с GitHub...".into(),
            log: Some(format!("Запрос манифеста: {}", request.manifest_url)),
        },
    );

    // 1. Скачиваем manifest.json
    let manifest_resp = client
        .get(&request.manifest_url)
        .send()
        .await
        .map_err(|e| format!("Не удалось скачать манифест: {}", e))?;

    if !manifest_resp.status().is_success() {
        return Err(format!("Ошибка ответа манифеста: статус {}", manifest_resp.status()));
    }

    let manifest: Manifest = manifest_resp
        .json()
        .await
        .map_err(|e| format!("Ошибка разбора манифеста: {}", e))?;

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

    // 2. Синхронизируем файлы сборки (Modrinth CDN + GitHub)
    downloader::sync_files(&app, &client, &game_dir, &manifest.files).await?;

    // 3. Проверяем/скачиваем нужную Java и устанавливаем NeoForge
    let resolved_java = game_launcher::ensure_neoforge_installed(
        &app,
        &client,
        &game_dir,
        &request.java_path,
        &manifest,
    )
    .await?;

    // 4. Подготавливаем authlib-injector если выбран вход через Ely.by
    let authlib_path = if request.auth_type == "elyby" {
        Some(game_launcher::ensure_authlib_injector(&client, &game_dir).await?)
    } else {
        None
    };

    // 5. Запуск Minecraft
    game_launcher::launch_minecraft(
        &app,
        &game_dir,
        &resolved_java,
        request.ram_gb,
        &request.nickname,
        &request.auth_type,
        &request.uuid,
        &request.access_token,
        &manifest.server.ip,
        &manifest.minecraft.version,
        &manifest.loader.version,
        authlib_path.as_deref(),
    )
    .await?;

    Ok(())
}

fn main() {
    if elyby::try_forward_deep_link() {
        return;
    }
    elyby::register_uri_scheme_linux();

    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            check_server_status,
            start_elyby_auth,
            fetch_elyby_skin,
            open_game_folder,
            start_sync_and_launch
        ])
        .run(tauri::generate_context!())
        .expect("Ошибка запуска приложения Tauri");
}
