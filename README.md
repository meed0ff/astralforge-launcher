# 🌌 Eternum — Кастомный лаунчер и менеджер сборки Minecraft

Комплексное open-source решение для вашего сервера Minecraft без сторонних бэкендов:
1. **`launcher/`** — десктопный лаунчер на **Tauri v2** (Rust + HTML5/CSS3/Vanilla JS) с поддержкой **Fabric** и **NeoForge**, авторизацией **Ely.by OAuth2** (`authlib-injector`) и офлайн-режимом, счётчиком скорости/размера скачивания и проверкой места на диске.
2. **`manifest-gen-gui/`** — графический менеджер сборки **Eternum Modpack Manager** (Tauri v2): выбор ядра (`Fabric` / `NeoForge` / `Forge` / вручную), автоподбор версий ядра под версию Minecraft, генерация `manifest.json`, двусторонняя синхронизация с GitHub и прямая публикация обновлений по токену **без необходимости устанавливать Git**.
3. **`manifest-gen/`** — консольный автономный генератор `manifest.json` на Rust (Windows `.exe` и статический бинарник Linux `x86_64-unknown-linux-musl`).
4. **`repo/`** — шаблон репозитория сборки (`repo_config.json`, `generate_manifest.py`, структура папок).
5. **`preview_server.py`** — локальный HTTP-сервер для быстрого предпросмотра UI лаунчера в браузере.

---

## 📦 Готовые сборки (Releases)

Готовые установщики и портативные архивы доступны во вкладке **[Releases](../../releases)** этого репозитория:
* **Для игроков:** установщик `Eternum Launcher_..._setup.exe` / `.msi` или портативный `EternumLauncher-win64.zip`.
* **Для администраторов сборки:** установщик `Eternum Modpack Manager_..._setup.exe` / `.msi`, портативный `EternumModpackManager-win64.zip` или консольный `eternum-manifest-gen-linux-x64` для Linux.

> **Важно для портативных `.zip` на Windows:** рядом с `eternum-launcher.exe` и `eternum-manifest-studio.exe` обязательно должен лежать файл `WebView2Loader.dll` из архива. В установщиках `.setup.exe` / `.msi` всё уже включено автоматически.

---

## 💎 Как устроена работа со сборкой

* **Никаких сторонних серверов и платного хостинга:**
  * Моды, которые есть на **Modrinth**, лаунчер скачивает напрямую с **`cdn.modrinth.com`** на максимальной скорости (они не занимают место в Git-репозитории).
  * Кастомные моды (отсутствующие на Modrinth), конфиги (`config/`, `defaultconfigs/`), ресурспаки (`resourcepacks/`), шейдеры (`shaderpacks/`), скрипты (`kubejs/`, `datapacks/`), `options.txt`, `servers.dat`, ZIP-бандлы (`bundles/*.zip`) и сам `manifest.json` раздаются через **GitHub Raw**.
  * Ванильный клиент, библиотеки и ассеты берутся с серверов **Mojang**, модлоадеры — с **Fabric Meta/Maven** и **NeoForged Maven**, Java — с **Adoptium** (`api.adoptium.net`).
* **Авторизация и скины:**
  * **Ely.by OAuth2** + автоматическая загрузка `authlib-injector.jar` (`https://authlib-injector.yushi.moe/artifact/latest.json` с резервами BMCLAPI и GitHub Releases) и подключение к `https://authserver.ely.by`.
  * **Офлайн-режим** с сохранением ника.
* **Быстрый вход на сервер:** при запуске передаётся `--quickPlayMultiplayer <server.ip>` прямо из `manifest.json`, а статус сервера пингуется в реальном времени.

---

## 🔧 Как настроить лаунчер под свой сервер и свою сборку

Вы можете адаптировать лаунчер под любой свой проект **двумя способами**:

### Вариант 1: Без пересборки (прямо в интерфейсе лаунчера)
1. Откройте лаунчер и нажмите **⚙ (Настройки)** в правом верхнем углу.
2. В поле **«URL манифеста (GitHub raw)»** вставьте ссылку на ваш `manifest.json`:
   ```text
   https://raw.githubusercontent.com/<ВАШ_ЛОГИН>/<ВАШ_РЕПОЗИТОРИЙ>/main/manifest.json
   ```
3. Нажмите **«Сохранить»** — лаунчер сразу обновит название сервера, IP для пинга и подключения, версию игры, модлоадер и список модов из вашего манифеста.

### Вариант 2: В исходном коде (если делаете свой брендированный лаунчер)

| Что поменять | Где находится | Что именно менять |
|---|---|---|
| **URL манифеста по умолчанию** | `launcher/src/index.html` (строка ~163) | `value="..."` у `#setting-manifest-url` |
| **Дефолтный сервер до загрузки манифеста** | `launcher/src/index.html` (строки ~39, 45)<br>`launcher/src/app.js` (строки ~8–13) | Текст `#server-title`, `#server-ip-text` и объект `serverData` (`ip`, `port`, `fallbackIp`, `fallbackPort`) |
| **Название, ID приложения и заголовок окна** | `launcher/src-tauri/tauri.conf.json`<br>`launcher/src-tauri/Cargo.toml`<br>`launcher/package.json` | `productName`, `identifier`, `windows[0].title`, `name` |
| **Логотип и иконки** | `launcher/src/logo.webp`<br>`launcher/src-tauri/icons/` | Замените `logo.webp` и перегенерируйте иконки (`npx tauri icon ...`) |
| **Папка игры на ПК игрока** | `launcher/src-tauri/src/game_launcher.rs` (`get_game_dir`)<br>`launcher/src/index.html`, `launcher/src/app.js` | Измените `.eternum` на своё имя папки (например, `.myserver`) |
| **OAuth2-клиент Ely.by** *(опционально)* | `launcher/src-tauri/src/elyby.rs` | `ELYBY_CLIENT_ID`, `ELYBY_CLIENT_SECRET`, `REDIRECT_URI` (если создаёте своё OAuth-приложение в кабинете Ely.by) |

> **Подсказка:** IP и название сервера, которые лаунчер показывает на главном экране и передаёт в `--quickPlayMultiplayer`, **автоматически берутся из `manifest.json`** (`server.name` и `server.ip`). То есть после настройки ссылки на ваш репозиторий сборка и адрес сервера меняются через Менеджер сборки без перекомпиляции лаунчера.

---

## 🛠️ Как настроить Менеджер сборки под свой репозиторий

Менеджер сборки (`manifest-gen-gui/`) позволяет обновлять сборку на GitHub в 1 клик без терминала и установленного Git.

### Настройка в интерфейсе программы (без правки кода)
1. Создайте на GitHub **публичный** репозиторий для вашей сборки (публичный нужен, чтобы лаунчер игроков мог свободно скачивать `manifest.json` и конфиги через `raw.githubusercontent.com` без токенов).
2. Запустите **Eternum Modpack Manager**.
3. В блоке **«1. Папка со сборкой»** выберите пустую или существующую папку сборки (**«📂 Обзор...»**).
4. В блоке **«3. Доступ для загрузки на GitHub»** раскройте **«Параметры GitHub-репозитория (владелец, имя, ветка)»** и укажите:
   * **Владелец (owner):** ваш логин GitHub;
   * **Репозиторий:** имя вашего репозитория сборки;
   * **Ветка:** обычно `main`.
5. Если репозиторий уже наполнен — нажмите **«⬇ Скачать из репы»**, чтобы скачать актуальную сборку на ПК. Если он пустой — выберите ядро (`Fabric` / `NeoForge`), версию Minecraft, версию ядра (**«🔍 Подобрать»**), версию сборки, название/IP сервера и нажмите **«🚀 СОБРАТЬ И ЗАГРУЗИТЬ НА GITHUB»**.

### Если хотите поменять значения по умолчанию в коде менеджера
Отредактируйте функции `default_repo_owner()`, `default_repo_name()`, `default_branch()`, `default_server_name()`, `default_server_ip()` и др. в:
* `manifest-gen-gui/src-tauri/src/main.rs` (строки 56–82)
* `manifest-gen/src/main.rs` (строки 49–75) — для консольной версии
* `repo/repo_config.json` — шаблон конфигурации

---

## 👥 Как выдать доступ к обновлению сборки другим людям

Чтобы обновлять сборку могли только вы и доверенные люди (например, со-админы/подруга), а не кто угодно:

1. **Добавьте человека в репозиторий на GitHub:**
   * Откройте ваш репозиторий сборки → **Settings → Collaborators → Add people**.
   * Введите GitHub-логин нужного человека и выдайте доступ **Write**.
2. **Создайте ограниченный токен (Fine-grained PAT):**
   * Пользователь открывает `https://github.com/settings/tokens?type=beta` → **Generate new token**.
   * В разделе **Repository access** выбирает **Only select repositories** → указывает **только репозиторий сборки**.
   * В **Repository permissions → Contents** выставляет **Read and write**.
   * *(Альтернатива, если у помощника нет аккаунта GitHub: владелец репозитория может сам выпустить такой Fine-grained PAT строго на один этот репозиторий и передать токен помощнику — в любой момент его можно отозвать).*
3. **Ввод в программе:**
   * Помощник один раз вставляет токен в поле **«GitHub Token»** в Менеджере сборки и нажимает **«🔍 Проверить доступ»**.
   * Токен сохраняется **только локально** на ПК пользователя (`%APPDATA%\EternumManifestGen\github_token.txt` на Windows, `~/.config/EternumManifestGen/github_token.txt` на Linux) и никогда не загружается в репозиторий.
4. **Защита от перезаписи чужих правок:**
   * Менеджер хранит состояние синхронизации в `.eternum-sync.json`. Если кто-то другой уже опубликовал новую версию сборки на GitHub, программа заблокирует публикацию жёлтым баннером и предложит сначала нажать **«⬇ Скачать обновление и разблокировать»**.

---

## 📁 Структура папки сборки (что попадает в `manifest.json`)

| Папка / файл | Источник скачивания у игроков | Назначение |
|---|---|---|
| `mods/*.jar` | **Modrinth CDN** (если найден по SHA-512) или **GitHub Raw** | Все моды сборки. Лишние `.jar` у игроков удаляются автоматически |
| `config/`, `defaultconfigs/` | **GitHub Raw** | Конфиги модов |
| `resourcepacks/`, `shaderpacks/` | **GitHub Raw** | Ресурспаки и шейдеры |
| `kubejs/`, `datapacks/` | **GitHub Raw** | Скрипты KubeJS и датапаки |
| `options.txt`, `servers.dat` | **GitHub Raw** | Настройки клиента и список серверов по умолчанию |
| `bundles/*.zip` | **GitHub Raw** | Архивы для быстрой распаковки на клиенте (напр. `kubejs.zip` → `kubejs/`, `root.zip` → корень `.eternum/`) |

*(Настройка распаковки бандлов задаётся в `repo_config.json` → `bundles_extract_map`.)*

---

## 💻 Сборка из исходников

### Windows
Требуется: **Rust** (`x86_64-pc-windows-gnu` или `x86_64-pc-windows-msvc`), **Node.js LTS (v20+)**, **WebView2 Runtime** (встроен в Windows 10/11).

```powershell
# 1. Лаунчер (соберёт eternum-launcher.exe + NSIS/MSI установщики)
cd launcher
npm install
npm run build

# 2. Менеджер сборки GUI (соберёт eternum-manifest-studio.exe + NSIS/MSI установщики)
cd ..\manifest-gen-gui
npm install
npm run build

# 3. Консольный генератор (опционально)
cargo build --release --manifest-path ..\manifest-gen\Cargo.toml
```

### Linux
Требуется: **Rust**, **Node.js LTS** и системные пакеты Tauri v2:

* **Ubuntu / Debian:**
  ```bash
  sudo apt update && sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```
* **Fedora:**
  ```bash
  sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel
  ```
* **Arch Linux:**
  ```bash
  sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg
  ```

Далее сборка пакетов (`.AppImage`, `.deb`, `.rpm` и бинарников в `src-tauri/target/release/`):
```bash
cd launcher && npm install && npm run build
cd ../manifest-gen-gui && npm install && npm run build
```

Для диалога выбора папки в Linux-версии менеджера сборки используется `zenity` или `kdialog`.
