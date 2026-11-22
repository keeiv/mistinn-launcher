// 霧宿啟動器 front end. Talks to the Rust side through Tauri's global API; when opened in a plain
// browser (design checks), a small stand-in answers with sample data.

const tauri = window.__TAURI__;
const invoke = tauri ? tauri.core.invoke : mockInvoke;
const listen = tauri ? tauri.event.listen : () => Promise.resolve();

const $ = (id) => document.getElementById(id);
const PRESET_NAMES = { smooth: "流暢", normal: "普通", ultra: "超高" };
const GRAPHICS = { fast: "流暢畫質", fancy: "精緻畫質", fabulous: "極致畫質" };
const STAGES = {
  version: "讀取版本資訊",
  java: "準備 Java",
  libraries: "下載函式庫",
  assets: "下載遊戲資源",
  mods: "安裝模組",
  start: "啟動遊戲",
};

let state = null;
let preset = "normal";
let account = 0;
let accounts = [];
/** The offline account whose skin panel is open, or -1. */
let skinFor = -1;

// Megabytes as gigabytes, for the hardware line and the presets.
function gb(mb) {
  return (mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1) + " GB";
}

// Draws the three ways to play, the chosen one checked.
function renderPresets() {
  const box = $("presets");
  box.replaceChildren();
  for (const s of state.presets) {
    const button = document.createElement("button");
    button.className = "preset";
    button.setAttribute("role", "radio");
    button.setAttribute("aria-checked", String(s.preset === preset));
    const lines = [`記憶體 ${gb(s.memory_mb)}`, `視距 ${s.render_distance} · ${GRAPHICS[s.graphics]}`, s.shaders ? "光影開啟" : "光影關閉"];
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

// Draws the accounts, the chosen one marked, and keeps `list` as the current accounts.
function renderAccounts(list) {
  accounts = list;
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
    row.innerHTML =
      `<canvas class="face" width="8" height="8" aria-hidden="true"></canvas><span class="name"></span>` +
      (a.kind === "offline" ? `<button class="skin" title="離線皮膚">皮膚</button>` : "") +
      `<span class="kind">${a.kind === "microsoft" ? "微軟" : "離線"}</span><button class="remove" title="移除">×</button>`;
    row.querySelector(".name").textContent = a.name;
    drawFace(row.querySelector(".face"), a.skin);
    const skinButton = row.querySelector(".skin");
    if (skinButton) {
      skinButton.addEventListener("click", (e) => {
        e.stopPropagation();
        openSkin(skinFor === i ? -1 : i);
      });
    }
    row.addEventListener("click", () => {
      account = i;
      renderAccounts(list);
    });
    row.querySelector(".remove").addEventListener("click", async (e) => {
      e.stopPropagation();
      const next = await invoke("remove_account", { index: i });
      account = 0;
      openSkin(-1);
      renderAccounts(next);
    });
    box.append(row);
  });
  $("play").disabled = !list.length;
}

// ── Offline skins ─────────────────────────────────────────────────────
// The game picks an offline player's default skin, and with it wide or slim arms, from the
// account's id, so the launcher keeps both versions of the player's skin: the arms are widened
// or narrowed from the one they drew. Old 64x32 skins are brought up to 64x64 first.

/** Draws a skin's face (with its hat layer) into an 8x8 canvas; the default face when none. */
function drawFace(canvas, dataUrl) {
  const g = canvas.getContext("2d");
  g.clearRect(0, 0, 8, 8);
  if (!dataUrl) {
    g.fillStyle = "#6b5b3a";
    g.fillRect(0, 0, 8, 8);
    g.fillStyle = "#2a2219";
    g.fillRect(2, 3, 1, 1);
    g.fillRect(5, 3, 1, 1);
    return;
  }
  const img = new Image();
  img.onload = () => {
    g.drawImage(img, 8, 8, 8, 8, 0, 0, 8, 8);
    g.drawImage(img, 40, 8, 8, 8, 0, 0, 8, 8);
  };
  img.src = dataUrl;
}

// Opens the skin panel for an offline account, or closes it when there is none.
function openSkin(index) {
  skinFor = index;
  const a = accounts[index];
  $("skin-panel").hidden = !a;
  if (!a) return;
  $("skin-title").textContent = `${a.name} 的離線皮膚`;
  document.querySelector(`input[name="arms"][value="${a.slim_arms ? "slim" : "wide"}"]`).checked = true;
  drawFace($("skin-face"), a.skin);
  $("skin-clear").disabled = !a.skin;
}

// Reads an image file the player picked into an Image element.
function loadImage(file) {
  return new Promise((resolve, reject) => {
    const fail = () => reject(new Error("無法讀取這張圖片。"));
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = fail;
    // The launcher window's image policy allows data: URLs.
    const reader = new FileReader();
    reader.onload = () => {
      img.src = reader.result;
    };
    reader.onerror = fail;
    reader.readAsDataURL(file);
  });
}

/** The skin as 64x64 pixels: an old 64x32 skin gets its left arm and leg from the right ones. */
function toModern(img) {
  if (img.width !== 64 || (img.height !== 64 && img.height !== 32)) {
    throw new Error("皮膚必須是 64×64 或 64×32 的 PNG。");
  }
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const g = canvas.getContext("2d");
  g.drawImage(img, 0, 0);
  if (img.height === 32) {
    g.drawImage(img, 0, 16, 16, 16, 16, 48, 16, 16);
    g.drawImage(img, 40, 16, 16, 16, 32, 48, 16, 16);
  }
  return canvas;
}

// Where each arm's faces are, for both layers: [x, y] of the 16x16 block each lives in.
const ARMS = [
  [40, 16],
  [40, 32],
  [32, 48],
  [48, 48],
];

/**
 * Converts the arms of a skin between wide (4 pixels) and slim (3 pixels): each face is copied
 * column by column, a column repeated to widen it or dropped to narrow it.
 */
function convertArms(source, toSlim) {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const g = canvas.getContext("2d");
  g.drawImage(source, 0, 0);
  for (const [bx, by] of ARMS) {
    g.clearRect(bx, by, 16, 16);
    // [x in the wide layout, x in the slim layout, top row, height, width wide, width slim]
    const faces = [
      [4, 4, 0, 4, 4, 3], // top
      [8, 7, 0, 4, 4, 3], // bottom
      [0, 0, 4, 12, 4, 4], // outer side
      [4, 4, 4, 12, 4, 3], // front
      [8, 7, 4, 12, 4, 4], // inner side
      [12, 11, 4, 12, 4, 3], // back
    ];
    for (const [wx, sx, top, height, ww, sw] of faces) {
      const [fromX, toX, fromW, toW] = toSlim ? [wx, sx, ww, sw] : [sx, wx, sw, ww];
      for (let c = 0; c < toW; c++) {
        // spread the columns: the last one repeats when widening, the third goes when narrowing
        const src = Math.min(fromW - 1, toW > fromW && c === toW - 1 ? fromW - 1 : toW < fromW && c >= 2 ? c + 1 : c);
        g.drawImage(source, bx + fromX + src, by + top, 1, height, bx + toX + c, by + top, 1, height);
      }
    }
  }
  return canvas;
}

// A canvas as PNG bytes, the way the Rust side takes them.
function pngBytes(canvas) {
  const data = atob(canvas.toDataURL("image/png").split(",")[1]);
  return Array.from(data, (c) => c.charCodeAt(0));
}

// Saves a skin in both arm widths for the account whose panel is open.
async function saveSkin(canvas, slimArms) {
  const wide = slimArms ? convertArms(canvas, false) : canvas;
  const slim = slimArms ? canvas : convertArms(canvas, true);
  const list = await invoke("set_skin", { index: skinFor, wide: pngBytes(wide), slim: pngBytes(slim), slimArms });
  const keep = skinFor;
  renderAccounts(list);
  openSkin(keep);
}

/** The skin drawn for the arms now chosen, from the stored versions. */
async function currentSkin() {
  const a = accounts[skinFor];
  if (!a || !a.skin) return null;
  const img = new Image();
  await new Promise((resolve) => {
    img.onload = resolve;
    img.src = a.skin;
  });
  return toModern(img);
}

// Shows the note that a version other than 霧宿's plays without the mod.
function onVersionChange() {
  const v = $("version").value;
  $("version-note").hidden = v === state.mistinn_version;
}

// Shows an error under the play button, or hides it for an empty message.
function showError(message) {
  $("error").hidden = !message;
  $("error").textContent = message || "";
}

// Starts the game with the chosen version, way to play and account, showing progress until it
// closes.
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

// Runs a Microsoft sign-in: shows the code, waits for the player, adds the account.
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

// Loads the launcher's state and wires up the window.
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

  $("offline-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    try {
      renderAccounts(await invoke("add_offline", { name: $("offline-name").value }));
      $("offline-name").value = "";
    } catch (err) {
      showError(String(err));
    }
  });

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
  } catch {
    showError("無法取得版本清單，請檢查網路。");
  }
  select.addEventListener("change", onVersionChange);
  onVersionChange();

  $("play").addEventListener("click", play);
  $("ms-login").addEventListener("click", microsoftLogin);
  $("open-folder").addEventListener("click", () => invoke("open_instance", { version: $("version").value }));
  $("skin-pick").addEventListener("click", () => $("skin-file").click());
  $("skin-file").addEventListener("change", async () => {
    const file = $("skin-file").files[0];
    $("skin-file").value = "";
    if (!file) return;
    try {
      showError("");
      const slimArms = document.querySelector('input[name="arms"]:checked').value === "slim";
      await saveSkin(toModern(await loadImage(file)), slimArms);
    } catch (err) {
      showError(err.message || String(err));
    }
  });
  for (const radio of document.querySelectorAll('input[name="arms"]')) {
    // the same picture, now read as drawn for the other arms
    radio.addEventListener("change", async () => {
      try {
        const skin = await currentSkin();
        if (skin) await saveSkin(skin, radio.value === "slim");
      } catch (err) {
        showError(err.message || String(err));
      }
    });
  }
  $("skin-clear").addEventListener("click", async () => {
    const keep = skinFor;
    renderAccounts(await invoke("clear_skin", { index: skinFor }));
    openSkin(keep);
  });
  $("toggle-log").addEventListener("click", () => ($("log").hidden = !$("log").hidden));

  listen("progress", ({ payload }) => {
    const pct = payload.total ? Math.round((payload.done / payload.total) * 100) : 0;
    $("bar-fill").style.width = pct + "%";
    $("progress-text").textContent =
      `${STAGES[payload.stage] || payload.stage} ${payload.total > 1 ? `${payload.done}/${payload.total}` : ""}`;
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
      mistinn_version: "26.3",
      launcher_version: "0.1.0",
    },
    versions: ["26.3", "26.2", "26.1.2", "1.21.11", "1.20.1"],
  };
  const mock = answers.state.accounts;
  if (command === "set_skin") {
    const bytes = new Uint8Array(arguments[1].slimArms ? arguments[1].slim : arguments[1].wide);
    mock[arguments[1].index].skin = "data:image/png;base64," + btoa(String.fromCharCode(...bytes));
    mock[arguments[1].index].slim_arms = arguments[1].slimArms;
    return Promise.resolve(mock);
  }
  if (command === "clear_skin") {
    mock[arguments[1].index].skin = null;
    return Promise.resolve(mock);
  }
  return Promise.resolve(answers[command]);
}

main().catch((e) => showError(String(e)));
