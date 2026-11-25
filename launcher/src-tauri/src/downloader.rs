use crate::manifest::ManifestFile;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub percent: u32,
    pub status: String,
    pub log: Option<String>,
}

pub fn calculate_sha256(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(hex::encode(hasher.finalize()))
}

/// Скачивает файл с прогрессом
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    target_path: &Path,
    expected_sha256: &str,
) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp_path = target_path.with_extension("download_tmp");

    let resp = client
        .get(url)
        .header("User-Agent", "AstralForge-Launcher/1.0")
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса к {}: {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!("Не удалось скачать {}: статус {}", url, resp.status()));
    }

    let mut stream = resp.bytes_stream();
    let mut out_file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| e.to_string())?;
        tokio::io::AsyncWriteExt::write_all(&mut out_file, &chunk)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::io::AsyncWriteExt::flush(&mut out_file)
        .await
        .map_err(|e| e.to_string())?;
    drop(out_file);

    // Валидация хеша
    if !expected_sha256.is_empty() {
        let downloaded_sha = calculate_sha256(&tmp_path)?;
        if !downloaded_sha.eq_ignore_ascii_case(expected_sha256) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!(
                "Несовпадение SHA-256 для {}: ожидалось {}, получено {}",
                target_path.display(),
                expected_sha256,
                downloaded_sha
            ));
        }
    }

    fs::rename(&tmp_path, target_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Синхронизирует файлы сборки (моды и конфиги)
pub async fn sync_files(
    app: &AppHandle,
    client: &reqwest::Client,
    game_dir: &Path,
    files: &[ManifestFile],
) -> Result<(), String> {
    let total = files.len();
    let mut valid_mod_names = std::collections::HashSet::new();

    for (idx, file_entry) in files.iter().enumerate() {
        let local_path = game_dir.join(&file_entry.path);

        if file_entry.path.starts_with("mods/") {
            if let Some(filename) = Path::new(&file_entry.path).file_name() {
                valid_mod_names.insert(filename.to_string_lossy().to_string());
            }
        }

        // Проверяем существующий файл
        let is_matching = if local_path.exists() {
            let local_sha = calculate_sha256(&local_path)?;
            local_sha.eq_ignore_ascii_case(&file_entry.sha256)
        } else {
            false
        };

        let percent = 30 + ((idx as f32 / total.max(1) as f32) * 45.0) as u32;

        if !is_matching {
            let src_label = if file_entry.source == "modrinth" { "Modrinth CDN" } else { "GitHub" };
            let status_msg = format!("Скачивание [{}] {}...", src_label, file_entry.path);

            let _ = app.emit(
                "download-progress",
                ProgressPayload {
                    percent,
                    status: status_msg.clone(),
                    log: Some(status_msg),
                },
            );

            download_file(client, &file_entry.url, &local_path, &file_entry.sha256).await?;
        }
    }

    // Удаляем лишние моды, которых больше нет в манифесте
    let mods_dir = game_dir.join("mods");
    if mods_dir.exists() {
        if let Ok(entries) = fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "jar") {
                    let filename = path.file_name().unwrap().to_string_lossy().to_string();
                    if !valid_mod_names.contains(&filename) {
                        let log_msg = format!("Удален устаревший мод: {}", filename);
                        let _ = app.emit(
                            "download-progress",
                            ProgressPayload {
                                percent: 75,
                                status: "Очистка устаревших модов...".into(),
                                log: Some(log_msg),
                            },
                        );
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
    }

    Ok(())
}
