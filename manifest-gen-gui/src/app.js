const hasTauri = typeof window.__TAURI__ !== "undefined";

const modpackDirInput = document.getElementById("modpack-dir");
const btnBrowse = document.getElementById("btn-browse");
const btnRefresh = document.getElementById("btn-refresh");
const btnOpenFolder = document.getElementById("btn-open-folder");
const configStatusEl = document.getElementById("config-status");

const badgeMods = document.getElementById("badge-mods");
const badgeExtra = document.getElementById("badge-extra");
const badgeBundles = document.getElementById("badge-bundles");

const loaderBtns = Array.from(document.querySelectorAll(".loader-btn"));
const customLoaderRow = document.getElementById("custom-loader-row");
const customLoaderInput = document.getElementById("custom-loader-input");
const loaderWarning = document.getElementById("loader-warning");

const mcVersionInput = document.getElementById("mc-version");
const mcChips = Array.from(document.querySelectorAll(".mc-chip"));
const loaderVersionInput = document.getElementById("loader-version");
const btnFetchVersions = document.getElementById("btn-fetch-versions");
const versionPicker = document.getElementById("version-picker");
const versionFilter = document.getElementById("version-filter");
const versionList = document.getElementById("version-list");
const btnCloseVersions = document.getElementById("btn-close-versions");

const packVersionInput = document.getElementById("pack-version");
const btnBumpVersion = document.getElementById("btn-bump-version");
const serverNameInput = document.getElementById("server-name");
const serverIpInput = document.getElementById("server-ip");

const repoOwnerInput = document.getElementById("repo-owner");
const repoNameInput = document.getElementById("repo-name");
const repoBranchInput = document.getElementById("repo-branch");

const githubTokenInput = document.getElementById("github-token");
const btnToggleToken = document.getElementById("btn-toggle-token");
const btnCheckGithub = document.getElementById("btn-check-github");
const btnToggleGuide = document.getElementById("btn-toggle-guide");
const githubGuide = document.getElementById("github-guide");
const githubStatusEl = document.getElementById("github-status");

const btnGeneratePublish = document.getElementById("btn-generate-publish");
const btnGenerate = document.getElementById("btn-generate");
const progressWrap = document.getElementById("progress-wrap");
const progressStatus = document.getElementById("progress-status");
const progressPercent = document.getElementById("progress-percent");
const progressFill = document.getElementById("progress-fill");

const statModrinth = document.getElementById("stat-modrinth");
const statGithub = document.getElementById("stat-github");
const statExtra = document.getElementById("stat-extra");
const statBundles = document.getElementById("stat-bundles");
const logBox = document.getElementById("log-box");
const toastContainer = document.getElementById("toast-container");

let selectedLoader = "neoforge";
let cachedBundlesMap = {};
let fetchedVersions = [];
let versionRequestSeq = 0;

function showToast(msg, type = "info") {
  const el = document.createElement("div");
  el.className = `toast ${type}`;
  el.textContent = msg;
  toastContainer.appendChild(el);
  setTimeout(() => el.remove(), 4000);
}

function addLog(msg, type = "info") {
  const line = document.createElement("div");
  line.className = `log-line ${type}`;
  const time = new Date().toLocaleTimeString();
  line.textContent = `[${time}] ${msg}`;
  logBox.appendChild(line);
  logBox.scrollTop = logBox.scrollHeight;
}

function setLoader(loader) {
  const normalized = (loader || "").trim().toLowerCase();
  selectedLoader = normalized;

  const presets = ["fabric", "neoforge", "forge"];
  const isPreset = presets.includes(normalized);

  for (const btn of loaderBtns) {
    const val = btn.dataset.loader;
    btn.classList.toggle("active", isPreset ? val === normalized : val === "custom");
  }

  customLoaderRow.classList.toggle("hidden", isPreset);
  if (!isPreset) {
    customLoaderInput.value = normalized;
  }

  const supported = normalized === "fabric" || normalized === "neoforge";
  loaderWarning.classList.toggle("hidden", supported || normalized === "");
  versionPicker.classList.add("hidden");
}

function getEffectiveLoader() {
  const active = loaderBtns.find((b) => b.classList.contains("active"));
  if (active && active.dataset.loader !== "custom") {
    return active.dataset.loader;
  }
  return (customLoaderInput.value || selectedLoader || "").trim().toLowerCase();
}

function bumpVersionString(ver) {
  const trimmed = (ver || "").trim();
  if (!trimmed) return "1.1";
  const match = trimmed.match(/^(\d+)\.(\d+)(.*)$/);
  if (match) {
    const major = match[1];
    const minor = parseInt(match[2], 10) + 1;
    const suffix = match[3] || "";
    return `${major}.${minor}${suffix}`;
  }
  const intMatch = trimmed.match(/^(\d+)(.*)$/);
  if (intMatch) {
    return `${parseInt(intMatch[1], 10) + 1}${intMatch[2] || ""}`;
  }
  return `${trimmed}.1`;
}

async function loadFolderOverview(dir) {
  if (!hasTauri) return;
  try {
    const data = await window.__TAURI__.core.invoke("inspect_modpack_dir", { dir });
    modpackDirInput.value = data.dir;
    badgeMods.textContent = `Модов: ${data.mods_count}`;
    badgeExtra.textContent = `Доп. файлов: ${data.extra_count}`;
    badgeBundles.textContent = `Бандлов: ${data.bundles_count}`;

    if (data.has_config) {
      configStatusEl.textContent = "✓ Найден repo_config.json — текущие параметры загружены.";
      configStatusEl.className = "hint-text ok";
    } else {
      configStatusEl.textContent = "ℹ repo_config.json не найден — будут созданы настройки по умолчанию при генерации.";
      configStatusEl.className = "hint-text warn";
    }

    const c = data.config;
    cachedBundlesMap = c.bundles_extract_map || {};
    setLoader(c.loader_type || "neoforge");
    mcVersionInput.value = c.minecraft_version || "1.21.1";
    loaderVersionInput.value = c.loader_version || "";
    packVersionInput.value = c.pack_version || "1.0";
    serverNameInput.value = c.server_name || "Eternum";
    serverIpInput.value = c.server_ip || "eternum.aurorix.pro";
    repoOwnerInput.value = c.repo_owner || "meed0ff";
    repoNameInput.value = c.repo_name || "eternum-modpack";
    repoBranchInput.value = c.branch || "main";
  } catch (err) {
    configStatusEl.textContent = `Ошибка: ${err}`;
    configStatusEl.className = "hint-text";
    showToast(String(err), "error");
  }
}

for (const btn of loaderBtns) {
  btn.addEventListener("click", () => {
    const val = btn.dataset.loader;
    const prevLoader = getEffectiveLoader();
    if (val === "custom") {
      setLoader(customLoaderInput.value || "quilt");
      customLoaderInput.focus();
    } else {
      setLoader(val);
      if (val !== prevLoader && (val === "fabric" || val === "neoforge")) {
        loadLoaderVersions({ openDropdown: false, forceLatest: true });
      }
    }
  });
}

customLoaderInput.addEventListener("input", () => {
  const v = customLoaderInput.value.trim().toLowerCase();
  const supported = v === "fabric" || v === "neoforge";
  loaderWarning.classList.toggle("hidden", supported || v === "");
});

for (const chip of mcChips) {
  chip.addEventListener("click", () => {
    mcVersionInput.value = chip.dataset.mc;
    versionPicker.classList.add("hidden");
    const loader = getEffectiveLoader();
    if (loader === "fabric" || loader === "neoforge") {
      loadLoaderVersions({ openDropdown: false, forceLatest: true });
    }
  });
}

mcVersionInput.addEventListener("change", () => {
  versionPicker.classList.add("hidden");
  const loader = getEffectiveLoader();
  if (loader === "fabric" || loader === "neoforge") {
    loadLoaderVersions({ openDropdown: false, forceLatest: true });
  }
});

btnBumpVersion.addEventListener("click", () => {
  packVersionInput.value = bumpVersionString(packVersionInput.value);
});

btnBrowse.addEventListener("click", async () => {
  if (!hasTauri) return;
  try {
    const picked = await window.__TAURI__.core.invoke("pick_folder", {
      initialDir: modpackDirInput.value.trim()
    });
    if (picked) {
      await loadFolderOverview(picked);
      addLog(`Выбрана папка сборки: ${picked}`);
    }
  } catch (err) {
    showToast(`Не удалось выбрать папку: ${err}`, "error");
  }
});

btnRefresh.addEventListener("click", () => {
  loadFolderOverview(modpackDirInput.value.trim());
});

btnOpenFolder.addEventListener("click", async () => {
  if (!hasTauri) return;
  try {
    await window.__TAURI__.core.invoke("open_folder", {
      dir: modpackDirInput.value.trim()
    });
  } catch (err) {
    showToast(`Не удалось открыть папку: ${err}`, "error");
  }
});

function renderVersionList() {
  const query = (versionFilter ? versionFilter.value : "").trim().toLowerCase();
  const current = loaderVersionInput.value.trim();
  const filtered = query
    ? fetchedVersions.filter((v) => v.toLowerCase().includes(query))
    : fetchedVersions;

  versionList.innerHTML = "";
  if (filtered.length === 0) {
    const empty = document.createElement("div");
    empty.className = "version-empty";
    empty.textContent = "Ничего не найдено";
    versionList.appendChild(empty);
    return;
  }

  for (let i = 0; i < filtered.length; i++) {
    const v = filtered[i];
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "version-item";
    if (v === current) btn.classList.add("active");

    const label = document.createElement("span");
    label.textContent = v;
    btn.appendChild(label);

    if (v === fetchedVersions[0]) {
      const tag = document.createElement("span");
      tag.className = "version-latest-tag";
      tag.textContent = "latest";
      btn.appendChild(tag);
    }

    btn.addEventListener("click", () => {
      loaderVersionInput.value = v;
      versionPicker.classList.add("hidden");
    });
    versionList.appendChild(btn);
  }
}

async function loadLoaderVersions({ openDropdown = false, forceLatest = false } = {}) {
  if (!hasTauri) return;
  const loaderType = getEffectiveLoader();
  const mcVersion = mcVersionInput.value.trim();
  if (!mcVersion) return;

  const reqId = ++versionRequestSeq;
  btnFetchVersions.disabled = true;
  btnFetchVersions.textContent = "Загрузка...";
  try {
    const versions = await window.__TAURI__.core.invoke("fetch_loader_versions", {
      loaderType,
      mcVersion
    });
    if (reqId !== versionRequestSeq) return;

    fetchedVersions = versions || [];
    if (fetchedVersions.length > 0) {
      if (forceLatest || !fetchedVersions.includes(loaderVersionInput.value.trim())) {
        loaderVersionInput.value = fetchedVersions[0];
      }
      if (openDropdown) {
        if (versionFilter) versionFilter.value = "";
        renderVersionList();
        versionPicker.classList.remove("hidden");
        if (versionFilter) versionFilter.focus();
        addLog(`Загружено ${fetchedVersions.length} версий ${loaderType} для MC ${mcVersion}.`, "success");
      } else {
        addLog(`Автоподбор: ${loaderType} ${fetchedVersions[0]} (MC ${mcVersion}).`, "info");
      }
    }
  } catch (err) {
    if (reqId !== versionRequestSeq) return;
    if (openDropdown) {
      showToast(String(err), "error");
      addLog(`Подбор версий: ${err}`, "error");
    } else {
      addLog(`Автоподбор версий пропущен: ${err}`, "info");
    }
  } finally {
    if (reqId === versionRequestSeq) {
      btnFetchVersions.disabled = false;
      btnFetchVersions.textContent = "🔍 Подобрать";
    }
  }
}

btnFetchVersions.addEventListener("click", () => {
  loadLoaderVersions({ openDropdown: true, forceLatest: false });
});

if (versionFilter) {
  versionFilter.addEventListener("input", renderVersionList);
}

if (btnCloseVersions) {
  btnCloseVersions.addEventListener("click", () => {
    versionPicker.classList.add("hidden");
  });
}

document.addEventListener("click", (e) => {
  if (versionPicker.classList.contains("hidden")) return;
  if (!versionPicker.contains(e.target) && e.target !== btnFetchVersions) {
    versionPicker.classList.add("hidden");
  }
});

btnToggleToken.addEventListener("click", () => {
  githubTokenInput.type = githubTokenInput.type === "password" ? "text" : "password";
});

btnToggleGuide.addEventListener("click", () => {
  githubGuide.classList.toggle("hidden");
});

btnCheckGithub.addEventListener("click", async () => {
  if (!hasTauri) return;
  const owner = repoOwnerInput.value.trim() || "meed0ff";
  const repo = repoNameInput.value.trim() || "eternum-modpack";
  const token = githubTokenInput.value.trim();

  btnCheckGithub.disabled = true;
  btnCheckGithub.textContent = "Проверка...";
  try {
    const info = await window.__TAURI__.core.invoke("check_github_access", {
      owner,
      repo,
      token
    });
    const who = info.login ? `@${info.login}` : "токен";
    if (info.can_push) {
      githubStatusEl.textContent = `✓ Доступ на запись подтверждён (${who} → ${owner}/${repo}).`;
      githubStatusEl.className = "hint-text ok";
      showToast("Доступ на запись в GitHub подтверждён!", "success");
    } else {
      githubStatusEl.textContent = `⚠ ${who} видит ${owner}/${repo}, но прав на запись нет.`;
      githubStatusEl.className = "hint-text warn";
      showToast("У этого токена нет прав на запись в репозиторий.", "error");
    }
  } catch (err) {
    githubStatusEl.textContent = `❌ ${err}`;
    githubStatusEl.className = "hint-text";
    showToast(String(err), "error");
  } finally {
    btnCheckGithub.disabled = false;
    btnCheckGithub.textContent = "🔍 Проверить доступ";
  }
});

async function runGenerateAndOptionallyPublish(andPublish) {
  if (!hasTauri) return;
  const token = githubTokenInput.value.trim();
  if (andPublish && !token) {
    showToast("Вставьте GitHub токен в раздел «Доступ для загрузки на GitHub».", "error");
    githubTokenInput.focus();
    return;
  }

  const dir = modpackDirInput.value.trim();
  const config = {
    repo_owner: repoOwnerInput.value.trim() || "meed0ff",
    repo_name: repoNameInput.value.trim() || "eternum-modpack",
    branch: repoBranchInput.value.trim() || "main",
    pack_version: packVersionInput.value.trim() || "1.0",
    minecraft_version: mcVersionInput.value.trim(),
    loader_type: getEffectiveLoader(),
    loader_version: loaderVersionInput.value.trim(),
    server_name: serverNameInput.value.trim() || "Eternum",
    server_ip: serverIpInput.value.trim() || "eternum.aurorix.pro",
    bundles_extract_map: cachedBundlesMap
  };

  btnGenerate.disabled = true;
  btnGeneratePublish.disabled = true;
  progressWrap.classList.remove("hidden");
  progressFill.style.width = "5%";
  progressPercent.textContent = "5%";
  progressStatus.textContent = "Запуск генерации...";
  logBox.innerHTML = "";

  try {
    const res = await window.__TAURI__.core.invoke("generate_manifest", { dir, config });
    statModrinth.textContent = res.modrinth_count;
    statGithub.textContent = res.github_count;
    statExtra.textContent = res.extra_count;
    statBundles.textContent = res.bundle_count;

    if (andPublish) {
      const pub = await window.__TAURI__.core.invoke("publish_to_github", {
        dir,
        token,
        commitMessage: null
      });
      if (pub.no_changes) {
        showToast("manifest.json обновлён — на GitHub уже актуальная версия!", "success");
      } else {
        showToast(
          `Опубликовано на GitHub! Обновлено: ${pub.uploaded_count}, удалено: ${pub.removed_count}`,
          "success"
        );
      }
    } else {
      showToast(`Готово! Записей в manifest.json: ${res.total_files}`, "success");
    }

    await loadFolderOverview(dir);
  } catch (err) {
    addLog(`Ошибка: ${err}`, "error");
    showToast(`Ошибка: ${err}`, "error");
  } finally {
    btnGenerate.disabled = false;
    btnGeneratePublish.disabled = false;
  }
}

btnGenerate.addEventListener("click", () => runGenerateAndOptionallyPublish(false));
btnGeneratePublish.addEventListener("click", () => runGenerateAndOptionallyPublish(true));

async function initApp() {
  if (!hasTauri) return;

  if (window.__TAURI__.event) {
    window.__TAURI__.event.listen("gen-progress", (event) => {
      const p = event.payload;
      progressWrap.classList.remove("hidden");
      progressFill.style.width = `${p.percent}%`;
      progressPercent.textContent = `${p.percent}%`;
      progressStatus.textContent = p.status;
      if (p.log) {
        addLog(p.log, p.percent === 100 ? "success" : "info");
      }
    });
  }

  try {
    const savedToken = await window.__TAURI__.core.invoke("load_github_token");
    if (savedToken) {
      githubTokenInput.value = savedToken;
      githubStatusEl.textContent = "✓ Сохранённый токен загружен (можно нажать «Проверить доступ»).";
      githubStatusEl.className = "hint-text ok";
    }
    const initialDir = await window.__TAURI__.core.invoke("detect_initial_dir");
    await loadFolderOverview(initialDir);
  } catch (err) {
    addLog(`Ошибка инициализации: ${err}`, "error");
  }
}

if (document.readyState === "loading") {
  window.addEventListener("DOMContentLoaded", initApp);
} else {
  initApp();
}
