use crate::downloader::{download_file, ProgressPayload};
use crate::manifest::Manifest;
use futures_util::stream::{self, StreamExt};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

static NEXT_GAME_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub struct RunningGameState {
    inner: Arc<Mutex<Option<RunningGameHandle>>>,
}

struct RunningGameHandle {
    id: u64,
    kill_tx: Option<oneshot::Sender<()>>,
}

impl RunningGameState {
    pub fn is_running(&self) -> bool {
        self.inner.lock().map(|guard| guard.is_some()).unwrap_or(false)
    }

    fn set_running(&self, kill_tx: oneshot::Sender<()>) -> u64 {
        let id = NEXT_GAME_ID.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(RunningGameHandle {
            id,
            kill_tx: Some(kill_tx),
        });
        id
    }

    fn clear_if_matches(&self, id: u64) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_some_and(|h| h.id == id) {
            *guard = None;
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(handle) = guard.as_mut() else {
            return Err("Игра сейчас не запущена.".into());
        };
        if let Some(tx) = handle.kill_tx.take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}

pub const AUTHLIB_INJECTOR_META_URLS: &[&str] = &[
    "https://authlib-injector.yushi.moe/artifact/latest.json",
    "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/latest.json",
];
pub const AUTHLIB_INJECTOR_GITHUB_API_URL: &str =
    "https://api.github.com/repos/yushijinhun/authlib-injector/releases/latest";
pub const ELYBY_AUTH_SERVER: &str = "https://authserver.ely.by";
pub const MOJANG_VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

pub fn get_game_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join(".astralforge")
    } else {
        PathBuf::from(".astralforge")
    }
}

fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn rules_allow(rules: Option<&Vec<Value>>) -> bool {
    let Some(rules) = rules else {
        return true;
    };
    let os_name = current_os_name();
    let mut allowed = false;
    for rule in rules {
        let action = rule["action"].as_str().unwrap_or("allow");
        if let Some(features) = rule.get("features").and_then(|f| f.as_object()) {
            // Skip feature-gated demo/custom-resolution/quick-play rules; we pass --quickPlayMultiplayer explicitly
            if !features.is_empty() {
                continue;
            }
        }
        let os_match = if let Some(os_obj) = rule.get("os").and_then(|o| o.as_object()) {
            let name_ok = os_obj
                .get("name")
                .and_then(|n| n.as_str())
                .map_or(true, |n| n == os_name);
            let arch_ok = os_obj
                .get("arch")
                .and_then(|a| a.as_str())
                .map_or(true, |a| {
                    (cfg!(target_arch = "x86_64") && (a == "x86_64" || a == "x64"))
                        || (cfg!(target_arch = "aarch64") && a == "arm64")
                        || (cfg!(target_arch = "x86") && a == "x86")
                });
            name_ok && arch_ok
        } else {
            true
        };
        if os_match {
            allowed = action == "allow";
        }
    }
    allowed
}

/// Скачивает ванильный version.json, клиентский jar, библиотеки и ассеты для Minecraft
async fn ensure_vanilla_installed(
    app: &AppHandle,
    client: &reqwest::Client,
    game_dir: &Path,
    mc_version: &str,
) -> Result<Value, String> {
    let version_dir = game_dir.join("versions").join(mc_version);
    let version_json_path = version_dir.join(format!("{}.json", mc_version));
    std::fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

    if !version_json_path.exists() {
        let _ = app.emit(
            "download-progress",
            ProgressPayload {
                percent: 76,
                status: format!("Получение метаданных Minecraft {}...", mc_version),
                log: Some(format!("Загрузка version_manifest_v2.json для {}", mc_version)),
            },
        );
        let manifest_val: Value = client
            .get(MOJANG_VERSION_MANIFEST_URL)
            .send()
            .await
            .map_err(|e| format!("Не удалось получить список версий Mojang: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Ошибка разбора списка версий Mojang: {}", e))?;

        let version_url = manifest_val["versions"]
            .as_array()
            .and_then(|arr| arr.iter().find(|v| v["id"].as_str() == Some(mc_version)))
            .and_then(|v| v["url"].as_str())
            .ok_or_else(|| format!("Версия Minecraft {} не найдена у Mojang", mc_version))?
            .to_string();

        download_file(client, &version_url, &version_json_path, "").await?;
    }

    let json_text = std::fs::read_to_string(&version_json_path).map_err(|e| e.to_string())?;
    let version_json: Value = serde_json::from_str(&json_text).map_err(|e| e.to_string())?;

    // Клиентский jar
    let client_jar = version_dir.join(format!("{}.jar", mc_version));
    if !client_jar.exists() {
        if let Some(client_url) = version_json["downloads"]["client"]["url"].as_str() {
            let _ = app.emit(
                "download-progress",
                ProgressPayload {
                    percent: 78,
                    status: format!("Скачивание клиента Minecraft {}...", mc_version),
                    log: Some(format!("Скачивание {}.jar с серверов Mojang...", mc_version)),
                },
            );
            download_file(client, client_url, &client_jar, "").await?;
        }
    }

    // Библиотеки Mojang
    let libraries_dir = game_dir.join("libraries");
    let mut lib_downloads = Vec::new();
    if let Some(libs) = version_json["libraries"].as_array() {
        for lib in libs {
            if !rules_allow(lib.get("rules").and_then(|r| r.as_array())) {
                continue;
            }
            if let Some(artifact) = lib["downloads"]["artifact"].as_object() {
                if let (Some(path_str), Some(url)) = (
                    artifact.get("path").and_then(|p| p.as_str()),
                    artifact.get("url").and_then(|u| u.as_str()),
                ) {
                    let target = libraries_dir.join(path_str);
                    if !target.exists() {
                        lib_downloads.push((url.to_string(), target));
                    }
                }
            }
        }
    }

    if !lib_downloads.is_empty() {
        let _ = app.emit(
            "download-progress",
            ProgressPayload {
                percent: 80,
                status: format!("Скачивание библиотек Minecraft ({} шт.)...", lib_downloads.len()),
                log: Some(format!("Загрузка {} недостающих библиотек Mojang...", lib_downloads.len())),
            },
        );
        let results: Vec<Result<(), String>> = stream::iter(lib_downloads)
            .map(|(url, target)| async move { download_file(client, &url, &target, "").await })
            .buffer_unordered(12)
            .collect()
            .await;
        for res in results {
            res?;
        }
    }

    // Asset Index и ассеты
    if let (Some(idx_id), Some(idx_url)) = (
        version_json["assetIndex"]["id"].as_str(),
        version_json["assetIndex"]["url"].as_str(),
    ) {
        let index_path = game_dir
            .join("assets")
            .join("indexes")
            .join(format!("{}.json", idx_id));
        if !index_path.exists() {
            download_file(client, idx_url, &index_path, "").await?;
        }
        let idx_text = std::fs::read_to_string(&index_path).map_err(|e| e.to_string())?;
        let idx_json: Value = serde_json::from_str(&idx_text).map_err(|e| e.to_string())?;
        let objects_dir = game_dir.join("assets").join("objects");

        let mut asset_downloads = Vec::new();
        if let Some(objects) = idx_json["objects"].as_object() {
            for (_name, obj) in objects {
                if let Some(hash) = obj["hash"].as_str() {
                    if hash.len() >= 2 {
                        let prefix = &hash[..2];
                        let target = objects_dir.join(prefix).join(hash);
                        if !target.exists() {
                            let url = format!(
                                "https://resources.download.minecraft.net/{}/{}",
                                prefix, hash
                            );
                            asset_downloads.push((url, target));
                        }
                    }
                }
            }
        }

        if !asset_downloads.is_empty() {
            let _ = app.emit(
                "download-progress",
                ProgressPayload {
                    percent: 83,
                    status: format!("Загрузка ресурсов игры ({} файлов)...", asset_downloads.len()),
                    log: Some(format!("Скачивание {} отсутствующих ассетов...", asset_downloads.len())),
                },
            );
            let results: Vec<Result<(), String>> = stream::iter(asset_downloads)
                .map(|(url, target)| async move { download_file(client, &url, &target, "").await })
                .buffer_unordered(24)
                .collect()
                .await;
            for res in results {
                res?;
            }
        }
    }

    Ok(version_json)
}

fn parse_java_major_version(version_output: &str) -> Option<u32> {
    // Examples: openjdk version "21.0.5" 2024-10-15 | java version "1.8.0_402" | openjdk version "21"
    let start = version_output.find('"')? + 1;
    let rest = &version_output[start..];
    let end = rest.find('"')?;
    let ver_str = &rest[..end];
    let mut parts = ver_str.split(['.', '_', '-']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

async fn check_java_version(java_bin: &Path) -> Option<u32> {
    let mut cmd = Command::new(java_bin);
    cmd.arg("-version");
    apply_no_window(&mut cmd);
    let output = cmd.output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_java_major_version(&combined)
}

fn collect_java_candidates(game_dir: &Path, required_major: u32) -> Vec<PathBuf> {
    let java_exe = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates = Vec::new();

    // 1. Управляемая Java лаунчера (~/.astralforge/runtime/java-{major})
    candidates.push(
        game_dir
            .join("runtime")
            .join(format!("java-{}", required_major))
            .join("bin")
            .join(java_exe),
    );

    // 2. JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join(java_exe));
    }

    // 3. Системная java в PATH
    candidates.push(PathBuf::from(java_exe));

    // 4. Стандартные директории JVM на Linux / macOS / Windows
    let mut search_roots = Vec::new();
    if cfg!(target_os = "linux") {
        search_roots.push(PathBuf::from("/usr/lib/jvm"));
        search_roots.push(PathBuf::from("/usr/lib64/jvm"));
        search_roots.push(PathBuf::from("/opt/java"));
        if let Some(home) = dirs::home_dir() {
            search_roots.push(home.join(".jdks"));
            search_roots.push(home.join(".sdkman/candidates/java"));
        }
    } else if cfg!(target_os = "macos") {
        search_roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    } else if cfg!(windows) {
        search_roots.push(PathBuf::from("C:\\Program Files\\Java"));
        search_roots.push(PathBuf::from("C:\\Program Files\\Eclipse Adoptium"));
        search_roots.push(PathBuf::from("C:\\Program Files\\Microsoft"));
        if let Some(local_app_data) = dirs::data_local_dir() {
            let programs = local_app_data.join("Programs");
            search_roots.push(programs.join("Eclipse Adoptium"));
            search_roots.push(programs.join("Java"));
            search_roots.push(programs.join("Microsoft"));
        }
    }

    for root in search_roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    candidates.push(p.join("bin").join(java_exe));
                    candidates.push(p.join("Contents/Home/bin").join(java_exe));
                }
            }
        }
    }

    candidates
}

/// Автоматически находит подходящую Java (>= required_major) или скачивает Eclipse Temurin
pub async fn resolve_or_install_java(
    app: &AppHandle,
    client: &reqwest::Client,
    game_dir: &Path,
    configured_java: &str,
    required_major: u32,
) -> Result<String, String> {
    let trimmed = configured_java.trim();
    if !trimmed.is_empty() && trimmed != "java" && trimmed != "auto" {
        let custom_path = PathBuf::from(trimmed);
        if let Some(ver) = check_java_version(&custom_path).await {
            let _ = app.emit(
                "download-progress",
                ProgressPayload {
                    percent: 85,
                    status: format!("Используется указанная Java {}...", ver),
                    log: Some(format!("Найдена пользовательская Java {} ({})", ver, trimmed)),
                },
            );
            return Ok(trimmed.to_string());
        }
        return Err(format!(
            "Указанный путь к Java не работает: {}. Укажите верный путь или оставьте 'java' для автопоиска.",
            trimmed
        ));
    }

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 84,
            status: format!("Поиск Java {}+ в системе...", required_major),
            log: Some(format!("Требуемая версия Java для сборки: {}+", required_major)),
        },
    );

    let mut fallback_candidate: Option<(u32, String)> = None;
    for candidate in collect_java_candidates(game_dir, required_major) {
        if let Some(ver) = check_java_version(&candidate).await {
            let path_str = candidate.to_string_lossy().to_string();
            if ver == required_major {
                let _ = app.emit(
                    "download-progress",
                    ProgressPayload {
                        percent: 86,
                        status: format!("Найдена Java {}!", ver),
                        log: Some(format!("Найдена подходящая Java {}: {}", ver, path_str)),
                    },
                );
                return Ok(path_str);
            }
            if ver > required_major && fallback_candidate.is_none() {
                fallback_candidate = Some((ver, path_str));
            }
        }
    }

    if let Some((ver, path_str)) = fallback_candidate {
        let _ = app.emit(
            "download-progress",
            ProgressPayload {
                percent: 86,
                status: format!("Найдена Java {}!", ver),
                log: Some(format!("Найдена совместимая Java {}: {}", ver, path_str)),
            },
        );
        return Ok(path_str);
    }

    // Если подходящей Java нет — скачиваем Eclipse Temurin с Adoptium
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };

    let download_url = format!(
        "https://api.adoptium.net/v3/binary/latest/{}/ga/{}/{}/jdk/hotspot/normal/eclipse",
        required_major, os, arch
    );

    let runtime_dir = game_dir.join("runtime");
    let target_java_dir = runtime_dir.join(format!("java-{}", required_major));
    std::fs::create_dir_all(&target_java_dir).map_err(|e| e.to_string())?;

    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive_path = runtime_dir.join(format!("temurin-{}.{}", required_major, ext));

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 85,
            status: format!("Java {} не найдена. Скачивание Eclipse Temurin {}...", required_major, required_major),
            log: Some(format!("Скачивание Java {} с Adoptium: {}", required_major, download_url)),
        },
    );

    download_file(client, &download_url, &archive_path, "").await?;

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 86,
            status: format!("Распаковка Java {}...", required_major),
            log: Some("Распаковка архива Java...".into()),
        },
    );

    let mut extract_cmd = Command::new("tar");
    if cfg!(windows) {
        extract_cmd.arg("-xf");
    } else {
        extract_cmd.arg("-xzf");
    }
    extract_cmd
        .arg(&archive_path)
        .arg("-C")
        .arg(&target_java_dir)
        .arg("--strip-components=1");
    apply_no_window(&mut extract_cmd);
    let extract_status = extract_cmd
        .status()
        .await
        .map_err(|e| format!("Не удалось распаковать Java: {}", e))?;

    let _ = std::fs::remove_file(&archive_path);

    if !extract_status.success() {
        return Err("Ошибка при распаковке архива Java.".into());
    }

    let java_exe = if cfg!(windows) { "java.exe" } else { "java" };
    let installed_java = target_java_dir.join("bin").join(java_exe);
    if !installed_java.exists() {
        return Err(format!(
            "После распаковки не найден исполняемый файл Java: {}",
            installed_java.display()
        ));
    }

    Ok(installed_java.to_string_lossy().to_string())
}

fn loader_profile_name(loader_type: &str, mc_version: &str, loader_version: &str) -> String {
    if loader_type.eq_ignore_ascii_case("fabric") {
        format!("fabric-loader-{}-{}", loader_version, mc_version)
    } else {
        format!("neoforge-{}", loader_version)
    }
}

/// Устанавливает Fabric Loader (профиль и библиотеки) и возвращает путь к подходящей Java
async fn ensure_fabric_installed(
    app: &AppHandle,
    client: &reqwest::Client,
    game_dir: &Path,
    mc_version: &str,
    fabric_version: &str,
    installer_url: Option<&str>,
    resolved_java: String,
) -> Result<String, String> {
    let profile_name = loader_profile_name("fabric", mc_version, fabric_version);
    let version_dir = game_dir.join("versions").join(&profile_name);
    let version_json_path = version_dir.join(format!("{}.json", profile_name));
    std::fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;

    if !version_json_path.exists() {
        let profile_url = installer_url
            .filter(|u| !u.trim().is_empty())
            .map(|u| u.to_string())
            .unwrap_or_else(|| {
                format!(
                    "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
                    mc_version, fabric_version
                )
            });

        let _ = app.emit(
            "download-progress",
            ProgressPayload {
                percent: 87,
                status: format!("Получение профиля Fabric {}...", fabric_version),
                log: Some(format!("Загрузка метаданных Fabric: {}", profile_url)),
            },
        );
        download_file(client, &profile_url, &version_json_path, "").await?;
    }

    let json_text = std::fs::read_to_string(&version_json_path).map_err(|e| e.to_string())?;
    let fabric_json: Value = serde_json::from_str(&json_text).map_err(|e| e.to_string())?;

    let libraries_dir = game_dir.join("libraries");
    let mut lib_downloads = Vec::new();
    if let Some(libs) = fabric_json["libraries"].as_array() {
        for lib in libs {
            if let Some(name) = lib["name"].as_str() {
                if let Some(rel_path) = maven_coords_to_path(name) {
                    let target = libraries_dir.join(&rel_path);
                    if !target.exists() {
                        let base_url = lib["url"]
                            .as_str()
                            .unwrap_or("https://maven.fabricmc.net/");
                        let normalized_base = if base_url.ends_with('/') {
                            base_url.to_string()
                        } else {
                            format!("{}/", base_url)
                        };
                        lib_downloads.push((normalized_base, rel_path, target));
                    }
                }
            }
        }
    }

    if !lib_downloads.is_empty() {
        let _ = app.emit(
            "download-progress",
            ProgressPayload {
                percent: 90,
                status: format!("Скачивание библиотек Fabric ({} шт.)...", lib_downloads.len()),
                log: Some(format!("Загрузка {} библиотек Fabric...", lib_downloads.len())),
            },
        );
        let results: Vec<Result<(), String>> = stream::iter(lib_downloads)
            .map(|(base_url, rel_path, target)| async move {
                let primary_url = format!("{}{}", base_url, rel_path);
                if download_file(client, &primary_url, &target, "").await.is_ok() {
                    return Ok(());
                }
                let mc_fallback = format!("https://libraries.minecraft.net/{}", rel_path);
                if download_file(client, &mc_fallback, &target, "").await.is_ok() {
                    return Ok(());
                }
                let maven_fallback = format!("https://repo1.maven.org/maven2/{}", rel_path);
                download_file(client, &maven_fallback, &target, "").await
            })
            .buffer_unordered(12)
            .collect()
            .await;
        for res in results {
            res?;
        }
    }

    Ok(resolved_java)
}

/// Устанавливает выбранный модлоадер (Fabric или NeoForge) и возвращает путь к подходящей Java
pub async fn ensure_loader_installed(
    app: &AppHandle,
    client: &reqwest::Client,
    game_dir: &Path,
    java_path: &str,
    manifest: &Manifest,
) -> Result<String, String> {
    // 1. Убеждаемся, что ванильный клиент, библиотеки и ассеты готовы
    let mc_json = ensure_vanilla_installed(app, client, game_dir, &manifest.minecraft.version).await?;
    let required_java = mc_json["javaVersion"]["majorVersion"]
        .as_u64()
        .unwrap_or(21) as u32;

    // 2. Автопоиск или скачивание нужной версии Java
    let resolved_java =
        resolve_or_install_java(app, client, game_dir, java_path, required_java).await?;

    let loader_type = manifest.loader.loader_type.to_lowercase();
    if loader_type == "fabric" {
        return ensure_fabric_installed(
            app,
            client,
            game_dir,
            &manifest.minecraft.version,
            &manifest.loader.version,
            manifest.loader.installer_url.as_deref(),
            resolved_java,
        )
        .await;
    }

    if loader_type != "neoforge" {
        return Err(format!("Неподдерживаемый тип загрузчика: {}", manifest.loader.loader_type));
    }

    let neoforge_version = &manifest.loader.version;
    let version_dir = game_dir
        .join("versions")
        .join(format!("neoforge-{}", neoforge_version));
    let version_json = version_dir.join(format!("neoforge-{}.json", neoforge_version));

    if version_json.exists() {
        return Ok(resolved_java);
    }

    // NeoForge installer требует наличия launcher_profiles.json в целевой папке
    let profiles_path = game_dir.join("launcher_profiles.json");
    if !profiles_path.exists() {
        std::fs::write(&profiles_path, r#"{"profiles":{}}"#).map_err(|e| e.to_string())?;
    }

    let installer_url = manifest.loader.installer_url.clone().unwrap_or_else(|| {
        format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            neoforge_version, neoforge_version
        )
    });

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 87,
            status: format!("Скачивание установщика NeoForge {}...", neoforge_version),
            log: Some("Скачивание установщика NeoForge с официального maven...".into()),
        },
    );

    let installer_path = game_dir.join("neoforge-installer.jar");
    download_file(client, &installer_url, &installer_path, "").await?;

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 90,
            status: "Установка клиента NeoForge...".into(),
            log: Some("Запуск фоновой установки: neoforge-installer --install-client...".into()),
        },
    );

    let game_dir_str = game_dir.to_string_lossy().to_string();
    let mut installer_cmd = tokio::process::Command::new(&resolved_java);
    installer_cmd
        .arg("-jar")
        .arg(&installer_path)
        .arg("--install-client")
        .arg(&game_dir_str)
        .current_dir(game_dir);
    apply_no_window(&mut installer_cmd);
    let output = installer_cmd
        .output()
        .await
        .map_err(|e| format!("Не удалось запустить Java для установки NeoForge: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "Установщик NeoForge завершился с ошибкой ({}).\n{}\n{}",
            output.status, stdout, stderr
        ));
    }

    let _ = std::fs::remove_file(&installer_path);
    Ok(resolved_java)
}

async fn download_authlib_from_meta(
    client: &reqwest::Client,
    meta_url: &str,
    target: &Path,
) -> Result<(), String> {
    let resp = client
        .get(meta_url)
        .header("User-Agent", "AstralForge-Launcher/1.0")
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса к {}: {}", meta_url, e))?;

    if !resp.status().is_success() {
        return Err(format!("Ошибка ответа {}: статус {}", meta_url, resp.status()));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| format!("Ошибка разбора JSON {}: {}", meta_url, e))?;

    let download_url = json["download_url"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("В ответе {} отсутствует download_url", meta_url))?;
    let sha256 = json["checksums"]["sha256"].as_str().unwrap_or("");

    download_file(client, download_url, target, sha256).await
}

async fn download_authlib_from_github(
    client: &reqwest::Client,
    target: &Path,
) -> Result<(), String> {
    let resp = client
        .get(AUTHLIB_INJECTOR_GITHUB_API_URL)
        .header("User-Agent", "AstralForge-Launcher/1.0")
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса к {}: {}", AUTHLIB_INJECTOR_GITHUB_API_URL, e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Ошибка ответа {}: статус {}",
            AUTHLIB_INJECTOR_GITHUB_API_URL,
            resp.status()
        ));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| format!("Ошибка разбора GitHub Release JSON: {}", e))?;

    let asset = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find(|a| {
                a["name"]
                    .as_str()
                    .map_or(false, |name| name.ends_with(".jar"))
            })
        })
        .ok_or_else(|| "В последнем релизе authlib-injector на GitHub не найден .jar".to_string())?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Отсутствует browser_download_url в релизе GitHub".to_string())?;
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .unwrap_or("");

    download_file(client, download_url, target, sha256).await
}

/// Обеспечивает наличие authlib-injector для поддержки скинов и входа Ely.by
pub async fn ensure_authlib_injector(
    client: &reqwest::Client,
    game_dir: &Path,
) -> Result<PathBuf, String> {
    let target = game_dir.join("authlib-injector.jar");
    if target.exists() && target.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(target);
    }

    let mut errors = Vec::new();
    for meta_url in AUTHLIB_INJECTOR_META_URLS {
        match download_authlib_from_meta(client, meta_url, &target).await {
            Ok(()) => return Ok(target),
            Err(err) => errors.push(err),
        }
    }

    match download_authlib_from_github(client, &target).await {
        Ok(()) => Ok(target),
        Err(err) => {
            errors.push(err);
            Err(format!(
                "Не удалось скачать authlib-injector ни с одного источника:\n{}",
                errors.join("\n")
            ))
        }
    }
}

fn maven_coords_to_path(coords: &str) -> Option<String> {
    // Format: group:artifact:version[:classifier][@ext]
    let (base, ext) = match coords.split_once('@') {
        Some((b, e)) => (b, e),
        None => (coords, "jar"),
    };
    let parts: Vec<&str> = base.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group_path = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let filename = if parts.len() >= 4 && !parts[3].is_empty() {
        format!("{}-{}-{}.{}", artifact, version, parts[3], ext)
    } else {
        format!("{}-{}.{}", artifact, version, ext)
    };
    Some(format!("{}/{}/{}/{}", group_path, artifact, version, filename))
}

fn collect_libraries_from_json(
    version_json: &Value,
    libraries_dir: &Path,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    if let Some(libs) = version_json["libraries"].as_array() {
        for lib in libs {
            if !rules_allow(lib.get("rules").and_then(|r| r.as_array())) {
                continue;
            }
            let rel_path = lib["downloads"]["artifact"]["path"]
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| lib["name"].as_str().and_then(maven_coords_to_path));

            if let Some(rel) = rel_path {
                let full = libraries_dir.join(rel);
                if full.exists() && seen.insert(full.clone()) {
                    out.push(full);
                }
            }
        }
    }
}

fn substitute_vars(input: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = input.to_string();
    for (k, v) in vars {
        let placeholder = format!("${{{}}}", k);
        if result.contains(&placeholder) {
            result = result.replace(&placeholder, v);
        }
    }
    result
}

fn collect_arguments(args_arr: Option<&Vec<Value>>, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut result = Vec::new();
    let Some(arr) = args_arr else {
        return result;
    };
    for item in arr {
        if let Some(s) = item.as_str() {
            result.push(substitute_vars(s, vars));
        } else if let Some(obj) = item.as_object() {
            if !rules_allow(obj.get("rules").and_then(|r| r.as_array())) {
                continue;
            }
            if let Some(val) = obj.get("value") {
                if let Some(s) = val.as_str() {
                    result.push(substitute_vars(s, vars));
                } else if let Some(list) = val.as_array() {
                    for entry in list {
                        if let Some(s) = entry.as_str() {
                            result.push(substitute_vars(s, vars));
                        }
                    }
                }
            }
        }
    }
    result
}

/// Запуск процесса Minecraft (Fabric / NeoForge)
#[allow(clippy::too_many_arguments)]
pub async fn launch_minecraft(
    app: &AppHandle,
    running_state: RunningGameState,
    game_dir: &Path,
    java_path: &str,
    ram_gb: u32,
    nickname: &str,
    auth_type: &str,
    uuid: &str,
    access_token: &str,
    server_ip: &str,
    mc_version: &str,
    loader_type: &str,
    loader_ver: &str,
    authlib_path: Option<&Path>,
) -> Result<(), String> {
    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 95,
            status: "Формирование аргументов запуска...".into(),
            log: Some(format!("Подготовка параметров запуска для игрока: {}", nickname)),
        },
    );

    let loader_ver_name = loader_profile_name(loader_type, mc_version, loader_ver);
    let loader_json_path = game_dir
        .join("versions")
        .join(&loader_ver_name)
        .join(format!("{}.json", loader_ver_name));
    let mc_json_path = game_dir
        .join("versions")
        .join(mc_version)
        .join(format!("{}.json", mc_version));

    let loader_json: Value = serde_json::from_str(
        &std::fs::read_to_string(&loader_json_path)
            .map_err(|e| format!("Не найден профиль загрузчика {}: {}", loader_json_path.display(), e))?,
    )
    .map_err(|e| e.to_string())?;

    let mc_json: Value = serde_json::from_str(
        &std::fs::read_to_string(&mc_json_path)
            .map_err(|e| format!("Не найден профиль Minecraft {}: {}", mc_json_path.display(), e))?,
    )
    .map_err(|e| e.to_string())?;

    let libraries_dir = game_dir.join("libraries");
    let mut seen_jars = HashSet::new();
    let mut classpath_entries = Vec::new();
    collect_libraries_from_json(&loader_json, &libraries_dir, &mut seen_jars, &mut classpath_entries);
    collect_libraries_from_json(&mc_json, &libraries_dir, &mut seen_jars, &mut classpath_entries);

    let client_jar = game_dir
        .join("versions")
        .join(mc_version)
        .join(format!("{}.jar", mc_version));
    if client_jar.exists() && seen_jars.insert(client_jar.clone()) {
        classpath_entries.push(client_jar);
    }

    let classpath_sep = if cfg!(windows) { ";" } else { ":" };
    let classpath_str = classpath_entries
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(classpath_sep);

    let natives_dir = game_dir
        .join("versions")
        .join(&loader_ver_name)
        .join("natives");
    let _ = std::fs::create_dir_all(natives_dir.join("java"));
    let _ = std::fs::create_dir_all(natives_dir.join("jna"));
    let _ = std::fs::create_dir_all(natives_dir.join("lwjgl"));
    let _ = std::fs::create_dir_all(natives_dir.join("netty"));

    let asset_index_id = mc_json["assetIndex"]["id"]
        .as_str()
        .unwrap_or("17")
        .to_string();

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("natives_directory", natives_dir.to_string_lossy().to_string());
    vars.insert("launcher_name", "Eternum".to_string());
    vars.insert("launcher_version", "1.0.0".to_string());
    vars.insert("classpath", classpath_str.clone());
    vars.insert("library_directory", libraries_dir.to_string_lossy().to_string());
    vars.insert("classpath_separator", classpath_sep.to_string());
    vars.insert("version_name", loader_ver_name.clone());
    vars.insert("auth_player_name", nickname.to_string());
    vars.insert("game_directory", game_dir.to_string_lossy().to_string());
    vars.insert("assets_root", game_dir.join("assets").to_string_lossy().to_string());
    vars.insert("assets_index_name", asset_index_id);
    vars.insert("auth_uuid", uuid.to_string());
    vars.insert("auth_access_token", access_token.to_string());
    vars.insert("clientid", "astralforge1".to_string());
    vars.insert("auth_xuid", "0".to_string());
    vars.insert(
        "user_type",
        if auth_type == "elyby" { "mojang" } else { "legacy" }.to_string(),
    );
    vars.insert("version_type", "release".to_string());

    let mut jvm_args = Vec::new();
    jvm_args.push(format!("-Xms{}G", (ram_gb / 2).max(1)));
    jvm_args.push(format!("-Xmx{}G", ram_gb));

    if auth_type == "elyby" {
        if let Some(agent_path) = authlib_path {
            jvm_args.push(format!(
                "-javaagent:{}={}",
                agent_path.to_string_lossy(),
                ELYBY_AUTH_SERVER
            ));
        }
    }

    let mut mc_jvm = collect_arguments(mc_json["arguments"]["jvm"].as_array(), &vars);
    let mut loader_jvm = collect_arguments(loader_json["arguments"]["jvm"].as_array(), &vars);
    jvm_args.append(&mut mc_jvm);
    jvm_args.append(&mut loader_jvm);

    let default_main = if loader_type.eq_ignore_ascii_case("fabric") {
        "net.fabricmc.loader.impl.launch.knot.KnotClient"
    } else {
        "cpw.mods.bootstraplauncher.BootstrapLauncher"
    };
    let main_class = loader_json["mainClass"]
        .as_str()
        .unwrap_or(default_main);

    let mut game_args = collect_arguments(mc_json["arguments"]["game"].as_array(), &vars);
    let mut loader_game = collect_arguments(loader_json["arguments"]["game"].as_array(), &vars);
    game_args.append(&mut loader_game);

    if !server_ip.is_empty() {
        game_args.push("--quickPlayMultiplayer".to_string());
        game_args.push(server_ip.to_string());
    }

    let mut cmd = Command::new(java_path);
    cmd.current_dir(game_dir);
    cmd.args(&jvm_args);
    cmd.arg(main_class);
    cmd.args(&game_args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    apply_no_window(&mut cmd);

    let _ = app.emit(
        "download-progress",
        ProgressPayload {
            percent: 100,
            status: format!("Minecraft {} запущен!", mc_version),
            log: Some("Процесс игры успешно стартовал!".into()),
        },
    );

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Не удалось запустить процесс Minecraft: {}", e))?;

    let (kill_tx, kill_rx) = oneshot::channel();
    let game_id = running_state.set_running(kill_tx);
    let _ = app.emit("game-started", ());

    if let Some(stdout) = child.stdout.take() {
        let app_handle = app.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = app_handle.emit("game-log", line);
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let app_handle = app.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = app_handle.emit("game-log", format!("[STDERR] {}", line));
            }
        });
    }

    let app_handle = app.clone();
    tokio::spawn(async move {
        tokio::select! {
            wait_res = child.wait() => {
                match wait_res {
                    Ok(status) => {
                        let _ = app_handle.emit("game-log", format!("Процесс игры завершился ({})", status));
                    }
                    Err(err) => {
                        let _ = app_handle.emit("game-log", format!("Ошибка ожидания процесса игры: {}", err));
                    }
                }
            }
            _ = kill_rx => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = app_handle.emit("game-log", "Процесс игры остановлен пользователем.");
            }
        }
        running_state.clear_if_matches(game_id);
        let _ = app_handle.emit("game-stopped", ());
    });

    Ok(())
}
