// Eternum Launcher Frontend Logic
const hasTauri = typeof window.__TAURI__ !== "undefined";

// State
let currentTab = "elyby"; // "elyby" | "offline"
let elybyAccount = null;
let gameState = "idle"; // "idle" | "launching" | "running" | "stopping"
let serverData = {
  ip: "eternum.aurorix.pro",
  port: 25565,
  fallbackIp: "eternum.aurorix.pro",
  fallbackPort: 25565
};

// DOM Elements
const tabElyby = document.getElementById("tab-elyby");
const tabOffline = document.getElementById("tab-offline");
const contentElyby = document.getElementById("content-elyby");
const contentOffline = document.getElementById("content-offline");

const elybyLoggedOut = document.getElementById("elyby-logged-out");
const elybyLoggedIn = document.getElementById("elyby-logged-in");
const btnElybyLogin = document.getElementById("btn-elyby-login");
const btnElybyLogout = document.getElementById("btn-elyby-logout");
const elybyUsernameEl = document.getElementById("elyby-username");
const elybyAvatarEl = document.getElementById("elyby-avatar");
const offlineNickInput = document.getElementById("offline-nick");

const ramInput = document.getElementById("ram-input");
const ramDecBtn = document.getElementById("ram-dec");
const ramIncBtn = document.getElementById("ram-inc");
const ramPresetBtns = Array.from(document.querySelectorAll(".ram-preset-btn"));
let currentRamGb = 6;

const btnPlay = document.getElementById("btn-play");
const playIconEl = document.getElementById("play-icon");
const playTextEl = document.getElementById("play-text");
const progressContainer = document.getElementById("progress-container");
const progressFill = document.getElementById("progress-fill");
const progressStatus = document.getElementById("progress-status");
const progressPercent = document.getElementById("progress-percent");
const progressDetails = document.getElementById("progress-details");
const progressBytes = document.getElementById("progress-bytes");
const progressSpeed = document.getElementById("progress-speed");

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

const btnLogsToggle = document.getElementById("btn-logs-toggle");
const btnLogsClose = document.getElementById("btn-logs-close");
const btnLogsCopy = document.getElementById("btn-logs-copy");
const btnLogsSave = document.getElementById("btn-logs-save");
const btnLogsClear = document.getElementById("btn-logs-clear");
const logsResizeHandle = document.getElementById("logs-resize-handle");
const logsDrawer = document.getElementById("logs-drawer");
const logsContent = document.getElementById("logs-content");

const mainView = document.getElementById("main-view");
const settingsView = document.getElementById("settings-view");
const btnSettings = document.getElementById("btn-settings");
const btnSettingsBack = document.getElementById("btn-settings-back");
const btnSaveSettings = document.getElementById("btn-save-settings");
const settingManifestUrl = document.getElementById("setting-manifest-url");
const btnCheckManifest = document.getElementById("btn-check-manifest");
const mirrorBtns = Array.from(document.querySelectorAll(".mirror-btn"));
let currentMirror = "github";
const settingJavaPath = document.getElementById("setting-java-path");
const settingStrictJava = document.getElementById("setting-strict-java");
const javaReqHint = document.getElementById("java-req-hint");
const btnAutoJava = document.getElementById("btn-auto-java");
const btnJavaRescan = document.getElementById("btn-java-rescan");
const btnJavaInstall = document.getElementById("btn-java-install");
const javaTableBody = document.getElementById("java-table-body");
const btnOpenGameFolder = document.getElementById("btn-open-game-folder");
const btnOpenLogsFolder = document.getElementById("btn-open-logs-folder");
const btnSettingsSaveLog = document.getElementById("btn-settings-save-log");

const dynamicIsland = document.getElementById("dynamic-island");
const diIcon = document.getElementById("di-icon");
const diTitle = document.getElementById("di-title");
const diPercent = document.getElementById("di-percent");
const diBarFill = document.getElementById("di-bar-fill");
const diMeta = document.getElementById("di-meta");
let diHideTimer = null;

let requiredJavaMajor = 21;
let currentPackDesc = "Minecraft 1.21.1";
let detectedJavaList = [];
let selectedJavaCandidatePath = null;
let isJavaDownloading = false;
let compatibleJavaInstalled = false;

const serverIpBtn = document.getElementById("server-ip-btn");
const serverIpText = document.getElementById("server-ip-text");
const serverTitleEl = document.getElementById("server-title");
const versionBadgeEl = document.getElementById("version-badge");
const serverMeta = document.getElementById("server-meta");
const toastContainer = document.getElementById("toast-container");

function showToast(message, type = "info") {
  if (!toastContainer) return;
  const toast = document.createElement("div");
  toast.className = `toast ${type}`;
  toast.textContent = message;
  toastContainer.appendChild(toast);
  setTimeout(() => {
    toast.classList.add("toast-leaving");
    setTimeout(() => toast.remove(), 320);
  }, 4200);
}

// Logger
function addLog(message, type = "info") {
  const line = document.createElement("div");
  line.className = `log-line ${type}`;
  const time = new Date().toLocaleTimeString();
  line.textContent = `[${time}] ${message}`;
  logsContent.appendChild(line);
  logsContent.scrollTop = logsContent.scrollHeight;
}

const authTabsHeader = document.getElementById("auth-tabs-header");

// Tabs switching (smooth gliding pill + non-jumping stage)
function switchAuthTab(tab, persist = true) {
  currentTab = tab === "offline" ? "offline" : "elyby";
  if (authTabsHeader) {
    authTabsHeader.setAttribute("data-active", currentTab);
  }
  const isElyby = currentTab === "elyby";
  tabElyby.classList.toggle("active", isElyby);
  tabOffline.classList.toggle("active", !isElyby);
  contentElyby.classList.toggle("tab-inactive", !isElyby);
  contentOffline.classList.toggle("tab-inactive", isElyby);
  contentElyby.setAttribute("aria-hidden", String(!isElyby));
  contentOffline.setAttribute("aria-hidden", String(isElyby));
  if (persist) {
    localStorage.setItem("astral_tab", currentTab);
  }
}

tabElyby.addEventListener("click", () => switchAuthTab("elyby", true));
tabOffline.addEventListener("click", () => switchAuthTab("offline", true));

// RAM Selection (presets + stepper + manual input)
function setRamGb(value, persist = true) {
  const parsed = parseInt(value, 10);
  const clamped = Number.isNaN(parsed) ? 6 : Math.min(32, Math.max(2, parsed));
  currentRamGb = clamped;
  if (ramInput) ramInput.value = String(clamped);
  if (ramDecBtn) ramDecBtn.disabled = clamped <= 2;
  if (ramIncBtn) ramIncBtn.disabled = clamped >= 32;
  for (const btn of ramPresetBtns) {
    const presetVal = parseInt(btn.dataset.ram, 10);
    btn.classList.toggle("active", presetVal === clamped);
  }
  if (persist) {
    localStorage.setItem("astral_ram", String(clamped));
  }
}

if (ramDecBtn) {
  ramDecBtn.addEventListener("click", () => setRamGb(currentRamGb - 1));
}
if (ramIncBtn) {
  ramIncBtn.addEventListener("click", () => setRamGb(currentRamGb + 1));
}
if (ramInput) {
  ramInput.addEventListener("change", () => setRamGb(ramInput.value));
  ramInput.addEventListener("blur", () => setRamGb(ramInput.value));
}
for (const btn of ramPresetBtns) {
  btn.addEventListener("click", () => setRamGb(btn.dataset.ram));
}

// Ely.by Login / Logout
btnElybyLogin.addEventListener("click", async () => {
  addLog("Запрос авторизации Ely.by...", "info");
  if (hasTauri) {
    try {
      const res = await window.__TAURI__.core.invoke("start_elyby_auth");
      setElybyUser(res);
    } catch (err) {
      addLog(`Ошибка авторизации Ely.by: ${err}`, "error");
      showToast(`Ошибка авторизации: ${err}`, "error");
    }
  } else {
    // Web Preview Simulation
    const mockUser = {
      username: "AstralHero",
      uuid: "11111111-2222-3333-4444-555555555555",
      accessToken: "mock_token_elyby"
    };
    setElybyUser(mockUser);
  }
});

btnElybyLogout.addEventListener("click", () => {
  elybyAccount = null;
  localStorage.removeItem("astral_elyby");
  elybyLoggedIn.classList.add("auth-inactive");
  elybyLoggedOut.classList.remove("auth-inactive");
  addLog("Выход из аккаунта Ely.by выполнен.");
});

const DEFAULT_AVATAR_SVG =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="8" fill="#1e1b3a"/><rect x="16" y="16" width="32" height="32" rx="4" fill="#9333ea"/><rect x="22" y="26" width="6" height="6" fill="#fff"/><rect x="36" y="26" width="6" height="6" fill="#fff"/><rect x="25" y="38" width="14" height="4" fill="#06b6d4"/></svg>'
  );

function renderHeadFromSkinSrc(src, onFail) {
  const skinImg = new Image();
  skinImg.onload = () => {
    try {
      const canvas = document.createElement("canvas");
      canvas.width = 64;
      canvas.height = 64;
      const ctx = canvas.getContext("2d");
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(skinImg, 8, 8, 8, 8, 0, 0, 64, 64);
      ctx.drawImage(skinImg, 40, 8, 8, 8, 0, 0, 64, 64);
      elybyAvatarEl.src = canvas.toDataURL();
    } catch (_) {
      if (onFail) onFail();
    }
  };
  skinImg.onerror = () => {
    if (onFail) onFail();
  };
  skinImg.src = src;
}

function loadElybyAvatar(username) {
  elybyAvatarEl.src = DEFAULT_AVATAR_SVG;
  const fallbackMcHeads = () => {
    const fallback = new Image();
    fallback.onload = () => {
      elybyAvatarEl.src = fallback.src;
    };
    fallback.src = `https://mc-heads.net/avatar/${encodeURIComponent(username)}/64`;
  };

  if (hasTauri) {
    window.__TAURI__.core
      .invoke("fetch_elyby_skin", { username })
      .then((dataUrl) => renderHeadFromSkinSrc(dataUrl, fallbackMcHeads))
      .catch(fallbackMcHeads);
  } else {
    renderHeadFromSkinSrc(
      `https://skinsystem.ely.by/skins/${encodeURIComponent(username)}.png`,
      fallbackMcHeads
    );
  }
}

function setElybyUser(user) {
  elybyAccount = user;
  localStorage.setItem("astral_elyby", JSON.stringify(user));
  elybyUsernameEl.textContent = user.username;
  loadElybyAvatar(user.username);
  elybyLoggedOut.classList.add("auth-inactive");
  elybyLoggedIn.classList.remove("auth-inactive");
  addLog(`Успешный вход в Ely.by: ${user.username}`);
}

// Copy Server IP
serverIpBtn.addEventListener("click", () => {
  navigator.clipboard.writeText(serverIpText.textContent);
  const oldText = serverMeta.textContent;
  serverMeta.textContent = "✓ IP скопирован в буфер обмена!";
  setTimeout(() => {
    serverMeta.textContent = oldText;
  }, 2000);
});

function saveLauncherLogs() {
  const lines = Array.from(logsContent.querySelectorAll(".log-line"))
    .map((el) => el.textContent)
    .join("\n");
  const header = `=== Eternum Launcher Console Log (${new Date().toISOString()}) ===\n`;
  const content = header + lines + "\n";

  if (hasTauri && window.__TAURI__?.core) {
    window.__TAURI__.core
      .invoke("save_launcher_log", { content })
      .then((savedPath) => {
        showToast(`Лог лаунчера сохранён: ${savedPath}`, "success");
        addLog(`Лог лаунчера сохранён в ${savedPath}`, "success");
      })
      .catch((err) => {
        showToast(`Не удалось сохранить лог: ${err}`, "error");
        addLog(`Ошибка сохранения лога: ${err}`, "error");
      });
  } else {
    const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `launcher-${Date.now()}.log`;
    a.click();
    URL.revokeObjectURL(url);
    showToast("Файл лога сформирован для скачивания", "success");
  }
}

function copyTextFallback(text) {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    return navigator.clipboard.writeText(text);
  }
  return new Promise((resolve, reject) => {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
      resolve();
    } catch (e) {
      reject(e);
    }
  });
}

function updateDynamicIsland(payload) {
  if (!dynamicIsland || !payload) return;
  if (diHideTimer) {
    clearTimeout(diHideTimer);
    diHideTimer = null;
  }

  const stage = payload.stage || "downloading";
  const pct = Math.max(0, Math.min(100, Math.round(payload.percent || 0)));
  const major = payload.major || requiredJavaMajor;

  dynamicIsland.classList.remove("hidden-island", "di-downloading", "di-extracting", "di-done", "di-error");
  dynamicIsland.classList.add(`di-${stage}`);

  if (diIcon) {
    if (stage === "done") diIcon.textContent = "✅";
    else if (stage === "error") diIcon.textContent = "❌";
    else if (stage === "extracting") diIcon.textContent = "📦";
    else diIcon.textContent = "☕";
  }

  if (diTitle) {
    if (stage === "downloading") diTitle.textContent = `Java ${major}`;
    else if (stage === "extracting") diTitle.textContent = `Распаковка Java ${major}...`;
    else if (stage === "done") diTitle.textContent = `Java ${major} готова`;
    else if (stage === "error") diTitle.textContent = `Ошибка Java ${major}`;
    else diTitle.textContent = payload.status || `Java ${major}`;
  }
  if (diPercent) diPercent.textContent = `${pct}%`;
  if (diBarFill) diBarFill.style.width = `${pct}%`;

  if (diMeta) {
    if (stage === "downloading" && payload.total > 0) {
      diMeta.textContent = `${formatBytes(payload.downloaded)}/${formatBytes(payload.total)} · ${formatSpeed(payload.speed_bps)}`;
    } else if (stage === "extracting") {
      diMeta.textContent = `runtime`;
    } else if (stage === "done") {
      diMeta.textContent = `установлена`;
    } else if (stage === "error") {
      diMeta.textContent = `сбой`;
    } else {
      diMeta.textContent = "";
    }
  }

  if (stage === "done") {
    isJavaDownloading = false;
    diHideTimer = setTimeout(() => {
      dynamicIsland.classList.add("hidden-island");
    }, 3200);
  } else if (stage === "error") {
    isJavaDownloading = false;
    diHideTimer = setTimeout(() => {
      dynamicIsland.classList.add("hidden-island");
    }, 5200);
  } else {
    isJavaDownloading = true;
  }
  updateJavaInstallButtonState();
}

// Logs Drawer Controls + Resizable Height
function setLogsHeight(heightPx, persist = true) {
  if (!logsDrawer) return;
  const maxH = Math.max(220, window.innerHeight - 75);
  const clamped = Math.max(130, Math.min(maxH, Math.round(heightPx)));
  logsDrawer.style.height = `${clamped}px`;
  if (persist) {
    localStorage.setItem("astral_logs_height", String(clamped));
  }
}

if (logsResizeHandle && logsDrawer) {
  let draggingLogs = false;
  logsResizeHandle.addEventListener("mousedown", (e) => {
    e.preventDefault();
    draggingLogs = true;
    logsDrawer.classList.add("resizing");
    document.body.style.cursor = "ns-resize";
  });
  window.addEventListener("mousemove", (e) => {
    if (!draggingLogs) return;
    setLogsHeight(window.innerHeight - e.clientY, false);
  });
  window.addEventListener("mouseup", () => {
    if (!draggingLogs) return;
    draggingLogs = false;
    logsDrawer.classList.remove("resizing");
    document.body.style.cursor = "";
    const curH = parseInt(logsDrawer.style.height, 10);
    if (!Number.isNaN(curH)) {
      localStorage.setItem("astral_logs_height", String(curH));
    }
  });
}

btnLogsToggle.addEventListener("click", () => {
  const isOpen = logsDrawer.classList.toggle("open");
  btnLogsToggle.classList.toggle("active", isOpen);
});
btnLogsClose.addEventListener("click", () => {
  logsDrawer.classList.remove("open");
  btnLogsToggle.classList.remove("active");
});

if (btnLogsCopy) {
  btnLogsCopy.addEventListener("click", () => {
    const sel = window.getSelection();
    const selText = sel ? sel.toString().trim() : "";
    const inConsole = sel && sel.anchorNode && logsContent.contains(sel.anchorNode);
    const toCopy = inConsole && selText
      ? selText
      : Array.from(logsContent.querySelectorAll(".log-line")).map((el) => el.textContent).join("\n");
    if (!toCopy) {
      showToast("Консоль пуста", "info");
      return;
    }
    copyTextFallback(toCopy)
      .then(() => {
        showToast(inConsole && selText ? "Выделенный фрагмент скопирован" : "Вся консоль скопирована", "success");
      })
      .catch((err) => {
        showToast(`Ошибка копирования: ${err}`, "error");
      });
  });
}

if (btnLogsSave) btnLogsSave.addEventListener("click", saveLauncherLogs);
if (btnSettingsSaveLog) btnSettingsSaveLog.addEventListener("click", saveLauncherLogs);

if (btnLogsClear) {
  btnLogsClear.addEventListener("click", () => {
    logsContent.innerHTML = "";
    addLog("Консоль очищена.");
  });
}

// Full-Screen Settings Navigation
function openSettingsView() {
  if (!settingsView || !mainView) return;
  mainView.classList.add("view-inactive");
  settingsView.classList.remove("view-inactive");
  settingsView.setAttribute("aria-hidden", "false");
  settingsView.scrollTop = 0;
  btnSettings.classList.add("active");
  updateJavaRequirementLabels();
  scanSystemJavas();
}

function closeSettingsView() {
  if (!settingsView || !mainView) return;
  settingsView.classList.add("view-inactive");
  settingsView.setAttribute("aria-hidden", "true");
  mainView.classList.remove("view-inactive");
  btnSettings.classList.remove("active");
}

btnSettings.addEventListener("click", () => {
  if (settingsView && settingsView.classList.contains("view-inactive")) {
    openSettingsView();
  } else {
    closeSettingsView();
  }
});
if (btnSettingsBack) btnSettingsBack.addEventListener("click", closeSettingsView);

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    if (logsDrawer && logsDrawer.classList.contains("open")) {
      logsDrawer.classList.remove("open");
      btnLogsToggle.classList.remove("active");
    } else if (settingsView && !settingsView.classList.contains("view-inactive")) {
      closeSettingsView();
    }
  }
});

if (btnCheckManifest) {
  btnCheckManifest.addEventListener("click", () => {
    showToast("Обновление данных манифеста...", "info");
    refreshManifestMetadata();
  });
}

function convertManifestMirrorUrl(url, mirror) {
  const trimmed = (url || "").trim();
  if (!trimmed) return trimmed;
  if (mirror === "gitlab") {
    const ghMatch = trimmed.match(/^https:\/\/raw\.githubusercontent\.com\/([^/]+)\/([^/]+)\/([^/]+)\/(.+)$/);
    if (ghMatch) {
      return `https://gitlab.com/${ghMatch[1]}/${ghMatch[2]}/-/raw/${ghMatch[3]}/${ghMatch[4]}`;
    }
  } else {
    const glMatch = trimmed.match(/^https:\/\/gitlab\.com\/(.+?)\/-\/raw\/(.+)$/);
    if (glMatch) {
      return `https://raw.githubusercontent.com/${glMatch[1]}/${glMatch[2]}`;
    }
  }
  return trimmed;
}

function setMirrorSource(mirror, updateUrl = true) {
  currentMirror = mirror === "gitlab" ? "gitlab" : "github";
  for (const btn of mirrorBtns) {
    btn.classList.toggle("active", btn.dataset.mirror === currentMirror);
  }
  if (updateUrl && settingManifestUrl) {
    settingManifestUrl.value = convertManifestMirrorUrl(settingManifestUrl.value, currentMirror);
  }
}

for (const btn of mirrorBtns) {
  btn.addEventListener("click", () => {
    setMirrorSource(btn.dataset.mirror, true);
  });
}

function inferRequiredJavaFromMc(mcVersion) {
  const parts = String(mcVersion || "1.21.1").trim().split(".").map((n) => parseInt(n, 10) || 0);
  if (parts[0] >= 26) return 25;
  if (parts[0] === 1) {
    const minor = parts[1] || 21;
    const patch = parts[2] || 0;
    if (minor <= 16) return 8;
    if (minor <= 19 || (minor === 20 && patch < 5)) return 17;
    return 21;
  }
  return 21;
}

function updateJavaInstallButtonState() {
  if (!btnJavaInstall) return;
  if (isJavaDownloading) {
    btnJavaInstall.disabled = true;
    btnJavaInstall.classList.remove("btn-installed");
    btnJavaInstall.textContent = `⏳ Скачивание Java ${requiredJavaMajor}...`;
    return;
  }
  if (compatibleJavaInstalled) {
    btnJavaInstall.disabled = true;
    btnJavaInstall.classList.add("btn-installed");
    btnJavaInstall.textContent = `✓ Java ${requiredJavaMajor} уже установлена`;
    return;
  }
  btnJavaInstall.disabled = false;
  btnJavaInstall.classList.remove("btn-installed");
  btnJavaInstall.textContent = `⬇ Скачать Java ${requiredJavaMajor}`;
}

function updateJavaRequirementLabels() {
  if (javaReqHint) {
    javaReqHint.textContent = `требуется Java ${requiredJavaMajor} (${currentPackDesc})`;
  }
  updateJavaInstallButtonState();
}

async function scanSystemJavas() {
  if (!hasTauri || !javaTableBody) return;
  selectedJavaCandidatePath = null;
  javaTableBody.innerHTML = '<tr><td colspan="5" class="java-empty">🔍 Сканирование системы...</td></tr>';
  if (btnJavaRescan) btnJavaRescan.disabled = true;
  try {
    const list = await window.__TAURI__.core.invoke("detect_java_installations", {
      requiredMajor: requiredJavaMajor
    });
    detectedJavaList = list || [];
    compatibleJavaInstalled = detectedJavaList.some(
      (item) => item.compatible && Number(item.major) === Number(requiredJavaMajor)
    );
    updateJavaInstallButtonState();

    javaTableBody.innerHTML = "";
    if (detectedJavaList.length === 0) {
      javaTableBody.innerHTML = `<tr><td colspan="5" class="java-empty">Java не найдена. Нажмите «⬇ Скачать Java ${requiredJavaMajor}» или оставьте «auto».</td></tr>`;
      return;
    }
    const currentPath = (settingJavaPath?.value || "").trim().toLowerCase();
    for (const item of detectedJavaList) {
      const tr = document.createElement("tr");
      tr.className = "java-row";
      if (item.path.toLowerCase() === currentPath || (!selectedJavaCandidatePath && item.compatible)) {
        tr.classList.add("selected");
        selectedJavaCandidatePath = item.path;
      }

      const tdVer = document.createElement("td");
      tdVer.textContent = `${item.version} (Java ${item.major})`;
      const tdArch = document.createElement("td");
      tdArch.textContent = item.arch;
      const tdVendor = document.createElement("td");
      tdVendor.textContent = item.vendor;
      const tdStatus = document.createElement("td");
      tdStatus.className = item.compatible ? "java-badge-ok" : "java-badge-bad";
      tdStatus.textContent = item.note;
      const tdPath = document.createElement("td");
      tdPath.className = "java-path-cell";
      tdPath.textContent = item.path;

      tr.append(tdVer, tdArch, tdVendor, tdStatus, tdPath);
      tr.addEventListener("click", () => {
        for (const r of javaTableBody.querySelectorAll(".java-row")) {
          r.classList.remove("selected");
        }
        tr.classList.add("selected");
        selectedJavaCandidatePath = item.path;
        if (settingJavaPath) settingJavaPath.value = item.path;
        showToast(`Выбрана Java ${item.major} (нажмите «Сохранить» сверху)`, "info");
      });
      javaTableBody.appendChild(tr);
    }
  } catch (err) {
    javaTableBody.innerHTML = `<tr><td colspan="5" class="java-empty">Ошибка сканирования: ${err}</td></tr>`;
  } finally {
    if (btnJavaRescan) btnJavaRescan.disabled = false;
  }
}

if (btnAutoJava) {
  btnAutoJava.addEventListener("click", () => {
    if (settingJavaPath) settingJavaPath.value = "auto";
    showToast(`Установлен автоподбор Java ${requiredJavaMajor}`, "info");
  });
}

if (btnJavaRescan) {
  btnJavaRescan.addEventListener("click", scanSystemJavas);
}

if (btnJavaInstall) {
  btnJavaInstall.addEventListener("click", async () => {
    if (!hasTauri || isJavaDownloading || compatibleJavaInstalled) return;
    isJavaDownloading = true;
    updateJavaInstallButtonState();
    try {
      const installedPath = await window.__TAURI__.core.invoke("install_java_runtime", {
        major: requiredJavaMajor
      });
      if (settingJavaPath) settingJavaPath.value = installedPath;
      showToast(`Java ${requiredJavaMajor} (Eclipse Temurin) готова!`, "success");
      addLog(`Java ${requiredJavaMajor} готова: ${installedPath}`, "success");
      await scanSystemJavas();
    } catch (err) {
      showToast(`Ошибка установки Java: ${err}`, "error");
      addLog(`Ошибка установки Java ${requiredJavaMajor}: ${err}`, "error");
    } finally {
      isJavaDownloading = false;
      updateJavaInstallButtonState();
    }
  });
}

btnSaveSettings.addEventListener("click", () => {
  setRamGb(ramInput ? ramInput.value : currentRamGb);
  localStorage.setItem("astral_mirror", currentMirror);
  localStorage.setItem("astral_manifest_url", settingManifestUrl.value);
  localStorage.setItem("astral_java_path", settingJavaPath.value.trim() || "auto");
  if (settingStrictJava) {
    localStorage.setItem("astral_strict_java", settingStrictJava.checked ? "1" : "0");
  }
  closeSettingsView();
  addLog(`Настройки сохранены (RAM: ${currentRamGb} GB, зеркало: ${currentMirror === "gitlab" ? "GitLab" : "GitHub"}, Java: ${settingJavaPath.value.trim() || "auto"}).`);
  showToast("Настройки сохранены", "success");
  refreshManifestMetadata();
});

btnOpenGameFolder.addEventListener("click", () => {
  if (hasTauri) {
    window.__TAURI__.core.invoke("open_game_folder").catch((err) => {
      showToast(`Не удалось открыть папку: ${err}`, "error");
    });
  } else {
    showToast("Папка игры: ~/.eternum", "info");
  }
});

if (btnOpenLogsFolder) {
  btnOpenLogsFolder.addEventListener("click", () => {
    if (hasTauri) {
      window.__TAURI__.core.invoke("open_logs_folder").catch((err) => {
        showToast(`Не удалось открыть папку логов: ${err}`, "error");
      });
    } else {
      showToast("Папка логов: ~/.eternum/logs", "info");
    }
  });
}

// Update Progress Display
function setProgress(percent, status) {
  progressContainer.style.display = "flex";
  progressFill.style.width = `${percent}%`;
  progressPercent.textContent = `${percent}%`;
  progressStatus.textContent = status;
}

function setPlayButtonMode(mode) {
  gameState = mode;
  if (mode === "running") {
    btnPlay.disabled = false;
    btnPlay.classList.add("btn-stop");
    if (playIconEl) playIconEl.textContent = "■";
    if (playTextEl) playTextEl.textContent = "ОСТАНОВИТЬ";
  } else if (mode === "stopping") {
    btnPlay.disabled = true;
    btnPlay.classList.add("btn-stop");
    if (playIconEl) playIconEl.textContent = "■";
    if (playTextEl) playTextEl.textContent = "ОСТАНОВКА...";
  } else if (mode === "launching") {
    btnPlay.disabled = true;
    btnPlay.classList.remove("btn-stop");
    if (playIconEl) playIconEl.textContent = "⏳";
    if (playTextEl) playTextEl.textContent = "ЗАПУСК...";
  } else {
    btnPlay.disabled = false;
    btnPlay.classList.remove("btn-stop");
    if (playIconEl) playIconEl.textContent = "▶";
    if (playTextEl) playTextEl.textContent = "ИГРАТЬ";
  }
}

// Play / Stop Button
btnPlay.addEventListener("click", async () => {
  if (gameState === "running") {
    setPlayButtonMode("stopping");
    addLog("Остановка процесса Minecraft...", "info");
    if (hasTauri) {
      try {
        await window.__TAURI__.core.invoke("stop_game");
      } catch (err) {
        addLog(`Не удалось остановить игру: ${err}`, "error");
        showToast(`Не удалось остановить игру: ${err}`, "error");
        setPlayButtonMode("idle");
      }
    } else {
      setTimeout(() => {
        setPlayButtonMode("idle");
        addLog("Процесс игры остановлен пользователем.");
      }, 400);
    }
    return;
  }

  if (gameState !== "idle") return;

  let nickname = "";
  let authType = currentTab;
  let accessToken = "0";
  let uuid = "00000000-0000-0000-0000-000000000000";

  if (currentTab === "elyby") {
    if (!elybyAccount) {
      showToast("Сначала войдите в аккаунт Ely.by или выберите Офлайн режим.", "error");
      return;
    }
    nickname = elybyAccount.username;
    accessToken = elybyAccount.accessToken || elybyAccount.access_token || "0";
    uuid = elybyAccount.uuid;
  } else {
    nickname = offlineNickInput.value.trim();
    if (!nickname) {
      showToast("Введите никнейм для игры в офлайн режиме.", "error");
      offlineNickInput.focus();
      return;
    }
    localStorage.setItem("astral_nick", nickname);
  }

  setPlayButtonMode("launching");
  addLog(`Подготовка к запуску для ${nickname} (${authType})...`);
  setProgress(5, "Чтение манифеста сборки...");

  const launchRequest = {
    nickname,
    authType,
    uuid,
    accessToken,
    ramGb: currentRamGb,
    javaPath: settingJavaPath.value.trim() || "auto",
    strictJavaCheck: settingStrictJava ? settingStrictJava.checked : true,
    manifestUrl: settingManifestUrl.value.trim(),
    mirrorSource: currentMirror
  };

  if (hasTauri) {
    try {
      await window.__TAURI__.core.invoke("start_sync_and_launch", { request: launchRequest });
      if (gameState === "launching") {
        setPlayButtonMode("running");
      }
      setTimeout(() => {
        progressContainer.style.display = "none";
      }, 2000);
    } catch (err) {
      addLog(`Ошибка запуска: ${err}`, "error");
      showToast(`Ошибка запуска: ${err}`, "error");
      setPlayButtonMode("idle");
      progressContainer.style.display = "none";
    }
  } else {
    // Simulated browser demo
    simulateLaunch();
  }
});

function simulateLaunch() {
  const steps = [
    { p: 15, s: "Сверка манифеста с GitHub..." },
    { p: 35, s: "Проверка модов с Modrinth CDN..." },
    { p: 65, s: "Сверка конфигурационных файлов..." },
    { p: 85, s: "Проверка библиотек Fabric..." },
    { p: 100, s: "Запуск Minecraft!" }
  ];
  let idx = 0;
  const interval = setInterval(() => {
    if (idx < steps.length) {
      setProgress(steps[idx].p, steps[idx].s);
      addLog(steps[idx].s);
      idx++;
    } else {
      clearInterval(interval);
      setTimeout(() => {
        setPlayButtonMode("running");
        progressContainer.style.display = "none";
        addLog("Игра успешно запущена!", "success");
      }, 1000);
    }
  }, 800);
}

function parseServerAddr(addr) {
  const parts = addr.split(":");
  return {
    ip: parts[0] || serverData.ip,
    port: parts[1] ? parseInt(parts[1], 10) || 25565 : 25565
  };
}

function pingCurrentServer() {
  if (!hasTauri || !window.__TAURI__.core) return;
  window.__TAURI__.core.invoke("check_server_status", { ip: serverData.ip, port: serverData.port })
    .catch(() => window.__TAURI__.core.invoke("check_server_status", { ip: serverData.fallbackIp, port: serverData.fallbackPort }))
    .then((status) => {
      serverMeta.textContent = `🟢 Онлайн: ${status.online_players}/${status.max_players} | Пинг: ${status.ping_ms} ms`;
    })
    .catch(() => {
      serverMeta.textContent = `🟡 Сервер запускается или спит`;
    });
}

function refreshManifestMetadata() {
  if (!hasTauri || !window.__TAURI__.core) return;
  const manifestUrl = settingManifestUrl.value.trim();
  if (!manifestUrl) return;
  window.__TAURI__.core.invoke("fetch_manifest_info", { manifestUrl, mirrorSource: currentMirror })
    .then((info) => {
      const loaderLabel = info.loader_type.charAt(0).toUpperCase() + info.loader_type.slice(1);
      requiredJavaMajor = inferRequiredJavaFromMc(info.minecraft_version);
      currentPackDesc = `${info.minecraft_version} ${loaderLabel}`;
      updateJavaRequirementLabels();
      if (versionBadgeEl) {
        const sizeLabel = info.total_pack_bytes ? ` · ${formatBytes(info.total_pack_bytes)}` : "";
        versionBadgeEl.textContent = info.pack_version
          ? `${info.minecraft_version} ${loaderLabel} · ${info.pack_version}${sizeLabel}`
          : `${info.minecraft_version} ${loaderLabel}${sizeLabel}`;
      }
      if (info.total_pack_bytes != null) {
        const freePart = info.free_disk_bytes != null ? `, свободно: ${formatBytes(info.free_disk_bytes)}` : "";
        addLog(`Сборка: ${formatBytes(info.total_pack_bytes)}, нужно скачать: ${formatBytes(info.needed_bytes || 0)}${freePart}`);
        if (info.enough_disk_space === false) {
          addLog(`⚠ Недостаточно свободного места на диске! Нужно ${formatBytes(info.needed_bytes)}, доступно ${formatBytes(info.free_disk_bytes)}`, "error");
        }
      }
      if (serverTitleEl && info.server_name) {
        serverTitleEl.textContent = info.server_name;
      }
      if (info.server_ip) {
        if (serverIpText) serverIpText.textContent = info.server_ip;
        const parsed = parseServerAddr(info.server_ip);
        serverData.ip = parsed.ip;
        serverData.port = parsed.port;
        serverData.fallbackIp = parsed.ip;
        serverData.fallbackPort = parsed.port;
        pingCurrentServer();
      }
    })
    .catch((err) => {
      addLog(`Не удалось получить метаданные манифеста: ${err}`, "info");
    });
}

// Initial Load
function initLauncher() {
  addLog("Интерфейс загружен (Tauri: " + hasTauri + ")");

  // Restore RAM
  try {
    const savedRam = localStorage.getItem("astral_ram");
    setRamGb(savedRam || 6, false);

    // Restore Offline Nick
    const savedNick = localStorage.getItem("astral_nick");
    if (savedNick) {
      offlineNickInput.value = savedNick;
    }

    // Restore Tab
    const savedTab = localStorage.getItem("astral_tab");
    switchAuthTab(savedTab === "offline" ? "offline" : "elyby", false);

    // Restore Ely.by Account
    const savedElyby = localStorage.getItem("astral_elyby");
    if (savedElyby) {
      try {
        setElybyUser(JSON.parse(savedElyby));
      } catch (_) {}
    }

    // Restore Settings
    const savedMirror = localStorage.getItem("astral_mirror");
    setMirrorSource(savedMirror || "github", false);

    const savedManifestUrl = localStorage.getItem("astral_manifest_url");
    if (savedManifestUrl && !savedManifestUrl.includes("YourGithubUsername")) {
      settingManifestUrl.value = savedManifestUrl;
    } else if (currentMirror === "gitlab") {
      settingManifestUrl.value = convertManifestMirrorUrl(settingManifestUrl.value, "gitlab");
    }

    const savedJavaPath = localStorage.getItem("astral_java_path");
    if (savedJavaPath) settingJavaPath.value = savedJavaPath;
    const savedStrictJava = localStorage.getItem("astral_strict_java");
    if (settingStrictJava && savedStrictJava !== null) {
      settingStrictJava.checked = savedStrictJava !== "0";
    }

    const savedLogsHeight = parseInt(localStorage.getItem("astral_logs_height"), 10);
    if (!Number.isNaN(savedLogsHeight) && savedLogsHeight >= 130) {
      setLogsHeight(savedLogsHeight, false);
    }

    updateJavaRequirementLabels();
  } catch (e) {
    addLog("Ошибка чтения localStorage: " + e, "error");
  }

  // Listen to Tauri progress events if in Tauri
  if (hasTauri && window.__TAURI__.event) {
    window.__TAURI__.event.listen("java-progress", (event) => {
      const payload = event.payload;
      updateDynamicIsland(payload);
      if (payload && payload.stage === "done") {
        addLog(`✓ ${payload.status || "Java готова"}`, "success");
        scanSystemJavas();
      } else if (payload && payload.stage === "error") {
        addLog(`✗ ${payload.status || "Ошибка загрузки Java"}`, "error");
      }
    });

    window.__TAURI__.event.listen("download-progress", (event) => {
      const payload = event.payload;
      setProgress(payload.percent, payload.status);
      if (payload.percent >= 95 && progressDetails) {
        progressDetails.classList.add("hidden");
      }
      if (payload.log) {
        addLog(payload.log);
      }
    });

    window.__TAURI__.event.listen("download-stats", (event) => {
      const s = event.payload;
      if (!progressDetails) return;
      progressContainer.style.display = "flex";
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

    window.__TAURI__.event.listen("game-log", (event) => {
      addLog(event.payload, "info");
    });

    window.__TAURI__.event.listen("game-started", () => {
      setPlayButtonMode("running");
    });

    window.__TAURI__.event.listen("game-stopped", () => {
      setPlayButtonMode("idle");
      progressContainer.style.display = "none";
    });

    // Check Server Status via Tauri (with fallback to direct node IP:port)
    pingCurrentServer();
    refreshManifestMetadata();
  } else {
    serverMeta.textContent = `🟢 Онлайн: 12/50 | Пинг: 24 ms`;
  }
}

function runSplashIntro() {
  const splash = document.getElementById("splash-screen");
  if (!splash) {
    document.body.classList.remove("splash-active");
    return;
  }
  let finished = false;
  const finishSplash = () => {
    if (finished) return;
    finished = true;
    splash.classList.add("splash-exit");
    document.body.classList.remove("splash-active");
    setTimeout(() => {
      splash.classList.add("hidden");
    }, 520);
  };

  setTimeout(() => {
    splash.classList.add("splash-reveal");
  }, 460);

  setTimeout(finishSplash, 1850);
  splash.addEventListener("click", finishSplash, { once: true });
}

if (document.readyState === "loading") {
  window.addEventListener("DOMContentLoaded", () => {
    runSplashIntro();
    initLauncher();
  });
} else {
  runSplashIntro();
  initLauncher();
}
