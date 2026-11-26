# Репозиторий сборки Eternum

Этот репозиторий хранит манифест сборки (`manifest.json`), конфигурацию (`repo_config.json`), конфиги и кастомные моды для **Eternum Launcher**.

---

### Как устроено распространение файлов
1. **Моды с Modrinth:** при генерации `manifest.json` хеши `.jar` из `mods/` сверяются с Modrinth API. Найденные моды игроки качают напрямую с **Modrinth CDN** (`cdn.modrinth.com`), поэтому в GitHub их можно не хранить.
2. **Кастомные моды и конфиги:** моды, которых нет на Modrinth, а также `config/`, `defaultconfigs/`, `resourcepacks/`, `shaderpacks/`, `kubejs/`, `datapacks/`, `options.txt`, `servers.dat` и архивы `bundles/*.zip` скачиваются с **GitHub Raw**.

---

### Как обновлять сборку
* **Через GUI-менеджер (`Eternum Modpack Manager`):** выберите папку сборки, при необходимости поменяйте ядро/версию/моды и нажмите **«🚀 СОБРАТЬ И ЗАГРУЗИТЬ НА GITHUB»** (поддерживает работу без установленного Git).
* **Через консольный генератор / Python:**
  ```bash
  python3 generate_manifest.py
  git add .
  git commit -m "Update modpack"
  git push origin main
  ```
  После пуша игроки получат обновление автоматически при следующем нажатии **«ИГРАТЬ»** в лаунчере.
