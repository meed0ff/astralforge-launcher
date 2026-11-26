const hasTauri = typeof window.__TAURI__ !== "undefined";

const modpackDirInput = document.getElementById("modpack-dir");
const btnBrowse = document.getElementById("btn-browse");
const btnRefresh = document.getElementById("btn-refresh");
const btnOpenFolder = document.getElementById("btn-open-folder");
const configStatusEl = document.getElementById("config-status");

const badgeMods = document.getElementById("badge-mods");
const badgeExtra = document.getElementById("badge-extra");
const badgeBundles = document.getElementById("badge-bundles");
const badgeSync = document.getElementById("badge-sync");
const packSizeInfo = document.getElementById("pack-size-info");
const neededSizeInfo = document.getElementById("needed-size-info");
const diskFreeInfo = document.getElementById("disk-free-info");
const btnDownloadRepo = document.getElementById("btn-download-repo");

const updateLockBanner = document.getElementById("update-lock-banner");
const updateCommitBadge = document.getElementById("update-commit-badge");
const updateLockDesc = document.getElementById("update-lock-desc");
const updateLockMeta = document.getElementById("update-lock-meta");
const btnPullUpdate = document.getElementById("btn-pull-update");
const btnRecheckSync = document.getElementById("btn-recheck-sync");
const cardParams = document.getElementById("card-params");
const actionCard = document.querySelector(".action-card");

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
const progressDetails = document.getElementById("progress-details");
const progressBytes = document.getElementById("progress-bytes");
const progressSpeed = document.getElementById("progress-speed");

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
let isManagementLocked = false;
let syncCheckSeq = 0;

function formatBytes(bytes) {
  if (bytes == null || Number.isNaN(bytes)) return "—";
  if (bytes < 1024) return `${bytes} Б`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} КБ`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} МБ`;
  const gb = mb / 1024;
  return `${gb.toFixed(2)} ГБ`;
}

function formatSpeed(bps) {
  if (!bps || bps <= 0) return "0 КБ/с";
  return `${formatBytes(bps)}/с`;
}

function applyStorageMetrics(totalPackBytes, neededBytes, freeDiskBytes, enoughDiskSpace) {
  if (packSizeInfo && totalPackBytes != null) {
    packSizeInfo.textContent = `Вес сборки: ${formatBytes(totalPackBytes)}`;
  }
  if (neededSizeInfo) {
    if (neededBytes == null) {
      neededSizeInfo.textContent = "Нужно скачать: —";
      neededSizeInfo.className = "storage-chip";
    } else if (neededBytes === 0) {
      neededSizeInfo.textContent = "Нужно скачать: 0 Б (актуально)";
      neededSizeInfo.className = "storage-chip ok";
    } else {
      neededSizeInfo.textContent = `Нужно скачать: ${formatBytes(neededBytes)}`;
      neededSizeInfo.className = "storage-chip warn";
    }
  }
  if (diskFreeInfo) {
    if (freeDiskBytes == null) {
      diskFreeInfo.textContent = "Свободно на диске: —";
      diskFreeInfo.className = "storage-chip";
    } else if (enoughDiskSpace === false) {
      diskFreeInfo.textContent = `Свободно на диске: ${formatBytes(freeDiskBytes)} (мало места!)`;
      diskFreeInfo.className = "storage-chip err";
    } else if (enoughDiskSpace === true) {
      diskFreeInfo.textContent = `Свободно на диске: ${formatBytes(freeDiskBytes)}`;
      diskFreeInfo.className = "storage-chip ok";
    } else {
      diskFreeInfo.textContent = `Свободно на диске: ${formatBytes(freeDiskBytes)}`;
      diskFreeInfo.className = "storage-chip";
    }
  }
}

function setManagementLocked(locked) {
  isManagementLocked = locked;
  if (actionCard) {
    actionCard.classList.toggle("locked-section", locked);
  }
  btnGenerate.disabled = locked;
  btnGeneratePublish.disabled = locked;
  btnBumpVersion.disabled = locked;
  btnFetchVersions.disabled = locked;
  mcVersionInput.disabled = locked;
  loaderVersionInput.disabled = locked;
  packVersionInput.disabled = locked;
  serverNameInput.disabled = locked;
  serverIpInput.disabled = locked;
  customLoaderInput.disabled = locked;
  for (const btn of loaderBtns) {
    btn.disabled = locked;
  }
  for (const chip of mcChips) {
    chip.disabled = locked;
  }
}

async function checkRemoteSync() {
  if (!hasTauri) return;
  const seq = ++syncCheckSeq;
  const dir = modpackDirInput.value.trim();
  const owner = repoOwnerInput.value.trim() || "meed0ff";
  const repo = repoNameInput.value.trim() || "eternum-modpack";
  const branch = repoBranchInput.value.trim() || "main";
  const token = githubTokenInput.value.trim();

  if (badgeSync) {
    badgeSync.textContent = "GitHub: проверка...";
    badgeSync.className = "pill sync-pill";
  }

  try {
    const res = await window.__TAURI__.core.invoke("check_remote_sync", {
      dir,
      owner,
      repo,
      branch,
      token
    });
    if (seq !== syncCheckSeq) return;

    applyStorageMetrics(
      res.total_pack_bytes,
      res.needed_bytes,
      res.free_disk_bytes,
      res.enough_disk_space
    );

    if (res.status === "up_to_date") {
      badgeSync.textContent = `✓ GitHub: актуально (${(res.remote_sha || "").slice(0, 7)})`;
      badgeSync.className = "pill sync-pill ok";
      updateLockBanner.classList.add("hidden");
      setManagementLocked(false);
    } else if (res.status === "update_required") {
      const shortRemote = (res.remote_sha || "").slice(0, 7) || "update";
      badgeSync.textContent = `⚠ GitHub: есть обнова (${shortRemote})`;
      badgeSync.className = "pill sync-pill warn";
      updateCommitBadge.textContent = res.local_sha
        ? `${res.local_sha.slice(0, 7)} → ${shortRemote}`
        : `Коммит ${shortRemote}`;
      updateLockDesc.textContent = res.message;

      const metaParts = [];
      if (res.remote_message) metaParts.push(`Коммит: ${res.remote_message}`);
      if (res.remote_author) metaParts.push(`Автор: ${res.remote_author}`);
      if (res.total_pack_bytes != null) metaParts.push(`Вес сборки: ${formatBytes(res.total_pack_bytes)}`);
      if (res.needed_bytes != null) metaParts.push(`Скачать: ${formatBytes(res.needed_bytes)}`);
      if (res.free_disk_bytes != null) metaParts.push(`Свободно: ${formatBytes(res.free_disk_bytes)}`);
      if (res.enough_disk_space === false) metaParts.push("⚠ Недостаточно места на диске!");
      if (metaParts.length > 0) {
        updateLockMeta.textContent = metaParts.join("  •  ");
        updateLockMeta.classList.remove("hidden");
      } else {
        updateLockMeta.classList.add("hidden");
      }

      updateLockBanner.classList.remove("hidden");
      setManagementLocked(true);
      addLog(`🔒 Обнаружено обновление на GitHub (${shortRemote}). Управление заблокировано до скачивания.`, "error");
    } else if (res.status === "uninitialized") {
      badgeSync.textContent = "GitHub: новая репа";
      badgeSync.className = "pill sync-pill";
      updateLockBanner.classList.add("hidden");
      setManagementLocked(false);
    } else {
      badgeSync.textContent = "GitHub: не проверено";
      badgeSync.className = "pill sync-pill err";
      updateLockBanner.classList.add("hidden");
      setManagementLocked(false);
      addLog(`Проверка обновлений GitHub: ${res.message}`, "info");
    }
  } catch (err) {
    if (seq !== syncCheckSeq) return;
    badgeSync.textContent = "GitHub: ошибка";
    badgeSync.className = "pill sync-pill err";
    updateLockBanner.classList.add("hidden");
    setManagementLocked(false);
    addLog(`Ошибка проверки обновлений: ${err}`, "error");
  }
}

async function downloadFromGithub() {
  if (!hasTauri) return;
  const dir = modpackDirInput.value.trim();
  if (!dir) {
    showToast("Укажите папку, куда скачать сборку.", "error");
    return;
  }
  const owner = repoOwnerInput.value.trim() || "meed0ff";
  const repo = repoNameInput.value.trim() || "eternum-modpack";
  const branch = repoBranchInput.value.trim() || "main";
  const token = githubTokenInput.value.trim();

  btnDownloadRepo.disabled = true;
  btnPullUpdate.disabled = true;
  btnRecheckSync.disabled = true;
  progressWrap.classList.remove("hidden");
  if (progressDetails) progressDetails.classList.add("hidden");
  progressFill.style.width = "5%";
  progressPercent.textContent = "5%";
  progressStatus.textContent = `Скачивание сборки из ${owner}/${repo}...`;
  logBox.innerHTML = "";

  try {
    const res = await window.__TAURI__.core.invoke("download_modpack_from_github", {
      dir,
      owner,
      repo,
      branch,
      token
    });
    showToast(
      `Сборка скачана (${res.commit_sha.slice(0, 7)})! GitHub: ${res.downloaded_github}, Modrinth: ${res.downloaded_modrinth}, кэш: ${res.skipped_cached}`,
      "success"
    );
    await loadFolderOverview(dir);
  } catch (err) {
    addLog(`Ошибка скачивания сборки: ${err}`, "error");
    showToast(`Ошибка скачивания: ${err}`, "error");
  } finally {
    btnDownloadRepo.disabled = false;
    btnPullUpdate.disabled = false;
    btnRecheckSync.disabled = false;
  }
}

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
    applyStorageMetrics(data.local_size_bytes, null, data.free_disk_bytes, null);

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

    await checkRemoteSync();
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
    await checkRemoteSync();
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
  if (isManagementLocked) {
    showToast("Сначала скачайте обновление сборки с GitHub!", "error");
    return;
  }
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
  if (progressDetails) progressDetails.classList.add("hidden");
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
btnDownloadRepo.addEventListener("click", downloadFromGithub);
btnPullUpdate.addEventListener("click", downloadFromGithub);
btnRecheckSync.addEventListener("click", checkRemoteSync);
repoOwnerInput.addEventListener("change", checkRemoteSync);
repoNameInput.addEventListener("change", checkRemoteSync);
repoBranchInput.addEventListener("change", checkRemoteSync);

async function initApp() {
  if (!hasTauri) return;

  if (window.__TAURI__.event) {
    window.__TAURI__.event.listen("gen-progress", (event) => {
      const p = event.payload;
      progressWrap.classList.remove("hidden");
      progressFill.style.width = `${p.percent}%`;
      progressPercent.textContent = `${p.percent}%`;
      progressStatus.textContent = p.status;
      if (p.percent === 100 && progressDetails) {
        progressDetails.classList.add("hidden");
      }
      if (p.log) {
        addLog(p.log, p.percent === 100 ? "success" : "info");
      }
    });

    window.__TAURI__.event.listen("download-stats", (event) => {
      const s = event.payload;
      if (!progressDetails) return;
      progressWrap.classList.remove("hidden");
      progressDetails.classList.remove("hidden");
      if (progressBytes) {
        const filePart = `Файл: ${formatBytes(s.file_downloaded)} / ${formatBytes(s.file_total)}`;
        const totalPart = s.total_bytes > 0
          ? ` • Всего: ${formatBytes(s.total_downloaded)} / ${formatBytes(s.total_bytes)}`
          : "";
        progressBytes.textContent = filePart + totalPart;
      }
      if (progressSpeed) {
        progressSpeed.textContent = `⚡ ${formatSpeed(s.speed_bps)}`;
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
