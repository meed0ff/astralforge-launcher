use crate::manifest::ManifestFile;
use crate::ui_bridge::UiHandle;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

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

#[derive(Clone, Serialize)]
pub struct DownloadStats {
    pub file_name: Option<String>,
    pub file_downloaded: u64,
    pub file_total: u64,
    pub total_downloaded: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
}

#[derive(Clone)]
pub struct DownloadTracker {
    inner: std::sync::Arc<std::sync::Mutex<DownloadTrackerState>>,
}

struct DownloadTrackerState {
    total_bytes: u64,
    total_downloaded: u64,
    window_bytes: u64,
    window_start: Instant,
    last_emit: Instant,
    smooth_speed_bps: u64,
    current_file: String,
    current_file_downloaded: u64,
    current_file_total: u64,
}

impl DownloadTracker {
    pub fn new(total_bytes: u64) -> Self {
        let now = Instant::now();
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(DownloadTrackerState {
                total_bytes,
                total_downloaded: 0,
                window_bytes: 0,
                window_start: now,
                last_emit: now.checked_sub(Duration::from_secs(1)).unwrap_or(now),
                smooth_speed_bps: 0,
                current_file: String::new(),
                current_file_downloaded: 0,
                current_file_total: 0,
            })),
        }
    }

    pub fn add_to_total(&self, extra_bytes: u64) {
        if let Ok(mut st) = self.inner.lock() {
            st.total_bytes = st.total_bytes.saturating_add(extra_bytes);
        }
    }

    pub fn emit_snapshot(&self, app: &UiHandle, force: bool) {
        let payload = {
            let Ok(mut st) = self.inner.lock() else {
                return;
            };
            if !force && st.last_emit.elapsed() < Duration::from_millis(120) {
                return;
            }
            let dt = st.window_start.elapsed().as_secs_f64().max(0.001);
            if st.window_bytes > 0 {
                let inst_speed = (st.window_bytes as f64 / dt) as u64;
                st.smooth_speed_bps = if st.smooth_speed_bps == 0 {
                    inst_speed
                } else {
                    ((st.smooth_speed_bps as f64 * 0.6) + (inst_speed as f64 * 0.4)) as u64
                };
            }
            st.last_emit = Instant::now();
            if st.window_start.elapsed() >= Duration::from_millis(500) {
                st.window_bytes = 0;
                st.window_start = Instant::now();
            }
            DownloadStats {
                file_name: if st.current_file.is_empty() {
                    None
                } else {
                    Some(st.current_file.clone())
                },
                file_downloaded: st.current_file_downloaded,
                file_total: st.current_file_total.max(st.current_file_downloaded),
                total_downloaded: st.total_downloaded,
                total_bytes: st.total_bytes.max(st.total_downloaded),
                speed_bps: st.smooth_speed_bps,
            }
        };
        let _ = app.emit("download-stats", payload);
    }

    pub fn on_chunk(
        &self,
        app: &UiHandle,
        file_name: &str,
        chunk_len: u64,
        file_downloaded: u64,
        file_total: u64,
    ) {
        if let Ok(mut st) = self.inner.lock() {
            st.total_downloaded = st.total_downloaded.saturating_add(chunk_len);
            st.window_bytes = st.window_bytes.saturating_add(chunk_len);
            st.current_file = file_name.to_string();
            st.current_file_downloaded = file_downloaded;
            st.current_file_total = file_total;
        }
        self.emit_snapshot(app, false);
    }
}

pub fn get_available_disk_space(path: &Path) -> Option<u64> {
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

pub fn format_bytes_ru(bytes: u64) -> String {
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

pub fn estimate_sync_sizes(game_dir: &Path, files: &[ManifestFile]) -> (u64, u64) {
    let mut total_pack_bytes: u64 = 0;
    let mut needed_bytes: u64 = 0;

    for file_entry in files {
        total_pack_bytes = total_pack_bytes.saturating_add(file_entry.size);
        if let Some(extract_rel) = &file_entry.extract_to {
            let Ok(dest_dir) = safe_join_rel(game_dir, extract_rel) else {
                needed_bytes = needed_bytes.saturating_add(file_entry.size);
                continue;
            };
            let marker = bundle_marker_path(game_dir, &file_entry.path);
            let is_synced = dest_dir.exists()
                && fs::read_to_string(&marker)
                    .map(|s| s.trim().eq_ignore_ascii_case(&file_entry.sha256))
                    .unwrap_or(false);
            if !is_synced {
                // Архив + распаковка требуют ~2x места на время установки
                needed_bytes = needed_bytes.saturating_add(file_entry.size.saturating_mul(2));
            }
            continue;
        }

        let Ok(local_path) = safe_join_rel(game_dir, &file_entry.path) else {
            needed_bytes = needed_bytes.saturating_add(file_entry.size);
            continue;
        };
        let is_matching = if local_path.is_file() {
            if file_entry.size > 0
                && fs::metadata(&local_path).map(|m| m.len()).unwrap_or(u64::MAX) != file_entry.size
            {
                false
            } else {
                calculate_sha256(&local_path)
                    .map(|sha| sha.eq_ignore_ascii_case(&file_entry.sha256))
                    .unwrap_or(false)
            }
        } else {
            false
        };
        if !is_matching {
            needed_bytes = needed_bytes.saturating_add(file_entry.size);
        }
    }

    (total_pack_bytes, needed_bytes)
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
    let mut speed = 0u64;
    download_file_with_progress(
        client,
        url,
        target_path,
        expected_sha256,
        None,
        "",
        0,
        0,
        0,
        &mut speed,
    )
    .await
    .map(|_| ())
}

pub async fn download_file_with_progress(
    client: &reqwest::Client,
    url: &str,
    target_path: &Path,
    expected_sha256: &str,
    app: Option<&UiHandle>,
    file_label: &str,
    known_size: u64,
    total_downloaded_so_far: u64,
    total_bytes: u64,
    smooth_speed_bps: &mut u64,
) -> Result<u64, String> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp_path = target_path.with_extension("download_tmp");

    let resp = client
        .get(url)
        .header("User-Agent", "Eternum-Launcher/1.0")
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса к {}: {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!("Не удалось скачать {}: статус {}", url, resp.status()));
    }

    let file_total = resp.content_length().unwrap_or(known_size);
    let mut stream = resp.bytes_stream();
    let mut out_file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut file_downloaded: u64 = 0;
    let mut window_start = Instant::now();
    let mut window_bytes: u64 = 0;
    let mut last_emit = Instant::now();

    if let Some(app_h) = app {
        let _ = app_h.emit(
            "download-stats",
            DownloadStats {
                file_name: Some(file_label.to_string()),
                file_downloaded: 0,
                file_total,
                total_downloaded: total_downloaded_so_far,
                total_bytes,
                speed_bps: *smooth_speed_bps,
            },
        );
    }

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| e.to_string())?;
        let len = chunk.len() as u64;
        tokio::io::AsyncWriteExt::write_all(&mut out_file, &chunk)
            .await
            .map_err(|e| e.to_string())?;
        file_downloaded += len;
        window_bytes += len;

        if let Some(app_h) = app {
            if last_emit.elapsed() >= Duration::from_millis(120) {
                let dt = window_start.elapsed().as_secs_f64().max(0.001);
                let inst_speed = (window_bytes as f64 / dt) as u64;
                *smooth_speed_bps = if *smooth_speed_bps == 0 {
                    inst_speed
                } else {
                    ((*smooth_speed_bps as f64 * 0.6) + (inst_speed as f64 * 0.4)) as u64
                };
                let _ = app_h.emit(
                    "download-stats",
                    DownloadStats {
                        file_name: Some(file_label.to_string()),
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
    }
    tokio::io::AsyncWriteExt::flush(&mut out_file)
        .await
        .map_err(|e| e.to_string())?;
    drop(out_file);

    if let Some(app_h) = app {
        if window_bytes > 0 {
            let dt = window_start.elapsed().as_secs_f64().max(0.001);
            let inst_speed = (window_bytes as f64 / dt) as u64;
            *smooth_speed_bps = if *smooth_speed_bps == 0 {
                inst_speed
            } else {
                ((*smooth_speed_bps as f64 * 0.6) + (inst_speed as f64 * 0.4)) as u64
            };
        }
        let _ = app_h.emit(
            "download-stats",
            DownloadStats {
                file_name: Some(file_label.to_string()),
                file_downloaded,
                file_total: file_total.max(file_downloaded),
                total_downloaded: total_downloaded_so_far + file_downloaded,
                total_bytes: total_bytes.max(total_downloaded_so_far + file_downloaded),
                speed_bps: *smooth_speed_bps,
            },
        );
    }

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
    Ok(file_downloaded)
}

pub async fn download_file_tracked(
    app: &UiHandle,
    tracker: &DownloadTracker,
    client: &reqwest::Client,
    url: &str,
    target_path: &Path,
    expected_sha256: &str,
    known_size: u64,
) -> Result<u64, String> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp_path = target_path.with_extension("download_tmp");
    let resp = client
        .get(url)
        .header("User-Agent", "Eternum-Launcher/1.0")
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса к {}: {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!("Не удалось скачать {}: статус {}", url, resp.status()));
    }

    let file_total = resp.content_length().unwrap_or(known_size);
    if known_size == 0 && file_total > 0 {
        tracker.add_to_total(file_total);
    }

    let file_label = target_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| url.to_string());

    let mut stream = resp.bytes_stream();
    let mut out_file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut file_downloaded: u64 = 0;
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| e.to_string())?;
        let len = chunk.len() as u64;
        tokio::io::AsyncWriteExt::write_all(&mut out_file, &chunk)
            .await
            .map_err(|e| e.to_string())?;
        file_downloaded += len;
        tracker.on_chunk(app, &file_label, len, file_downloaded, file_total);
    }
    tokio::io::AsyncWriteExt::flush(&mut out_file)
        .await
        .map_err(|e| e.to_string())?;
    drop(out_file);

    tracker.emit_snapshot(app, true);

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
    Ok(file_downloaded)
}

pub fn resolve_mirror_urls(url: &str, mirror_source: &str) -> (String, Option<String>) {
    let trimmed = url.trim();
    let use_gitlab = mirror_source.eq_ignore_ascii_case("gitlab");

    if let Some(rest) = trimmed.strip_prefix("https://raw.githubusercontent.com/") {
        let mut parts = rest.splitn(4, '/');
        if let (Some(owner), Some(repo), Some(branch), Some(path)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        {
            let gitlab_url = format!("https://gitlab.com/{}/{}/-/raw/{}/{}", owner, repo, branch, path);
            return if use_gitlab {
                (gitlab_url, Some(trimmed.to_string()))
            } else {
                (trimmed.to_string(), Some(gitlab_url))
            };
        }
    } else if let Some(rest) = trimmed.strip_prefix("https://gitlab.com/") {
        if let Some((repo_path, raw_rest)) = rest.split_once("/-/raw/") {
            let github_url = format!("https://raw.githubusercontent.com/{}/{}", repo_path, raw_rest);
            return if use_gitlab {
                (trimmed.to_string(), Some(github_url))
            } else {
                (github_url, Some(trimmed.to_string()))
            };
        }
    }

    (trimmed.to_string(), None)
}

async fn download_with_mirror(
    client: &reqwest::Client,
    primary_url: &str,
    fallback_url: Option<&str>,
    target_path: &Path,
    expected_sha256: &str,
    app: Option<&UiHandle>,
    display_name: &str,
    known_size: u64,
    total_downloaded_so_far: u64,
    total_bytes: u64,
    smooth_speed_bps: &mut u64,
) -> Result<u64, String> {
    match download_file_with_progress(
        client,
        primary_url,
        target_path,
        expected_sha256,
        app,
        display_name,
        known_size,
        total_downloaded_so_far,
        total_bytes,
        smooth_speed_bps,
    )
    .await
    {
        Ok(bytes) => Ok(bytes),
        Err(primary_err) => {
            if let Some(fb) = fallback_url {
                if let Some(handle) = app {
                    let _ = handle.emit(
                        "download-progress",
                        ProgressPayload {
                            percent: 35,
                            status: "Переключение на резервное зеркало...".into(),
                            log: Some(format!("⚠ Основное зеркало недоступно ({}), пробуем резерв...", primary_err)),
                        },
                    );
                }
                download_file_with_progress(
                    client,
                    fb,
                    target_path,
                    expected_sha256,
                    app,
                    display_name,
                    known_size,
                    total_downloaded_so_far,
                    total_bytes,
                    smooth_speed_bps,
                )
                .await
            } else {
                Err(primary_err)
            }
        }
    }
}

/// Синхронизирует файлы сборки (моды, конфиги, ресурспаки, шейдеры, kubejs и zip-бандлы)
pub async fn sync_files(
    app: &UiHandle,
    client: &reqwest::Client,
    game_dir: &Path,
    files: &[ManifestFile],
    mirror_source: &str,
) -> Result<(), String> {
    let (total_pack_bytes, needed_bytes) = estimate_sync_sizes(game_dir, files);
    if let Some(free_bytes) = get_available_disk_space(game_dir) {
        if free_bytes < needed_bytes {
            return Err(format!(
                "Недостаточно свободного места на диске! Нужно: {}, доступно: {}.",
                format_bytes_ru(needed_bytes),
                format_bytes_ru(free_bytes)
            ));
        }
        let _ = app.emit(
            "download-progress",
            ProgressPayload {
                percent: 20,
                status: format!(
                    "Подготовка загрузки ({} из {})...",
                    format_bytes_ru(needed_bytes),
                    format_bytes_ru(total_pack_bytes)
                ),
                log: Some(format!(
                    "Вес сборки: {}, нужно скачать/места: {}, свободно на диске: {}",
                    format_bytes_ru(total_pack_bytes),
                    format_bytes_ru(needed_bytes),
                    format_bytes_ru(free_bytes)
                )),
            },
        );
    }

    let total = files.len();
    let mut valid_mod_names = std::collections::HashSet::new();
    let mut total_downloaded: u64 = 0;
    let mut smooth_speed_bps: u64 = 0;

    for (idx, file_entry) in files.iter().enumerate() {
        let percent = if needed_bytes > 0 {
            30 + (((total_downloaded as f64 / needed_bytes as f64) * 45.0) as u32).min(45)
        } else {
            30 + ((idx as f32 / total.max(1) as f32) * 45.0) as u32
        };
        let (primary_url, fallback_url) = if file_entry.source == "modrinth" {
            (file_entry.url.clone(), None)
        } else {
            resolve_mirror_urls(&file_entry.url, mirror_source)
        };
        let src_label = if file_entry.source == "modrinth" {
            "Modrinth CDN"
        } else if primary_url.starts_with("https://gitlab.com/") {
            "GitLab"
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
                let dl = download_with_mirror(
                    client,
                    &primary_url,
                    fallback_url.as_deref(),
                    &tmp_zip,
                    &file_entry.sha256,
                    Some(app),
                    &file_entry.path,
                    file_entry.size,
                    total_downloaded,
                    needed_bytes,
                    &mut smooth_speed_bps,
                )
                .await?;
                total_downloaded = total_downloaded.saturating_add(dl);
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

            let dl = download_with_mirror(
                client,
                &primary_url,
                fallback_url.as_deref(),
                &local_path,
                &file_entry.sha256,
                Some(app),
                &file_entry.path,
                file_entry.size,
                total_downloaded,
                needed_bytes,
                &mut smooth_speed_bps,
            )
            .await?;
            total_downloaded = total_downloaded.saturating_add(dl);
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
