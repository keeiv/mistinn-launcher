<div align="center">

[EN](docs/CONTRIBUTING.en.md) | **繁中**

<img src="src-tauri/icons/icon.png" alt="" width="64">

</div>

# 貢獻指南

霧宿啟動器接受錯誤修復、功能、測試與文件更新。一般貢獻使用 Fork 與 Pull Request；維護者也經 Pull Request 合併，`main` 不直接推送。

## 變更範圍

局部修正與文件更新可以直接提交 PR。新增依賴、改變資料位置或檔案格式（`launcher.json`、`accounts.dat`、實例資料夾）、改變三種模式的數值，以及動到登入流程的變更，請先開 Issue 說明目的與相容性，再動手實作。

## 使用 AI 工具

這個專案不排斥 AI。用 AI 工具協助撰寫、重構或找問題的貢獻，和手寫的一視同仁，不需要標註，也不會因此被拒絕。

條件只有一個：**Pull Request 必須沒有安全隱患才會合併。** 送出之前請確認：

- 你讀過並理解送出的每一行，審查時能說明它為什麼這樣寫。
- 啟動器不經手密碼：微軟登入只用裝置代碼流程，權杖只以 DPAPI 加密後存檔，不寫進紀錄。
- 下載的檔案都核對雜湊值（Mojang、Modrinth 用 SHA-1，內嵌模組用 GitHub 記錄的 SHA-256），檔名與路徑不使用未經檢查的輸入。
- `unsafe` 只用在呼叫 Windows API 的地方，每個區塊只做一件事，並寫明為什麼安全（見下方「記憶體」）。
- 新增的依賴是真實存在、仍在維護、沒有已知高風險通報的套件。

註解寫給之後維護的人看：說明程式為什麼這樣做，不要留下與工具的對話或修改過程。

## 開發環境

需要 Rust（MSVC 工具鏈）、Node.js 22、Python 3 和 Visual Studio Build Tools。

```powershell
git clone https://github.com/keeiv/mistinn-launcher.git
cd mistinn-launcher
npm ci
.\build.ps1 test
```

第一次建置會從模組最新的 Release 下載要內嵌的 JAR（`scripts/fetch-mod.ps1`），放在不進版本控制的 `src-tauri/resources/mods`。

## 檢查

CI 在每個 Pull Request 跑四組檢查，全部通過才能合併。提交前請在本機跑一次：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

```powershell
npm run format:check
```

```powershell
npm run lint
```

```powershell
npm run check:comments
```

```powershell
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

```powershell
.\build.ps1 test
```

格式不對時，`cargo fmt --manifest-path src-tauri/Cargo.toml` 與 `npm run format` 會直接改好。

### 程式碼風格

- Rust 用 rustfmt 排版（`rustfmt.toml`，一行最多 140 欄），並通過 Clippy 的 `all` 與 `pedantic` 兩組規則，任何警告都算失敗（`Cargo.toml` 的 `[lints]`）。
- 視窗的程式、樣式與頁面（`src/`）用 Prettier 排版，`main.js` 另外通過 ESLint（`eslint.config.mjs`）。

### 註釋風格

由 `scripts/check_comments.py` 與 Clippy 的文件規則檢查：

- 每個 Rust 檔案開頭有 `//!` 模組說明；模組層級的每個函式、結構、列舉、常數，以及 `impl` 裡的每個 `pub fn`，都有 `///` 文件註解。測試模組不在此限。
- 文件註解的第一段要短，讀起來是完整的句子。
- `main.js` 開頭說明這個檔案是什麼，每個頂層函式上方都有一行註解；`style.css` 開頭也有說明。
- 行註解的 `//` 後面空一格；Rust 不用 `/* */`；註解裡不留 TODO、FIXME、XXX 或 HACK。

### 記憶體

- `unsafe` 在整個程式碼裡是禁止的（`unsafe_code = "deny"`），只有呼叫 Windows API 的函式用 `#[allow(unsafe_code)]` 個別打開，目前只在 `auth.rs`（DPAPI）與 `hardware.rs`（DXGI）。在其他檔案出現 `unsafe`，CI 會失敗。
- 每個 `unsafe` 區塊只含一個不安全的操作，上方用 `// SAFETY:` 寫明為什麼安全；Windows 配置給我們的記憶體一定要釋放，而且只釋放一次。
- 不刻意洩漏記憶體（`mem::forget` 禁用）。
- 依賴每次都用 RustSec 資料庫檢查（`cargo audit`）：有已知漏洞就失敗，不再維護的套件會列出來給審查的人看。

## 提交與 Pull Request

提交訊息與 PR 標題使用 Conventional Commits 前綴，搭配簡潔的繁體中文描述：`feat:`、`fix:`、`docs:`、`style:`、`refactor:`、`test:`、`ci:`、`chore:`。

PR 說明應包含問題、變更後的行為與驗證結果（跑過哪些檢查、看過哪些畫面）。只提交與變更相關的檔案；`node_modules/`、`target/`、`dist/`、`src-tauri/resources/mods/` 與任何金鑰都不得提交。

## 發布

不需要手動發布。`Release` 工作流程每小時檢查一次《霧宿：追放》有沒有新的 Release，有的話就建置一個內嵌新模組的啟動器並發布，tag 為 `v<啟動器版本>-mod.<模組版本>`。啟動器本身的程式改了要出新版時，把 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 與 `package.json` 的版本號一起調高；合併後一小時內會自動發布，也可以到 Actions 手動執行 `Release`。

## 回報問題

請到[森之宿茶室官網](https://moriteahouse.shizune.life/account)回報，或在本倉庫開 Issue，附上啟動器版本、Windows 版本、重現步驟，以及「遊戲紀錄」裡的內容。

## 授權

啟動器以 **AGPL-3.0-only** 授權，完整條文見 [LICENSE](LICENSE)。新貢獻採用相同授權。
