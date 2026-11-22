# 霧宿啟動器（Mistinn Launcher）

為《霧宿：追放》準備好一切的 Minecraft 啟動器。按下「出發」，它會下載遊戲、Java、Fabric、本模組與一整套優化模組，並依你的電腦調好設定。

## 三種模式

啟動器會讀取記憶體、CPU 執行緒和顯示卡（Windows 上透過 DXGI 讀取專用顯示記憶體），替三種模式各自算出適合的數值，並標出建議的那一個。

| | 流暢 | 普通 | 超高 |
|---|---|---|---|
| 記憶體 | 約 1/4，最多 4 GB | 約 1/3，最多 6 GB | 約 2/5，最多 8 GB |
| 視距（弱／中／強顯卡） | 6／8／10 | 8／12／14 | 12／16／24 |
| 畫質 | 流暢 | 精緻 | 極致（弱顯卡改為精緻） |
| 粒子、雲 | 最少、關 | 減少、快速 | 全部、精緻 |
| 光影 | 關 | 關 | 顯示記憶體 6 GB 以上時開啟 Iris 與 Complementary Reimagined |

- 記憶體至少留 3 GB 給系統，遊戲至少 2 GB。
- 模擬距離依 CPU 執行緒數設上限。
- 只會改這幾項畫面設定，你在遊戲裡調整的其他選項（按鍵、音量、視野）都會保留。
- 這些是起始值，不保證特定的幀數。

## 模組

依主流的 Fabric 優化包（Fabulously Optimized 等）挑選，每次啟動時都從 Modrinth 取得對應版本的檔案，並用 SHA-1 檢查。

- **一律加入**：Fabric API、Sodium、Sodium Extra、Reese's Sodium Options、Lithium、FerriteCore、ImmediatelyFast、EntityCulling、Dynamic FPS、More Culling、AppleSkin、Mod Menu。
- **超高（強顯卡）**：Iris 與 Complementary Reimagined 光影包。
- **其他版本（一般遊玩）**：再加上 Xaero 小地圖、Xaero 世界地圖、Jade、Mouse Tweaks。
- **霧宿：追放（26.3）**：不另外加小地圖和背包類模組，因為本模組已經內建小地圖和格子背包。

某個模組在你選的版本沒有對應檔案時，會略過並記在遊戲紀錄裡。啟動器只管理自己放進去的模組，你自己放進 `mods` 的檔案不會被刪掉。

## 其他版本

版本選單裡可以選任何正式版。選了 26.3 以外的版本時，啟動器會提醒你**這個版本沒有支援《霧宿：追放》**，改用一般模式遊玩。Fabric 沒有支援的舊版本則以原版啟動。

## 帳號

所有玩家都可以直接加入離線帳號，在本機遊玩並自訂皮膚。角色名稱需使用 3 至 16 個英文字母、數字或底線。

微軟登入使用裝置代碼流程，啟動器不會經手密碼，權杖用 Windows DPAPI 加密保存。**微軟登入需要先申請**，步驟見 [docs/microsoft-login.md](docs/microsoft-login.md)。

## 資料位置

全部放在 `%APPDATA%\.mistinn`：`versions`、`libraries`、`assets`、`runtimes`（Java）、`instances`（每個版本各自的遊戲資料夾）。

## 建置

需要 Rust（MSVC 工具鏈）、Node.js 和 Visual Studio Build Tools。

```powershell
.\build.ps1 test      # 單元測試
.\build.ps1 dev       # 開發模式
.\build.ps1 build     # 產生安裝程式（NSIS）
```

美術由 `tools/make_art.py` 產生：`python tools/make_art.py`。

自動檢查：`mistinn-launcher.exe --selftest <版本> <smooth|normal|ultra> <角色名稱> [霧宿模組 jar]` 會在不開視窗的情況下，完整安裝並啟動一次遊戲。

## 授權與致謝

- 像素字型：[Fusion Pixel Font](https://github.com/TakWolf/fusion-pixel-font)，SIL Open Font License 1.1（`src/assets/fonts/Fusion-Pixel-LICENSE.txt`）。
- 模組由 Modrinth 上的原作者發佈，啟動器只在玩家電腦上下載，不重新散佈。
- 啟動器本身的授權尚未決定。
