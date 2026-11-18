// 霧宿啟動器 front end. Talks to the Rust side through Tauri's global API; when opened in a plain
// browser (design checks), a small stand-in answers with sample data.

const tauri = window.__TAURI__;
const invoke = tauri ? tauri.core.invoke : mockInvoke;
const listen = tauri ? tauri.event.listen : () => Promise.resolve();

const $ = (id) => document.getElementById(id);
const PRESET_NAMES = { smooth: "流暢", normal: "普通", ultra: "超高" };
const GRAPHICS = { fast: "流暢畫質", fancy: "精緻畫質", fabulous: "極致畫質" };
const STAGES = { version: "讀取版本資訊", java: "準備 Java", libraries: "下載函式庫", assets: "下載遊戲資源", mods: "安裝模組", start: "啟動遊戲" };

let state = null;
let preset = "normal";
let account = 0;

function gb(mb) {
  return (mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1) + " GB";
}

function renderPresets() {
  const box = $("presets");
  box.replaceChildren();
  for (const s of state.presets) {
    const button = document.createElement("button");
    button.className = "preset";
    button.setAttribute("role", "radio");
    button.setAttribute("aria-checked", String(s.preset === preset));
    const lines = [
      `記憶體 ${gb(s.memory_mb)}`,
      `視距 ${s.render_distance} · ${GRAPHICS[s.graphics]}`,
      s.shaders ? "光影開啟" : "光影關閉",
    ];
    button.innerHTML = `<span class="tag">${s.preset === state.recommended ? "建議" : ""}</span>
      <span class="name">${PRESET_NAMES[s.preset]}</span>
      <span class="detail">${lines.join("<br>")}</span>`;
    button.addEventListener("click", () => {
      preset = s.preset;
      renderPresets();
    });
    box.append(button);
  }
}

function renderAccounts(list) {
  const box = $("accounts");
  box.replaceChildren();
  if (!list.length) {
    const empty = document.createElement("p");
    empty.className = "note";
    empty.textContent = "還沒有帳號。";
    box.append(empty);
  }
  list.forEach((a, i) => {
    const row = document.createElement("div");
    row.className = "account";
    row.setAttribute("role", "radio");
    row.setAttribute("aria-checked", String(i === account));
    row.tabIndex = 0;
    row.innerHTML = `<span></span><span class="kind">${a.kind === "microsoft" ? "微軟" : "離線測試"}</span><button class="remove" title="移除">×</button>`;
    row.firstChild.textContent = a.name;
    row.addEventListener("click", () => {
      account = i;
      renderAccounts(list);
    });
    row.querySelector(".remove").addEventListener("click", async (e) => {
      e.stopPropagation();
      const next = await invoke("remove_account", { index: i });
      account = 0;
      renderAccounts(next);
    });
    box.append(row);
  });
  $("play").disabled = !list.length;
}

function onVersionChange() {
  const v = $("version").value;
  $("version-note").hidden = v === state.mistinn_version;
}

function showError(message) {
  $("error").hidden = !message;
  $("error").textContent = message || "";
}

async function play() {
  showError("");
  $("play").disabled = true;
  $("progress").hidden = false;
  $("progress-text").textContent = "準備中…";
  try {
    await invoke("play", { version: $("version").value, preset, account });
    $("progress-text").textContent = "遊戲已關閉。";
  } catch (e) {
    showError(String(e));
    $("progress").hidden = true;
  } finally {
    $("play").disabled = false;
  }
}

async function microsoftLogin() {
  showError("");
  try {
    const code = await invoke("microsoft_start");
    $("ms-code").hidden = false;
    $("ms-url").textContent = code.verification_uri;
    $("ms-user-code").textContent = code.user_code;
    const list = await invoke("microsoft_finish", { code });
    $("ms-code").hidden = true;
    account = 0;
    renderAccounts(list);
  } catch (e) {
    $("ms-code").hidden = true;
    showError(String(e));
  }
}

async function main() {
  state = await invoke("state");
  preset = state.recommended;
  const hw = state.hardware;
  $("hardware").textContent = `記憶體 ${gb(hw.ram_mb)} · ${hw.threads} 執行緒 · ${hw.gpu}${hw.vram_mb ? `（${gb(hw.vram_mb)}）` : ""}`;
  $("launcher-version").textContent = `v${state.launcher_version}`;
  renderPresets();
  renderAccounts(state.accounts);

  $("ms-login").disabled = !state.microsoft_ready;
  $("ms-note").hidden = state.microsoft_ready;
  $("offline-form").hidden = !state.dev_offline;

  const select = $("version");
  const main = document.createElement("option");
  main.value = state.mistinn_version;
  main.textContent = `${state.mistinn_version} · 霧宿：追放`;
  select.append(main);
  try {
    for (const v of await invoke("versions")) {
      if (v === state.mistinn_version) continue;
      const o = document.createElement("option");
      o.value = v;
      o.textContent = `${v} · 一般遊玩`;
      select.append(o);
    }
  } catch (e) {
    showError("無法取得版本清單，請檢查網路。");
  }
  select.addEventListener("change", onVersionChange);
  onVersionChange();

  $("play").addEventListener("click", play);
  $("ms-login").addEventListener("click", microsoftLogin);
  $("offline-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    try {
      renderAccounts(await invoke("add_offline", { name: $("offline-name").value }));
      $("offline-name").value = "";
    } catch (err) {
      showError(String(err));
    }
  });
  $("open-folder").addEventListener("click", () => invoke("open_instance", { version: $("version").value }));
  $("toggle-log").addEventListener("click", () => ($("log").hidden = !$("log").hidden));

  listen("progress", ({ payload }) => {
    const pct = payload.total ? Math.round((payload.done / payload.total) * 100) : 0;
    $("bar-fill").style.width = pct + "%";
    $("progress-text").textContent = `${STAGES[payload.stage] || payload.stage} ${payload.total > 1 ? `${payload.done}/${payload.total}` : ""}`;
  });
  listen("game-log", ({ payload }) => {
    const log = $("log");
    log.textContent += payload + "\n";
    if (log.textContent.length > 200000) log.textContent = log.textContent.slice(-150000);
    log.scrollTop = log.scrollHeight;
  });
}

// ── Stand-in for design checks in a browser ───────────────────────────
function mockInvoke(command) {
  const presets = [
    { preset: "smooth", memory_mb: 4096, render_distance: 10, graphics: "fast", shaders: false },
    { preset: "normal", memory_mb: 5376, render_distance: 14, graphics: "fancy", shaders: false },
    { preset: "ultra", memory_mb: 6400, render_distance: 24, graphics: "fabulous", shaders: true },
  ];
  const answers = {
    state: {
      hardware: { ram_mb: 16384, threads: 32, gpu: "NVIDIA GeForce RTX 5070 Ti Laptop GPU", vram_mb: 12227 },
      recommended: "ultra",
      presets,
      accounts: [{ name: "Hunter", kind: "offline" }],
      microsoft_ready: false,
      dev_offline: true,
      mistinn_version: "26.3",
      launcher_version: "0.1.0",
    },
    versions: ["26.3", "26.2", "26.1.2", "1.21.11", "1.20.1"],
  };
  return Promise.resolve(answers[command]);
}

main().catch((e) => showError(String(e)));
