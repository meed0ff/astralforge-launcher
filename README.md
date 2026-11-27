# 🌌 Eternum — Кастомный лаунчер и менеджер сборки Minecraft

Комплексное open-source решение для вашего сервера Minecraft без сторонних бэкендов:
1. **`launcher/`** — десктопный лаунчер на **Tauri v2** (Rust + HTML5/CSS3/Vanilla JS) с поддержкой **Fabric** и **NeoForge**, авторизацией **Ely.by OAuth2** (`authlib-injector`) и офлайн-режимом, счётчиком скорости/размера скачивания и проверкой места на диске.
2. **`manifest-gen-gui/`** — графический менеджер сборки **Eternum Modpack Manager** (Tauri v2): выбор ядра (`Fabric` / `NeoForge` / `Forge` / вручную), автоподбор версий ядра под версию Minecraft, генерация `manifest.json`, двусторонняя синхронизация с GitHub и прямая публикация обновлений по токену **без необходимости устанавливать Git**.
3. **`repo/`** — шаблон репозитория сборки (`repo_config.json`, `generate_manifest.py`, структура папок).
4. **`preview_server.py`** — локальный HTTP-сервер для быстрого предпросмотра UI лаунчера в браузере.

---

## 📦 Готовые портативные сборки (Releases)

Готовые портативные программы для **Windows** и **Linux** (без установщиков) собираются автоматически через CI/CD и доступны в обоих зеркалах:
* **GitHub:** [`meed0ff/eternum-launcher`](https://github.com/meed0ff/eternum-launcher/releases)
* **GitLab (зеркало):** [`meed0ff/eternum-launcher`](https://gitlab.com/meed0ff/eternum-launcher/-/releases)

Файлы в релизах:
* **Для игроков:**
  * Windows: `EternumLauncher-win64.exe`
  * Fedora (нативный бинарник): `EternumLauncher-fedora-x86_64` (`chmod +x EternumLauncher-fedora-x86_64 && ./EternumLauncher-fedora-x86_64`)
  * Linux (универсальный AppImage): `EternumLauncher-linux-x86_64.AppImage` (`chmod +x EternumLauncher-linux-x86_64.AppImage && ./EternumLauncher-linux-x86_64.AppImage`)
* **Для администраторов сборки:**
  * Windows: `EternumModpackManager-win64.exe`
  * Fedora (нативный бинарник): `EternumModpackManager-fedora-x86_64` (`chmod +x` и запуск)
  * Linux (универсальный AppImage): `EternumModpackManager-linux-x86_64.AppImage` (`chmod +x` и запуск)

> **Примечание для Windows:** официальная сборка в Releases собрана через MSVC, поэтому запускается одним `.exe` файлом. При локальной сборке через MinGW (`x86_64-pc-windows-gnu`) рядом с `.exe` должен лежать `WebView2Loader.dll`.

---

## 💎 Как устроена работа со сборкой

* **Никаких сторонних серверов и платного хостинга (+ зеркало GitLab):**
  * Моды, которые есть на **Modrinth**, лаунчер скачивает напрямую с **`cdn.modrinth.com`** на максимальной скорости (они не занимают место в Git-репозитории).
  * Кастомные моды (отсутствующие на Modrinth), конфиги (`config/`, `defaultconfigs/`), ресурспаки (`resourcepacks/`), шейдеры (`shaderpacks/`), скрипты (`kubejs/`, `datapacks/`), `options.txt`, `servers.dat`, ZIP-бандлы (`bundles/*.zip`) и сам `manifest.json` раздаются через **GitHub Raw** или **GitLab Raw** (`https://gitlab.com/meed0ff/eternum-modpack/-/raw/main/...`).
  * В настройках лаунчера (`⚙`) можно выбрать основное зеркало (**GitHub** или **GitLab**). При сбое выбранного источника лаунчер автоматически переключается на резервное зеркало.
  * Оба репозитория (`eternum-launcher` и `eternum-modpack`) асинхронно зеркалируются с GitHub на GitLab через GitHub Actions (`.github/workflows/mirror-gitlab.yml`).
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
2. Выберите **Источник скачивания (зеркало)**: **GitHub** (основной) или **GitLab (зеркало)** — ссылка переключится автоматически, а при проблемах с сетью сработает резервный откат на второе зеркало.
3. При необходимости в поле **«URL манифеста»** вставьте ссылку на ваш `manifest.json`:
   ```text
   https://raw.githubusercontent.com/<ВАШ_ЛОГИН>/<ВАШ_РЕПОЗИТОРИЙ>/main/manifest.json
   ```
   или для GitLab:
   ```text
   https://gitlab.com/<ВАШ_ЛОГИН>/<ВАШ_РЕПОЗИТОРИЙ>/-/raw/main/manifest.json
   ```
4. Нажмите **«Сохранить»** — лаунчер сразу обновит название сервера, IP для пинга и подключения, версию игры, модлоадер и список модов из вашего манифеста.

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

Менеджер сборки (`manifest-gen-gui/`) позволяет обновлять сборку на **GitHub** и **GitLab** в 1 клик без терминала и установленного Git.

### Настройка в интерфейсе программы (без правки кода)
1. Создайте на **GitHub** и/или **GitLab** **публичный** репозиторий для вашей сборки (публичный нужен, чтобы лаунчер игроков мог свободно скачивать `manifest.json` и конфиги без токенов).
2. Запустите **Eternum Modpack Manager**.
3. В блоке **«1. Папка со сборкой»** выберите пустую или существующую папку сборки (**«📂 Обзор...»**).
4. В блоке **«3. Платформа и доступ для загрузки»** выберите платформу (**GitHub** или **GitLab**), раскройте **«Параметры репозитория (владелец, имя, ветка)»** и укажите:
   * **Владелец (owner):** ваш логин/группа;
   * **Репозиторий:** имя вашего репозитория сборки;
   * **Ветка:** обычно `main`.
5. Если репозиторий уже наполнен — нажмите **«⬇ Скачать из репы»**, чтобы скачать актуальную сборку на ПК. Если он пустой — выберите ядро (`Fabric` / `NeoForge`), версию Minecraft, версию ядра (**«🔍 Подобрать»**), версию сборки, название/IP сервера и нажмите **«🚀 СОБРАТЬ И ЗАГРУЗИТЬ НА GITHUB / GITLAB»**.

### Если хотите поменять значения по умолчанию в коде менеджера
Отредактируйте функции `default_repo_owner()`, `default_repo_name()`, `default_branch()`, `default_server_name()`, `default_server_ip()` и др. в:
* `manifest-gen-gui/src-tauri/src/main.rs` (строки 56–82)
* `repo/repo_config.json` — шаблон конфигурации

---

## 👥 Как выдать доступ к обновлению сборки другим людям

Чтобы обновлять сборку могли только вы и доверенные люди (например, со-админы/подруга), а не кто угодно:

1. **Добавьте человека в репозиторий:**
   * **GitHub:** репозиторий сборки → **Settings → Collaborators → Add people** → доступ **Write**.
   * **GitLab:** проект сборки → **Manage → Members → Invite members** → роль **Developer**.
2. **Создайте ограниченный токен:**
   * **GitHub:** `https://github.com/settings/tokens?type=beta` → **Generate new token** → **Only select repositories** (только репозиторий сборки) → **Contents: Read and write**.
   * **GitLab:** **Personal/Project Access Token** с правами `api` (или `read_repository` + `write_repository`).
   * *(Альтернатива: владелец может сам выпустить такой токен на один репозиторий и передать помощнику — в любой момент его можно отозвать).*
3. **Ввод в программе:**
   * Помощник выбирает нужную вкладку (**GitHub** или **GitLab**), один раз вставляет токен и нажимает **«🔍 Проверить доступ»**.
   * Токены сохраняются **только локально** на ПК пользователя (`%APPDATA%\EternumManifestGen\` на Windows, `~/.config/EternumManifestGen/` на Linux) и никогда не загружаются в репозиторий.
4. **Защита от перезаписи чужих правок:**
   * Менеджер хранит состояние синхронизации в `.eternum-sync.json`. Если кто-то другой уже опубликовал новую версию сборки на сервере, программа заблокирует публикацию жёлтым баннером и предложит сначала нажать **«⬇ Скачать обновление и разблокировать»**.

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
# 1. Лаунчер (соберёт только eternum-launcher.exe без установщиков)
cd launcher
npm install
npx tauri build --no-bundle

# 2. Менеджер сборки GUI (соберёт только eternum-manifest-studio.exe без установщиков)
cd ..\manifest-gen-gui
npm install
npx tauri build --no-bundle
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

Далее сборка портативного `.AppImage` (и бинарников в `src-tauri/target/release/`):
```bash
cd launcher && npm install && npx tauri build --bundles appimage
cd ../manifest-gen-gui && npm install && npx tauri build --bundles appimage
```

Для диалога выбора папки в Linux-версии менеджера сборки используется `zenity` или `kdialog`.
