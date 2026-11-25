// AstralForge Launcher Frontend Logic
const hasTauri = typeof window.__TAURI__ !== "undefined";

// State
let currentTab = "elyby"; // "elyby" | "offline"
let elybyAccount = null;
let serverData = {
  ip: "eternum.aurorix.pro",
  port: 25565,
  fallbackIp: "d22.aurorix.net",
  fallbackPort: 25584
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

const ramSlider = document.getElementById("ram-slider");
const ramDisplay = document.getElementById("ram-display");

const btnPlay = document.getElementById("btn-play");
const progressContainer = document.getElementById("progress-container");
const progressFill = document.getElementById("progress-fill");
const progressStatus = document.getElementById("progress-status");
const progressPercent = document.getElementById("progress-percent");

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

// RAM Slider
ramSlider.addEventListener("input", (e) => {
  ramDisplay.textContent = `${e.target.value} GB`;
  localStorage.setItem("astral_ram", e.target.value);
});

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
  localStorage.setItem("astral_manifest_url", settingManifestUrl.value);
  localStorage.setItem("astral_java_path", settingJavaPath.value);
  settingsModal.classList.add("hidden");
  addLog("Настройки сохранены.");
  showToast("Настройки сохранены", "success");
});

btnOpenGameFolder.addEventListener("click", () => {
  if (hasTauri) {
    window.__TAURI__.core.invoke("open_game_folder").catch((err) => {
      showToast(`Не удалось открыть папку: ${err}`, "error");
    });
  } else {
    showToast("Папка игры: ~/.astralforge", "info");
  }
});

// Update Progress Display
function setProgress(percent, status) {
  progressContainer.style.display = "flex";
  progressFill.style.width = `${percent}%`;
  progressPercent.textContent = `${percent}%`;
  progressStatus.textContent = status;
}

// Play Button
btnPlay.addEventListener("click", async () => {
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
    accessToken = elybyAccount.accessToken;
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

  btnPlay.disabled = true;
  addLog(`Подготовка к запуску для ${nickname} (${authType})...`);
  setProgress(5, "Чтение манифеста сборки...");

  const launchRequest = {
    nickname,
    authType,
    uuid,
    accessToken,
    ramGb: parseInt(ramSlider.value),
    javaPath: settingJavaPath.value.trim() || "java",
    manifestUrl: settingManifestUrl.value.trim()
  };

  if (hasTauri) {
    try {
      await window.__TAURI__.core.invoke("start_sync_and_launch", { request: launchRequest });
      btnPlay.disabled = false;
      setTimeout(() => {
        progressContainer.style.display = "none";
      }, 2000);
    } catch (err) {
      addLog(`Ошибка запуска: ${err}`, "error");
      showToast(`Ошибка запуска: ${err}`, "error");
      btnPlay.disabled = false;
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
    { p: 85, s: "Проверка библиотек NeoForge 21.1.267..." },
    { p: 100, s: "Запуск Minecraft 1.21.1!" }
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
        btnPlay.disabled = false;
        progressContainer.style.display = "none";
        addLog("Игра успешно запущена!", "success");
      }, 1000);
    }
  }, 800);
}

// Initial Load
function initLauncher() {
  addLog("Интерфейс загружен (Tauri: " + hasTauri + ")");

  // Restore RAM
  try {
    const savedRam = localStorage.getItem("astral_ram");
    if (savedRam) {
      ramSlider.value = savedRam;
      ramDisplay.textContent = `${savedRam} GB`;
    }

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
    if (savedManifestUrl) settingManifestUrl.value = savedManifestUrl;

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
      if (payload.log) {
        addLog(payload.log);
      }
    });

    window.__TAURI__.event.listen("game-log", (event) => {
      addLog(event.payload, "info");
    });

    // Check Server Status via Tauri (with fallback to direct node IP:port)
    window.__TAURI__.core.invoke("check_server_status", { ip: serverData.ip, port: serverData.port })
      .catch(() => window.__TAURI__.core.invoke("check_server_status", { ip: serverData.fallbackIp, port: serverData.fallbackPort }))
      .then((status) => {
        serverMeta.textContent = `🟢 Онлайн: ${status.online_players}/${status.max_players} | Пинг: ${status.ping_ms} ms`;
      })
      .catch(() => {
        serverMeta.textContent = `🟡 Сервер запускается или спит`;
      });
  } else {
    serverMeta.textContent = `🟢 Онлайн: 12/50 | Пинг: 24 ms`;
  }
}

if (document.readyState === "loading") {
  window.addEventListener("DOMContentLoaded", initLauncher);
} else {
  initLauncher();
}
