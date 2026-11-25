#!/usr/bin/env python3
"""
AstralForge Manifest Generator
Автоматический генератор manifest.json для сборки Minecraft 1.21.1 (NeoForge 21.1.267).

Возможности:
1. Сканирует папку mods/ и вычисляет SHA-512 / SHA-256 хеши каждого мода.
2. Отправляет запрос к Modrinth API: если мод есть на Modrinth, берет быструю ссылку с cdn.modrinth.com.
3. Если мода нет на Modrinth (кастомный мод сервера), прописывает ссылку на GitHub raw.
4. Сканирует папку config/ и добавляет все файлы с ссылками на GitHub raw.
5. Формирует актуальный manifest.json.
"""

import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
from pathlib import Path

CONFIG_FILE = "repo_config.json"
MODRINTH_API = "https://api.modrinth.com/v2/version_files"
USER_AGENT = "AstralForge-ManifestGen/1.0"


def load_config():
    if os.path.exists(CONFIG_FILE):
        with open(CONFIG_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    return {
        "repo_owner": "YourGithubUsername",
        "repo_name": "astralforge-repo",
        "branch": "main",
        "minecraft_version": "1.21.1",
        "loader_type": "neoforge",
        "loader_version": "21.1.267",
        "server_name": "AstralForge",
        "server_ip": "play.astralforge.net:25565"
    }


def compute_hashes(filepath):
    """Вычисляет sha256 и sha512 файла блоками по 64 КБ."""
    sha256 = hashlib.sha256()
    sha512 = hashlib.sha512()
    size = 0
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            sha256.update(chunk)
            sha512.update(chunk)
            size += len(chunk)
    return sha256.hexdigest(), sha512.hexdigest(), size


def query_modrinth_files(sha512_hashes):
    """Запрашивает у Modrinth API информацию о файлах по их SHA-512."""
    if not sha512_hashes:
        return {}

    req_data = json.dumps({"hashes": sha512_hashes, "algorithm": "sha512"}).encode("utf-8")
    req = urllib.request.Request(
        MODRINTH_API,
        data=req_data,
        headers={"User-Agent": USER_AGENT, "Content-Type": "application/json"}
    )

    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            if resp.status == 200:
                return json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        print(f"⚠️  Ошибка запроса к Modrinth API: {e}. Файлы будут скачиваться с GitHub.")
    return {}


def main():
    config = load_config()
    print("=" * 60)
    print(f"🚀 Генератор манифеста {config.get('server_name', 'AstralForge')}")
    print(f"📦 Minecraft {config['minecraft_version']} | {config['loader_type']} {config['loader_version']}")
    print(f"🔗 Репозиторий: {config['repo_owner']}/{config['repo_name']} (ветка: {config['branch']})")
    print("=" * 60)

    base_dir = Path(__file__).resolve().parent
    mods_dir = base_dir / "mods"
    config_dir = base_dir / "config"

    mods_dir.mkdir(exist_ok=True)
    config_dir.mkdir(exist_ok=True)

    # 1. Сканирование модов
    mod_files = [f for f in mods_dir.glob("*.jar") if f.is_file()]
    print(f"\n[1/3] Найдено модов в mods/: {len(mod_files)}")

    mod_data_map = {}
    sha512_list = []

    for mod_path in mod_files:
        sha256, sha512, size = compute_hashes(mod_path)
        mod_data_map[mod_path.name] = {
            "path": f"mods/{mod_path.name}",
            "sha256": sha256,
            "sha512": sha512,
            "size": size
        }
        sha512_list.append(sha512)

    # 2. Поиск модов на Modrinth
    print("[2/3] Поиск модов на серверах Modrinth...")
    modrinth_results = {}
    # Modrinth принимает пачками
    chunk_size = 100
    for i in range(0, len(sha512_list), chunk_size):
        chunk = sha512_list[i:i + chunk_size]
        res = query_modrinth_files(chunk)
        modrinth_results.update(res)

    files_list = []
    modrinth_count = 0
    github_count = 0

    for filename, item in mod_data_map.items():
        sha512 = item["sha512"]
        if sha512 in modrinth_results:
            mod_info = modrinth_results[sha512]
            # Находим URL файла в версии Modrinth
            file_url = None
            for file_entry in mod_info.get("files", []):
                if file_entry.get("hashes", {}).get("sha512") == sha512:
                    file_url = file_entry.get("url")
                    break

            if file_url:
                files_list.append({
                    "path": item["path"],
                    "source": "modrinth",
                    "url": file_url,
                    "sha256": item["sha256"],
                    "size": item["size"]
                })
                modrinth_count += 1
                print(f"  ✓ [Modrinth CDN] {filename}")
                continue

        # Если мода нет на Modrinth
        raw_url = f"https://raw.githubusercontent.com/{config['repo_owner']}/{config['repo_name']}/{config['branch']}/mods/{filename}"
        files_list.append({
            "path": item["path"],
            "source": "github",
            "url": raw_url,
            "sha256": item["sha256"],
            "size": item["size"]
        })
        github_count += 1
        print(f"  • [GitHub Raw]   {filename}")

    # 3. Сканирование папки config/
    print("\n[3/3] Сканирование папки config/...")
    cfg_count = 0
    for root, _, files in os.walk(config_dir):
        for file in files:
            file_path = Path(root) / file
            rel_path = file_path.relative_to(base_dir).as_posix()
            sha256, _, size = compute_hashes(file_path)
            raw_url = f"https://raw.githubusercontent.com/{config['repo_owner']}/{config['repo_name']}/{config['branch']}/{rel_path}"
            files_list.append({
                "path": rel_path,
                "source": "github",
                "url": raw_url,
                "sha256": sha256,
                "size": size
            })
            cfg_count += 1

    print(f"  Добавлено файлов конфигурации: {cfg_count}")

    # 4. Формирование manifest.json
    manifest = {
        "schemaVersion": 1,
        "server": {
            "name": config.get("server_name", "AstralForge"),
            "ip": config.get("server_ip", "play.astralforge.net:25565")
        },
        "minecraft": {
            "version": config["minecraft_version"]
        },
        "loader": {
            "type": config["loader_type"],
            "version": config["loader_version"],
            "installer_url": f"https://maven.neoforged.net/releases/net/neoforged/neoforge/{config['loader_version']}/neoforge-{config['loader_version']}-installer.jar"
        },
        "elyby": {
            "clientId": "astralforge1"
        },
        "files": files_list
    }

    manifest_path = base_dir / "manifest.json"
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)

    print("\n" + "=" * 60)
    print(f"🎉 Готово! manifest.json успешно сгенерирован:")
    print(f"   - Модов с Modrinth CDN: {modrinth_count}")
    print(f"   - Модов с GitHub:        {github_count}")
    print(f"   - Конфигов с GitHub:     {cfg_count}")
    print(f"   - Всего файлов:          {len(files_list)}")
    print(f"Сохранено в: {manifest_path}")
    print("=" * 60)


if __name__ == "__main__":
    main()
