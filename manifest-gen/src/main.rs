use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256, Sha512};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

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
struct RepoConfig {
    #[serde(default = "default_repo_owner")]
    repo_owner: String,
    #[serde(default = "default_repo_name")]
    repo_name: String,
    #[serde(default = "default_branch")]
    branch: String,
    #[serde(default = "default_pack_version")]
    pack_version: String,
    #[serde(default = "default_mc_version")]
    minecraft_version: String,
    #[serde(default = "default_loader_type")]
    loader_type: String,
    #[serde(default = "default_loader_version")]
    loader_version: String,
    #[serde(default = "default_server_name")]
    server_name: String,
    #[serde(default = "default_server_ip")]
    server_ip: String,
    #[serde(default)]
    bundles_extract_map: BTreeMap<String, String>,
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

struct CliArgs {
    dir: Option<PathBuf>,
    loader_type: Option<String>,
    loader_version: Option<String>,
    minecraft_version: Option<String>,
    pack_version: Option<String>,
    server_ip: Option<String>,
    server_name: Option<String>,
    non_interactive: bool,
    show_help: bool,
    has_cli_flags: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut out = CliArgs {
        dir: None,
        loader_type: None,
        loader_version: None,
        minecraft_version: None,
        pack_version: None,
        server_ip: None,
        server_name: None,
        non_interactive: false,
        show_help: false,
        has_cli_flags: !args.is_empty(),
    };

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let next_val = |idx: &mut usize, flag: &str| -> Result<String, String> {
            *idx += 1;
            args.get(*idx)
                .cloned()
                .ok_or_else(|| format!("Для флага {} не указано значение", flag))
        };

        match arg {
            "-h" | "--help" => {
                out.show_help = true;
                return Ok(out);
            }
            "-y" | "--yes" | "--non-interactive" => out.non_interactive = true,
            "--dir" => out.dir = Some(PathBuf::from(next_val(&mut i, arg)?)),
            "--loader" | "--core" => out.loader_type = Some(next_val(&mut i, arg)?),
            "--loader-version" | "--core-version" => {
                out.loader_version = Some(next_val(&mut i, arg)?)
            }
            "--mc" | "--minecraft" => out.minecraft_version = Some(next_val(&mut i, arg)?),
            "--pack-version" | "--pack" => out.pack_version = Some(next_val(&mut i, arg)?),
            "--server-ip" => out.server_ip = Some(next_val(&mut i, arg)?),
            "--server-name" => out.server_name = Some(next_val(&mut i, arg)?),
            other => return Err(format!("Неизвестный аргумент: {}", other)),
        }
        i += 1;
    }

    Ok(out)
}

fn print_help() {
    println!(
        "Eternum Manifest Generator v1.0
Использование:
  eternum-manifest-gen [флаги]

Без флагов запускается интерактивный мастер настройки и генерации.

Флаги:
  --dir <путь>             Папка сборки (по умолчанию текущая или рядом с программой)
  --loader <тип>           Ядро: fabric, neoforge, forge и т.д.
  --loader-version <вер.>  Конкретная версия ядра (например, 21.1.267 или 0.20.1)
  --mc <версия>            Версия Minecraft (например, 1.21.1 или 26.3)
  --pack-version <версия>  Версия сборки (например, 1.1)
  --server-name <имя>      Название сервера
  --server-ip <адрес>      Адрес сервера (host или host:port)
  -y, --yes                Не задавать вопросы, сразу сгенерировать manifest.json
  -h, --help               Показать эту справку"
    );
}

fn resolve_base_dir(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = explicit {
        return dir;
    }
    if let Ok(cwd) = env::current_dir() {
        if cwd.join(CONFIG_FILE).exists() || cwd.join("mods").is_dir() {
            return cwd;
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.join(CONFIG_FILE).exists() || parent.join("mods").is_dir() {
                return parent.to_path_buf();
            }
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn prompt_line(prompt: &str) -> Result<String, String> {
    print!("{}", prompt);
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Ошибка чтения ввода: {}", e))?;
    Ok(input.trim().to_string())
}

fn prompt_with_default(label: &str, current: &str) -> Result<String, String> {
    let ans = prompt_line(&format!("{} [текущая: {}]: ", label, current))?;
    if ans.is_empty() {
        Ok(current.to_string())
    } else {
        Ok(ans)
    }
}

fn run_interactive_wizard(config: &mut RepoConfig) -> Result<(), String> {
    println!("Текущие параметры (Enter — оставить без изменений):");
    println!("  1) Ядро (Loader):    {}", config.loader_type);
    println!("  2) Версия ядра:      {}", config.loader_version);
    println!("  3) Версия Minecraft: {}", config.minecraft_version);
    println!("  4) Версия сборки:    {}", config.pack_version);
    println!("  5) Сервер:           {} ({})", config.server_name, config.server_ip);
    println!("{}", "-".repeat(60));

    let edit = prompt_line("Изменить параметры сборки? [y/N]: ")?;
    if !matches!(edit.to_lowercase().as_str(), "y" | "yes" | "д" | "да") {
        return Ok(());
    }

    println!("\nВыберите ядро:");
    println!("  [1] fabric");
    println!("  [2] neoforge");
    println!("  [3] forge (внимание: лаунчер сейчас поддерживает fabric и neoforge)");
    println!("  [4] ввести вручную");
    let loader_choice = prompt_line(&format!(
        "Номер или название ядра [Enter = {}]: ",
        config.loader_type
    ))?;

    if !loader_choice.is_empty() {
        config.loader_type = match loader_choice.as_str() {
            "1" => "fabric".to_string(),
            "2" => "neoforge".to_string(),
            "3" => {
                println!("  ⚠ Напоминание: текущий лаунчер Eternum умеет устанавливать только fabric и neoforge.");
                "forge".to_string()
            }
            "4" => prompt_with_default("Введите название ядра", &config.loader_type)?,
            other => {
                let lower = other.to_lowercase();
                if lower != "fabric" && lower != "neoforge" {
                    println!("  ⚠ Напоминание: текущий лаунчер Eternum умеет устанавливать только fabric и neoforge.");
                }
                lower
            }
        };
    }

    config.minecraft_version =
        prompt_with_default("Версия Minecraft", &config.minecraft_version)?;
    config.loader_version = prompt_with_default(
        &format!("Конкретная версия ядра ({})", config.loader_type),
        &config.loader_version,
    )?;
    config.pack_version = prompt_with_default("Версия сборки", &config.pack_version)?;

    let edit_server = prompt_line("Изменить название/IP сервера? [y/N]: ")?;
    if matches!(edit_server.to_lowercase().as_str(), "y" | "yes" | "д" | "да") {
        config.server_name = prompt_with_default("Название сервера", &config.server_name)?;
        config.server_ip = prompt_with_default("IP сервера", &config.server_ip)?;
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
        config.repo_owner, config.repo_name, config.branch, encoded
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

fn build_http_agent() -> ureq::Agent {
    let _ = rustls::crypto::CryptoProvider::install_default(rustls_rustcrypto::provider());
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .root_certs(ureq::tls::RootCerts::WebPki)
        .build();

    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn query_modrinth_files(agent: &ureq::Agent, sha512_hashes: &[String]) -> HashMap<String, Value> {
    if sha512_hashes.is_empty() {
        return HashMap::new();
    }
    let payload = json!({
        "hashes": sha512_hashes,
        "algorithm": "sha512"
    });

    match agent
        .post(MODRINTH_API)
        .header("Content-Type", "application/json")
        .send_json(&payload)
    {
        Ok(mut resp) => match resp.body_mut().read_json::<HashMap<String, Value>>() {
            Ok(map) => map,
            Err(err) => {
                println!(
                    "⚠ Ошибка разбора ответа Modrinth API: {}. Файлы будут скачиваться с GitHub.",
                    err
                );
                HashMap::new()
            }
        },
        Err(err) => {
            println!(
                "⚠ Ошибка запроса к Modrinth API: {}. Файлы будут скачиваться с GitHub.",
                err
            );
            HashMap::new()
        }
    }
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
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

fn build_installer_url(loader_type: &str, mc_version: &str, loader_version: &str) -> Option<String> {
    match loader_type.to_lowercase().as_str() {
        "fabric" => Some(format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
            mc_version, loader_version
        )),
        "neoforge" => Some(format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            loader_version, loader_version
        )),
        "forge" => Some(format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{}-{}/forge-{}-{}-installer.jar",
            mc_version, loader_version, mc_version, loader_version
        )),
        _ => None,
    }
}

struct ModEntry {
    filename: String,
    rel_path: String,
    sha256: String,
    sha512: String,
    size: u64,
}

fn run() -> Result<(), String> {
    let cli = parse_args()?;
    if cli.show_help {
        print_help();
        return Ok(());
    }

    let base_dir = resolve_base_dir(cli.dir);
    let config_path = base_dir.join(CONFIG_FILE);

    let mut config = if config_path.exists() {
        let raw = fs::read_to_string(&config_path)
            .map_err(|e| format!("Не удалось прочитать {}: {}", config_path.display(), e))?;
        serde_json::from_str::<RepoConfig>(&raw)
            .map_err(|e| format!("Ошибка разбора {}: {}", config_path.display(), e))?
    } else {
        RepoConfig::default()
    };

    if let Some(v) = cli.loader_type {
        config.loader_type = v.to_lowercase();
    }
    if let Some(v) = cli.loader_version {
        config.loader_version = v;
    }
    if let Some(v) = cli.minecraft_version {
        config.minecraft_version = v;
    }
    if let Some(v) = cli.pack_version {
        config.pack_version = v;
    }
    if let Some(v) = cli.server_ip {
        config.server_ip = v;
    }
    if let Some(v) = cli.server_name {
        config.server_name = v;
    }

    println!("{}", "=".repeat(60));
    println!("🚀 Генератор манифеста {}", config.server_name);
    println!("📁 Папка сборки: {}", base_dir.display());
    println!(
        "🔗 Репозиторий: {}/{} (ветка: {})",
        config.repo_owner, config.repo_name, config.branch
    );
    println!("{}", "=".repeat(60));

    if !cli.non_interactive && !cli.has_cli_flags {
        run_interactive_wizard(&mut config)?;
    }

    // Сохраняем актуальный repo_config.json
    let config_pretty = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&config_path, format!("{}\n", config_pretty))
        .map_err(|e| format!("Не удалось сохранить {}: {}", config_path.display(), e))?;

    println!("\nПараметры генерации:");
    println!("  Версия сборки:    {}", config.pack_version);
    println!("  Minecraft:        {}", config.minecraft_version);
    println!("  Ядро:             {} {}", config.loader_type, config.loader_version);
    println!("  Сервер:           {} ({})", config.server_name, config.server_ip);

    let mods_dir = base_dir.join("mods");
    let config_dir = base_dir.join("config");
    fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;

    // 1. Сканирование модов
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

    println!("\n[1/4] Найдено модов в mods/: {}", mod_paths.len());

    let mut mods = Vec::with_capacity(mod_paths.len());
    let mut sha512_list = Vec::with_capacity(mod_paths.len());
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

    // 2. Поиск модов на Modrinth
    println!("[2/4] Поиск модов на серверах Modrinth...");
    let agent = build_http_agent();
    let mut modrinth_results: HashMap<String, Value> = HashMap::new();
    for chunk in sha512_list.chunks(100) {
        let res = query_modrinth_files(&agent, chunk);
        modrinth_results.extend(res);
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
            println!("  ✓ [Modrinth CDN] {}", item.filename);
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
            println!("  • [GitHub Raw]   {}", item.filename);
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

    // 3. Дополнительные папки
    println!("\n[3/4] Сканирование конфигураций, ресурспаков, шейдеров, kubejs...");
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
    println!("  Добавлено доп. файлов: {}", extra_count);

    // 4. ZIP-бандлы
    println!("\n[4/4] Сканирование zip-бандлов (bundles/)...");
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

        println!("  📦 [Bundle -> /{}] {}", extract_to, filename);
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

    println!("\n{}", "=".repeat(60));
    println!("🎉 Готово! manifest.json успешно сгенерирован:");
    println!("   - Модов с Modrinth CDN: {}", modrinth_count);
    println!("   - Модов с GitHub:       {}", github_count);
    println!("   - Доп. файлов:          {}", extra_count);
    println!("   - ZIP-бандлов:          {}", bundle_count);
    println!("   - Всего записей:        {}", total_files);
    println!("Сохранено в: {}", manifest_path.display());
    println!("{}", "=".repeat(60));

    if !cli.non_interactive && !cli.has_cli_flags {
        let _ = prompt_line("\nНажмите Enter для выхода...");
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("\n❌ Ошибка: {}", err);
        if env::args().len() <= 1 {
            let _ = prompt_line("\nНажмите Enter для выхода...");
        }
        std::process::exit(1);
    }
}
