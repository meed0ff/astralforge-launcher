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
const logsDrawer = document.getElementById("logs-drawer");
const logsContent = document.getElementById("logs-content");

const btnSettings = document.getElementById("btn-settings");
const settingsModal = document.getElementById("settings-modal");
const btnModalClose = document.getElementById("btn-modal-close");
const btnSaveSettings = document.getElementById("btn-save-settings");
const settingManifestUrl = document.getElementById("setting-manifest-url");
const settingJavaPath = document.getElementById("setting-java-path");
const btnOpenGameFolder = document.getElementById("btn-open-game-folder");

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
    toast.remove();
  }, 4500);
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

// Tabs switching
tabElyby.addEventListener("click", () => {
  currentTab = "elyby";
  tabElyby.classList.add("active");
  tabOffline.classList.remove("active");
  contentElyby.classList.remove("hidden");
  contentOffline.classList.add("hidden");
  localStorage.setItem("astral_tab", "elyby");
});

tabOffline.addEventListener("click", () => {
  currentTab = "offline";
  tabOffline.classList.add("active");
  tabElyby.classList.remove("active");
  contentOffline.classList.remove("hidden");
  contentElyby.classList.add("hidden");
  localStorage.setItem("astral_tab", "offline");
});

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
  elybyLoggedIn.classList.add("hidden");
  elybyLoggedOut.classList.remove("hidden");
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
  elybyLoggedOut.classList.add("hidden");
  elybyLoggedIn.classList.remove("hidden");
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

// Settings & Logs Drawers
btnLogsToggle.addEventListener("click", () => logsDrawer.classList.toggle("open"));
btnLogsClose.addEventListener("click", () => logsDrawer.classList.remove("open"));

btnSettings.addEventListener("click", () => settingsModal.classList.remove("hidden"));
btnModalClose.addEventListener("click", () => settingsModal.classList.add("hidden"));

btnSaveSettings.addEventListener("click", () => {
  setRamGb(ramInput ? ramInput.value : currentRamGb);
  localStorage.setItem("astral_manifest_url", settingManifestUrl.value);
  localStorage.setItem("astral_java_path", settingJavaPath.value);
  settingsModal.classList.add("hidden");
  addLog("Настройки сохранены.");
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
    javaPath: settingJavaPath.value.trim() || "java",
    manifestUrl: settingManifestUrl.value.trim()
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
  window.__TAURI__.core.invoke("fetch_manifest_info", { manifestUrl })
    .then((info) => {
      const loaderLabel = info.loader_type.charAt(0).toUpperCase() + info.loader_type.slice(1);
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
    if (savedTab === "offline") {
      tabOffline.click();
    } else {
      tabElyby.click();
    }

    // Restore Ely.by Account
    const savedElyby = localStorage.getItem("astral_elyby");
    if (savedElyby) {
      try {
        setElybyUser(JSON.parse(savedElyby));
      } catch (_) {}
    }

    // Restore Settings
    const savedManifestUrl = localStorage.getItem("astral_manifest_url");
    if (savedManifestUrl && !savedManifestUrl.includes("YourGithubUsername")) {
      settingManifestUrl.value = savedManifestUrl;
    }

    const savedJavaPath = localStorage.getItem("astral_java_path");
    if (savedJavaPath) settingJavaPath.value = savedJavaPath;
  } catch (e) {
    addLog("Ошибка чтения localStorage: " + e, "error");
  }

  // Listen to Tauri progress events if in Tauri
  if (hasTauri && window.__TAURI__.event) {
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
      progressContainer.style.display = "block";
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

if (document.readyState === "loading") {
  window.addEventListener("DOMContentLoaded", initLauncher);
} else {
  initLauncher();
}
