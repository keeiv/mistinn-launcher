// 霧宿啟動器 update window: shows which release is out against the running one, and runs the
// update (download, checked against its SHA-256, swapped in, restarted) with the bell's animation.
// Opened in a plain browser (design checks), a stand-in plays the whole update with sample data.

const tauri = window.__TAURI__;
const invoke = tauri ? tauri.core.invoke : mockInvoke;
const listen = tauri ? tauri.event.listen : mockListen;

const $ = (id) => document.getElementById(id);
const STAGES = {
  install: "替換成新版本…",
  installer: "正在開啟安裝程式…",
  restart: "完成，正在重新啟動…",
};

// A release and its source commit, as one short line.
function describe(tag, commit) {
  return `${tag || "未知版本"}（${commit ? commit.slice(0, 7) : "—"}）`;
}

// Megabytes with one decimal, for the download line.
function mb(bytes) {
  return (bytes / 1048576).toFixed(1);
}

// Shows a step of the update, moving the bar and the bell along with it.
function showProgress({ stage, done, total }) {
  $("progress").hidden = false;
  if (stage === "download") {
    const pct = total ? Math.min(100, Math.round((done / total) * 100)) : 0;
    $("fill").style.width = pct + "%";
    $("status").textContent = `下載中 ${mb(done)} / ${mb(total)} MB，下載後核對 SHA-256`;
    return;
  }
  $("fill").style.width = "100%";
  $("status").textContent = STAGES[stage] || stage;
  if (stage === "restart" || stage === "installer") {
    document.body.classList.remove("working");
    document.body.classList.add("done");
    $("title").textContent = "更新完成";
  }
}

// Shows what went wrong and lets the player try again or close the window.
function showError(message) {
  document.body.classList.remove("working");
  $("error").hidden = false;
  $("error").textContent = `更新失敗：${message}`;
  $("start").textContent = "再試一次";
  $("actions").hidden = false;
}

// Runs the update; the window closes with the launcher once the new one starts.
async function start() {
  $("error").hidden = true;
  $("actions").hidden = true;
  document.body.classList.add("working");
  showProgress({ stage: "download", done: 0, total: 0 });
  try {
    await invoke("update_apply");
  } catch (err) {
    showError(String(err));
  }
}

// Closes this window and leaves the launcher as it is.
function later() {
  if (tauri) {
    tauri.window.getCurrentWindow().close();
  } else {
    window.close();
  }
}

// Fills the window with the release that is out, or says there is nothing to do.
async function main() {
  const found = await invoke("update_info");
  if (!found) {
    $("title").textContent = "已是最新版本";
    $("start").hidden = true;
    $("later").textContent = "關閉";
  } else {
    $("name").textContent = found.name;
    $("current").textContent = describe(found.current_tag, found.current_commit);
    $("latest").textContent = describe(found.tag, found.commit);
  }
  $("start").addEventListener("click", start);
  $("later").addEventListener("click", later);
  listen("update-progress", ({ payload }) => showProgress(payload));
}

// Sample answers for a design check in a plain browser.
function mockInvoke(command) {
  if (command === "update_info") {
    return Promise.resolve({
      tag: "v0.1.1-mod.0.1.0",
      name: "霧宿啟動器 0.1.1（霧宿：追放 0.1.0）",
      commit: "3c03fe0e1a9c4a8b2f7d6e5c4b3a29180f7e6d5c",
      current_tag: "v0.1.0-mod.0.1.0",
      current_commit: "a457322676f1fb8d370b7c24fb554db1184f7016",
    });
  }
  if (command === "update_apply") {
    const total = 27301888;
    const steps = [
      ...Array.from({ length: 12 }, (_, i) => ({ stage: "download", done: (total * (i + 1)) / 12, total })),
      { stage: "install" },
    ];
    steps.push({ stage: "restart" });
    steps.forEach((step, i) => setTimeout(() => mockHandlers.forEach((h) => h({ payload: step })), 400 * (i + 1)));
    return new Promise(() => {});
  }
  return Promise.resolve(null);
}

const mockHandlers = [];

// Stand-in for Tauri's event listener in a plain browser.
function mockListen(_event, handler) {
  mockHandlers.push(handler);
  return Promise.resolve();
}

main();
