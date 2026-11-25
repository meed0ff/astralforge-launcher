# 🌌 Проект AstralForge — Кастомный лаунчер и сборка Minecraft 1.21.1 (NeoForge 21.1.267)

Комплексный проект для вашего сервера Minecraft:
1. **`repo/`** — Репозиторий сборки для GitHub (автоматическое распределение: Modrinth CDN + GitHub raw).
2. **`launcher/`** — Легковесный кроссплатформенный лаунчер на **Tauri (Rust + HTML5/CSS3)**.
3. **`preview_server.py`** — Сервер для моментального тестирования и просмотра интерфейса лаунчера в браузере.

---

## 💎 Особенности архитектуры

* **Никаких сторонних бэкендов:** Лаунчер общается напрямую с GitHub, серверами Mojang, NeoForge Maven и Modrinth CDN.
* **Экономия трафика и лимитов GitHub:**
  * Популярные моды берутся напрямую с **Modrinth CDN** (`cdn.modrinth.com`) на максимальной скорости.
  * Ваши уникальные моды, конфиги и `manifest.json` скачиваются с **GitHub**.
  * Базовый клиент и библиотеки берутся с официальных серверов Mojang и NeoForge.
* **Авторизация:**
  * **Офлайн режим:** Любой никнейм для свободного входа.
  * **Ely.by OAuth2:** Вход через аккаунт `AstralForge` (ClientID: `astralforge1`), автоматическое отображение скинов и плащей игроков через `authlib-injector`.
* **Строго одна сборка:** Никаких лишних профилей и настроек — только серверная сборка AstralForge, кнопка выбора RAM и «Играть».
* **Прямое подключение:** При запуске игра сразу подключается к вашему серверу (`--quickPlayMultiplayer`).

---

## 🛠️ Быстрый старт

### 1. Как протестировать интерфейс лаунчера прямо сейчас:
Запустите скрипт предпросмотра:
```bash
python3 /home/meedoff/projects/astralforge/preview_server.py
```
В браузере откроется полнофункциональный интерфейс с переключением вкладок (Офлайн / Ely.by), ползунком RAM, статусом сервера, окном настроек и симуляцией запуска с прогресс-баром.

---

### 2. Как настроить и загрузить сборку на GitHub (`repo/`):

1. Перейдите в папку репозитория:
   ```bash
   cd /home/meedoff/projects/astralforge/repo
   ```
2. Откройте `repo_config.json` и укажите ваш логин GitHub и имя репозитория:
   ```json
   {
     "repo_owner": "ВашGitHubЛогин",
     "repo_name": "astralforge-repo",
     "branch": "main",
     "minecraft_version": "1.21.1",
     "loader_type": "neoforge",
     "loader_version": "21.1.267",
     "server_name": "AstralForge",
     "server_ip": "play.astralforge.net:25565"
   }
   ```
3. Скопируйте моды в папку `mods/` и конфиги в папку `config/`.
4. Запустите генератор:
   ```bash
   python3 generate_manifest.py
   ```
   *Скрипт сам рассчитает хеши, найдет моды на Modrinth и пропишет CDN-ссылки, а для остальных сформирует ссылки на GitHub.*
5. Создайте новый публичный репозиторий на GitHub (например, `astralforge-repo`) и отправьте файлы:
   ```bash
   git init
   git add .
   git commit -m "Первая версия сборки AstralForge"
   git branch -M main
   git remote add origin https://github.com/ВАШ_ЛОГИН/astralforge-repo.git
   git push -u origin main
   ```

---

### 3. Как собрать лаунчер (`launcher/`):

Для сборки десктопного бинарника потребуется Rust:
1. Если Rust еще не установлен, установите его в одну команду:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source "$HOME/.cargo/env"
   ```
2. Установите зависимости Tauri (для Fedora):
   ```bash
   sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget libappindicator-gtk3-devel librsvg2-devel
   ```
3. Перейдите в папку лаунчера и запустите сборку:
   ```bash
   cd /home/meedoff/projects/astralforge/launcher
   npm install
   npm run build
   ```
   *Готовый исполняемый файл лаунчера появится в папке `src-tauri/target/release/`.*
