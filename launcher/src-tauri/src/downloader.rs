use crate::manifest::ManifestFile;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter};

fn safe_join_rel(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let trimmed = rel.trim();
    if trimmed.is_empty() {
        return Ok(base.to_path_buf());
    }
    let rel_path = Path::new(trimmed);
    let mut out = base.to_path_buf();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => {
                return Err(format!("Недопустимый относительный путь в манифесте: {}", rel));
            }
        }
    }
    Ok(out)
}

fn bundle_marker_path(game_dir: &Path, manifest_path: &str) -> PathBuf {
    let safe_name: String = manifest_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    game_dir.join(".bundles").join(format!("{}.sha256", safe_name))
}

fn extract_zip_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let file = fs::File::open(archive_path).map_err(|e| format!("Не удалось открыть архив {}: {}", archive_path.display(), e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Ошибка чтения ZIP {}: {}", archive_path.display(), e))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest_dir.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out_file = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

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

/// Синхронизирует файлы сборки (моды, конфиги, ресурспаки, шейдеры, kubejs и zip-бандлы)
pub async fn sync_files(
    app: &AppHandle,
    client: &reqwest::Client,
    game_dir: &Path,
    files: &[ManifestFile],
) -> Result<(), String> {
    let total = files.len();
    let mut valid_mod_names = std::collections::HashSet::new();

    for (idx, file_entry) in files.iter().enumerate() {
        let percent = 30 + ((idx as f32 / total.max(1) as f32) * 45.0) as u32;
        let src_label = if file_entry.source == "modrinth" {
            "Modrinth CDN"
        } else {
            "GitHub"
        };

        if let Some(extract_rel) = &file_entry.extract_to {
            let dest_dir = safe_join_rel(game_dir, extract_rel)?;
            let marker = bundle_marker_path(game_dir, &file_entry.path);
            let is_synced = dest_dir.exists()
                && fs::read_to_string(&marker)
                    .map(|s| s.trim().eq_ignore_ascii_case(&file_entry.sha256))
                    .unwrap_or(false);

            if !is_synced {
                let status_msg = format!("Скачивание и распаковка [{}] {}...", src_label, file_entry.path);
                let _ = app.emit(
                    "download-progress",
                    ProgressPayload {
                        percent,
                        status: status_msg.clone(),
                        log: Some(status_msg),
                    },
                );

                let tmp_zip = game_dir.join(".bundles").join("bundle.zip.tmp");
                download_file(client, &file_entry.url, &tmp_zip, &file_entry.sha256).await?;
                extract_zip_archive(&tmp_zip, &dest_dir)?;
                let _ = fs::remove_file(&tmp_zip);
                if let Some(parent) = marker.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::write(&marker, &file_entry.sha256).map_err(|e| e.to_string())?;
            }
            continue;
        }

        let local_path = safe_join_rel(game_dir, &file_entry.path)?;

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

        if !is_matching {
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
