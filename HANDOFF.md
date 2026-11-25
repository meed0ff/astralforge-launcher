# AstralForge Launcher — Handoff & Инструкция для продолжения на ПК (Windows -> Linux)

Этот документ подготовлен для быстрого входа в контекст проекта на Windows-ПК (для себя/агента) без необходимости заново изучать весь код.

---

## 1. Что уже сделано и как устроен проект

Проект лежит в одном репозитории и состоит из двух частей + превью-сервера:

1. **`launcher/`** — десктопный лаунчер на **Tauri v2** (`@tauri-apps/cli ^2.0.0`, Rust `edition = 2021`, чистый HTML5/CSS3/Vanilla JS без фреймворков сборщика, `frontendDist: "../src"`).
   - **`launcher/src/index.html`, `style.css`, `app.js`**:
     - Темный космический UI под `AstralForge Official` (`1.21.1 NeoForge`).
     - Пинг сервера при старте (`check_server_status` -> `eternum.aurorix.pro:25565` с fallback на `d22.aurorix.net:25584`).
     - Переключатель вкладок: **Ely.by OAuth2** и **Офлайн режим**, сохранение состояния в `localStorage`.
     - Ползунок RAM (`2..16 GB`, по умолчанию `6 GB`).
     - Прогресс-бар (`download-progress`), выдвижная консоль логов (`game-log` + системные логи), модалка настроек (URL манифеста, путь к Java, кнопка «Открыть папку игры» `~/.astralforge`).
     - Загрузка скина/головы игрока через `fetch_elyby_skin` (бэкенд-прокси через `reqwest` + base64 `data:image/png`, с fallback на `mc-heads.net`).
   - **`launcher/src-tauri/src/main.rs`**:
     - Точка входа Tauri. При старте вызывает `elyby::try_forward_deep_link()` и `elyby::register_uri_scheme_linux()`.
     - На Linux выставляет `WEBKIT_DISABLE_DMABUF_RENDERER=1` (фикс рендера WebKitGTK на Wayland/Nvidia).
     - Команды Tauri: `check_server_status`, `start_elyby_auth`, `fetch_elyby_skin`, `open_game_folder`, `start_sync_and_launch`.
   - **`launcher/src-tauri/src/elyby.rs`**:
     - OAuth2-авторизация через Ely.by (`ClientID: astralforge1`, `redirect_uri: astralforge://callback`).
     - Локальный IPC listener на `127.0.0.1:45679`: когда браузер редиректит на `astralforge://callback?code=...`, второй инстанс лаунчера пересылает URL в основной процесс через `try_forward_deep_link()` и завершается.
     - Обмен `code` на `access_token` и запрос профиля `https://account.ely.by/api/account/v1/info`.
   - **`launcher/src-tauri/src/downloader.rs`**:
     - Сверка файлов из `manifest.json` по SHA-256, скачивание недостающих/измененных модов и конфигов (с Modrinth CDN или GitHub raw), удаление устаревших `.jar` из `mods/`.
   - **`launcher/src-tauri/src/game_launcher.rs`**:
     - Игровая директория: `dirs::home_dir().join(".astralforge")` (на Windows это `C:\Users\<User>\.astralforge`).
     - `ensure_vanilla_installed`: качает `version_manifest_v2.json` Mojang, клиентский `.jar`, библиотеки (с проверкой `rules` под ОС/архитектуру) и ассеты (`buffer_unordered(24)`).
     - `resolve_or_install_java`: ищет Java 21+ (`runtime/java-21`, `JAVA_HOME`, `PATH`, стандартные пути JVM), при отсутствии качает Eclipse Temurin с `api.adoptium.net` и распаковывает в `~/.astralforge/runtime/java-21`.
     - `ensure_neoforge_installed`: качает `neoforge-21.1.267-installer.jar` с Maven и запускает headless установку `--install-client`.
     - `ensure_authlib_injector`: качает `authlib-injector.jar` для скинов/авторизации Ely.by (`-javaagent:...=https://authlib.ely.by/api/yggdrasil`).
     - `launch_minecraft`: собирает classpath и аргументы из `neoforge-21.1.267.json` + `1.21.1.json`, добавляет `--quickPlayMultiplayer <server_ip>`, запускает процесс игры и стримит `stdout`/`stderr` в `game-log`.
   - **`launcher/src-tauri/src/server_ping.rs`**:
     - Реализация Minecraft Server List Ping (протокол 767 для 1.21.1).

2. **`repo/`** — шаблон репозитория сборки:
   - `repo_config.json` — конфиг генератора (`minecraft_version: 1.21.1`, `loader_type: neoforge`, `loader_version: 21.1.267`, `server_ip: eternum.aurorix.pro`).
   - `generate_manifest.py` — сканирует `mods/` и `config/`, проверяет моды по SHA-512 в Modrinth API (берет CDN-ссылки, если есть, иначе GitHub raw) и генерирует `manifest.json`.
   - `mods/` и `config/` сейчас пустые, `manifest.json` с `"files": []`.

3. **`preview_server.py`** — локальный HTTP-сервер на Python для быстрого предпросмотра UI лаунчера в браузере без компиляции Rust.

---

## 2. Что нужно доделать на Windows (приоритетный чеклист)

### Критично для Windows-сборки и работы:
1. **Сгенерировать иконку `icon.ico` для Windows**:
   - В `launcher/src-tauri/icons/` сейчас есть только PNG (`icon.png`, `32x32.png`, `128x128.png`, `128x128@2x.png`).
   - Перед `npm run build` на Windows нужно выполнить из `launcher/`:
     ```powershell
     npx tauri icon src-tauri/icons/icon.png
     ```
     (или убедиться, что появился `src-tauri/icons/icon.ico` и при необходимости добавить `"icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico"]` в `tauri.conf.json` -> `bundle`).
2. **Регистрация URI-схемы `astralforge://` на Windows**:
   - В `launcher/src-tauri/src/elyby.rs` сейчас реализована только `register_uri_scheme_linux()` (создание `.desktop` файла).
   - На Windows при входе через Ely.by браузер попытается открыть `astralforge://callback?code=...`, и без записи в реестре Windows не поймет, какое приложение запускать!
   - **Что сделать**: добавить `register_uri_scheme_windows()` (под `#[cfg(target_os = "windows")]`), который прописывает текущий `std::env::current_exe()` в реестр пользователя (без прав админа):
     - `HKCU\Software\Classes\astralforge` -> `(Default)` = `"URL:AstralForge Protocol"`, `"URL Protocol"` = `""`
     - `HKCU\Software\Classes\astralforge\shell\open\command` -> `(Default)` = `"\"<exe_path>\" \"%1\""`
     (Можно через `winreg` crate или `reg.exe add` без всплывающего окна).
3. **Скрытие консольного окна при запуске `java` / `tar` на Windows**:
   - В `game_launcher.rs` при запуске `Command::new(java_path)` (установка NeoForge, проверка версии Java, запуск Minecraft) и `Command::new("tar")` на Windows может мелькать или висеть пустое окно `cmd`/консоли.
   - **Что сделать**: добавить helper для `tokio::process::Command` с `#[cfg(windows)] cmd.creation_flags(0x08000000);` (`CREATE_NO_WINDOW`).
4. **Проверить распаковку Adoptium JDK `.zip` на Windows**:
   - В `game_launcher.rs` (строка 428) для Windows используется `tar -xf <zip> -C <dir> --strip-components=1`. На Windows 10/11 встроенный `bsdtar` обычно умеет `.zip`, но стоит протестировать на реальной Windows (если что, заменить на `zip` crate в Rust, чтобы не зависеть от системного `tar`).
5. **Указать реальный URL манифеста и данные репозитория**:
   - В `launcher/src/index.html` (строка 150) и `repo/repo_config.json` сейчас стоит заглушка `YourGithubUsername/astralforge-repo`.
   - После создания репозитория со сборкой (или размещения `manifest.json` на GitHub) заменить `YourGithubUsername` на реальный логин/репозиторий.
   - Наполнить `repo/mods/` и `repo/config/`, прогнать `python generate_manifest.py` и залить на GitHub.

### На потом (для Linux-сборки после Windows):
- Собрать на Linux (`npm run build` в `launcher/`) — бандлы `.deb` / `.rpm` / `.AppImage` (потребуются системные зависимости WebKitGTK `webkit2gtk4.1-devel` и др., описаны в `README.md`).
- Проверить запуск игры на Linux (Wayland/X11) и работу `astralforge://` через `.desktop`.

---

## 3. Как развернуть и собрать на Windows с нуля

### Необходимое ПО:
1. **Visual Studio Build Tools 2022** (рабочая нагрузка «Разработка классических приложений на C++» / Desktop development with C++ + Windows 10/11 SDK).
2. **Rust** (`rustup-init.exe` с `https://rustup.rs`, toolchain `x86_64-pc-windows-msvc`).
3. **Node.js LTS** (v20+ / v22+).
4. **WebView2 Runtime** (на Windows 10/11 обычно уже установлен; если нет — поставить Evergreen Bootstrapper от Microsoft).

### Команды сборки:
```powershell
git clone <URL_ЭТОГО_РЕПОЗИТОРИЯ>
cd astralforge/launcher
npm install
npx tauri icon src-tauri/icons/icon.png
npm run build
```
Готовый `.exe` / установщик (`.msi` / `.exe` NSIS) появится в `launcher/src-tauri/target/release/bundle/`.

Для режима разработки с горячей перезагрузкой:
```powershell
npm run dev
```
Для быстрого теста только UI в браузере (из корня `astralforge/`):
```powershell
python preview_server.py
```

---

## 4. Заметки для AI-агента при продолжении работы

- **Архитектурный принцип**: никаких сторонних бэкендов. Все скачивается напрямую с Modrinth CDN, GitHub raw, Mojang, NeoForge Maven и Adoptium.
- **Tauri версия**: строго **Tauri v2** (`tauri = "2.0.0"`, `@tauri-apps/cli = "^2.0.0"`, `withGlobalTauri: true`). Не путать с v1 API.
- **Безопасность/Секреты**: `ELYBY_CLIENT_SECRET` сейчас зашит в `elyby.rs` (для OAuth2 клиента `astralforge1`). Если репозиторий лаунчера будет публичным, при желании можно вынести или заменить на PKCE (если Ely.by поддерживает), либо держать репо приватным.
- **Проверка кода**: после правок в Rust обязательно прогонять `cargo check` / `cargo build` в `launcher/src-tauri/`.
