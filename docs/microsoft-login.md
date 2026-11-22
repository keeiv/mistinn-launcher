# 開通微軟登入

第三方啟動器不能借用官方啟動器的身分登入。它必須有自己的 Azure 應用程式，而且新的應用程式要**另外向微軟申請 Minecraft API 的使用權限**。在通過之前，登入流程會在最後一步 `login_with_xbox` 收到 HTTP 403。這是微軟的規定，不是程式錯誤。

這件事只有啟動器的擁有者能做。步驟如下。

## 1. 註冊 Azure 應用程式

1. 用你的微軟帳號登入 <https://portal.azure.com>，前往 **Microsoft Entra ID → 應用程式註冊 → 新增註冊**。
2. 名稱填「霧宿啟動器」（或你想給玩家看到的名稱）。
3. **支援的帳戶類型**：選「僅限個人 Microsoft 帳戶」。
4. 重新導向 URI 留空，按下「註冊」。
5. 進入剛建好的應用程式 → **驗證** → 最下面的「允許公用用戶端流程」設為**是**，然後儲存。啟動器用的是裝置代碼流程，需要這個設定。
6. 在「概觀」頁複製 **應用程式 (用戶端) 識別碼**。這串 ID 不算機密，但仍請保管好。

## 2. 申請 Minecraft API 權限

1. 開啟微軟提供的申請表單：<https://aka.ms/mce-reviewappid>。
2. 填入剛才的應用程式識別碼，並說明用途：一個用來啟動《霧宿：追放》與一般 Minecraft Java 版的啟動器，會使用 Xbox Live 登入取得 Minecraft 存取權杖。
3. 送出後等待審核。審核需要多久沒有公開說明，社群裡有人等了幾天，也有人等得更久。

## 3. 把 ID 交給啟動器

審核通過後，編輯玩家電腦上的 `%APPDATA%\.mistinn\launcher.json`：

```json
{
  "msa_client_id": "貼上你的應用程式識別碼"
}
```

發佈給玩家的版本，可以在打包前把這個檔案的預設內容做進安裝程式，或之後改成在建置時寫入程式裡。重新開啟啟動器後，「微軟帳號登入」按鈕就會啟用。

## 登入怎麼運作

1. 啟動器向微軟要一組**裝置代碼**，並在畫面上顯示網址和代碼。
2. 玩家用瀏覽器打開微軟的網頁輸入代碼並登入。**啟動器從頭到尾都不會經手密碼。**
3. 啟動器依序換取 Xbox Live、XSTS 與 Minecraft 的權杖，並確認帳號擁有 Java 版。
4. 權杖用 Windows DPAPI 加密後存在 `accounts.dat`，只有同一個 Windows 使用者解得開。

## 離線帳號

在啟動器輸入角色名稱，按「加入離線帳號」就能使用。離線帳號可在本機遊玩並自訂皮膚，無須設定 `launcher.json`。

參考資料：[Microsoft Authentication Scheme（minecraft.wiki）](https://minecraft.wiki/w/Minecraft_Wiki:Projects/wiki.vg_merge/Microsoft_Authentication_Scheme)
