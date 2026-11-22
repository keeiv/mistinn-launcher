//! Accounts. Microsoft sign-in uses the device code flow (the player signs in on Microsoft's own
//! page, the launcher never sees a password), then Xbox Live, XSTS and Minecraft services, as
//! documented by the community at minecraft.wiki ("Microsoft Authentication Scheme").
//!
//! Microsoft sign-in needs an Azure application ID that Microsoft has approved for the Minecraft
//! API. The launcher reads it from launcher.json ("msa_client_id"). Offline accounts work without it.
//!
//! Saved tokens are encrypted with Windows DPAPI, so they only open for the same Windows user.

use crate::net;
use crate::paths;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Account {
    Microsoft { name: String, uuid: String, access_token: String, refresh_token: String, expires_at: u64 },
    Offline { name: String, uuid: String },
}

impl Account {
    pub fn name(&self) -> &str {
        match self {
            Account::Microsoft { name, .. } | Account::Offline { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LauncherConfig {
    #[serde(default)]
    pub msa_client_id: String,
}

pub fn config() -> LauncherConfig {
    std::fs::read_to_string(paths::config_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

// ── Storage ──────────────────────────────────────────────────────────

pub fn load() -> Vec<Account> {
    let Ok(bytes) = std::fs::read(paths::accounts_file()) else { return vec![] };
    let Ok(plain) = unprotect(&bytes) else { return vec![] };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn save(accounts: &[Account]) -> Result<()> {
    std::fs::create_dir_all(paths::root())?;
    let plain = serde_json::to_vec(accounts)?;
    std::fs::write(paths::accounts_file(), protect(&plain)?)?;
    Ok(())
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Result<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input points at `data` for the call; output is freed with LocalFree below
    unsafe {
        CryptProtectData(&input, None, None, None, None, 0, &mut output)?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as _)));
        Ok(bytes)
    }
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as in protect()
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, 0, &mut output)?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as _)));
        Ok(bytes)
    }
}

#[cfg(not(windows))]
fn protect(_: &[u8]) -> Result<Vec<u8>> {
    bail!("saving accounts is only supported on Windows for now")
}

#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>> {
    bail!("saving accounts is only supported on Windows for now")
}

pub fn offline(name: &str) -> Result<Account> {
    let valid = (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        bail!("a name is 3-16 letters, digits or underscores");
    }
    Ok(Account::Offline { name: name.to_string(), uuid: uuid::Uuid::new_v4().simple().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_offline_account_with_a_valid_name() {
        let Account::Offline { name, uuid } = offline("New_Hunter25").unwrap() else {
            panic!("expected an offline account");
        };
        assert_eq!(name, "New_Hunter25");
        assert!(uuid::Uuid::parse_str(&uuid).is_ok());
    }

    #[test]
    fn rejects_invalid_offline_names() {
        for name in ["", "ab", "seventeen_letters", "new hunter", "獵人_25"] {
            assert_eq!(offline(name).unwrap_err().to_string(), "a name is 3-16 letters, digits or underscores");
        }
    }
}

// ── Microsoft sign-in ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
}

fn client_id() -> Result<String> {
    let id = config().msa_client_id;
    if id.trim().is_empty() {
        bail!("Microsoft sign-in is not set up yet: this launcher's Azure application is waiting for approval");
    }
    Ok(id)
}

pub async fn start_device_code() -> Result<DeviceCode> {
    let response: Value = net::client()
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[("client_id", client_id()?.as_str()), ("scope", "XboxLive.signin offline_access")])
        .send()
        .await?
        .json()
        .await?;
    Ok(serde_json::from_value(response).context("device code response")?)
}

/// Polls until the player finishes signing in on Microsoft's page, then signs in to Minecraft.
pub async fn finish_device_code(code: &DeviceCode) -> Result<Account> {
    let id = client_id()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(code.expires_in);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(code.interval.max(2))).await;
        if std::time::Instant::now() > deadline {
            bail!("the sign-in code expired");
        }
        let response: Value = net::client()
            .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .form(&[
                ("client_id", id.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", code.device_code.as_str()),
            ])
            .send()
            .await?
            .json()
            .await?;
        match response.get("error").and_then(|e| e.as_str()) {
            Some("authorization_pending") | Some("slow_down") => continue,
            Some(other) => bail!("Microsoft sign-in failed: {other}"),
            None => {
                let access = response["access_token"].as_str().context("access token")?;
                let refresh = response["refresh_token"].as_str().unwrap_or_default();
                return minecraft_login(access, refresh).await;
            }
        }
    }
}

/// Renews a Microsoft account's Minecraft token with its saved refresh token.
pub async fn refresh(account: &Account) -> Result<Account> {
    let Account::Microsoft { refresh_token, .. } = account else { return Ok(account.clone()) };
    let response: Value = net::client()
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("client_id", client_id()?.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await?
        .json()
        .await?;
    let access = response["access_token"].as_str().ok_or_else(|| anyhow!("please sign in to Microsoft again"))?;
    let refresh = response["refresh_token"].as_str().unwrap_or(refresh_token);
    minecraft_login(access, refresh).await
}

async fn post_json(url: &str, body: Value) -> Result<Value> {
    let response = net::client().post(url).header("Accept", "application/json").json(&body).send().await?;
    let status = response.status();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        if url.contains("xsts") {
            if let Some(code) = value.get("XErr").and_then(|x| x.as_u64()) {
                bail!("Xbox Live refused the sign-in (XErr {code}): the account may need an Xbox profile or parental consent");
            }
        }
        bail!("{url}: HTTP {status}");
    }
    Ok(value)
}

async fn minecraft_login(ms_access: &str, refresh: &str) -> Result<Account> {
    let xbl = post_json(
        "https://user.auth.xboxlive.com/user/authenticate",
        json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access}") },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }),
    )
    .await?;
    let xbl_token = xbl["Token"].as_str().context("Xbox Live token")?;
    let xsts = post_json(
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }),
    )
    .await?;
    let xsts_token = xsts["Token"].as_str().context("XSTS token")?;
    let uhs = xsts.pointer("/DisplayClaims/xui/0/uhs").and_then(|u| u.as_str()).context("user hash")?;
    let mc = post_json(
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }),
    )
    .await
    .map_err(|e| anyhow!("{e:#} (a 403 here means the launcher's Azure app is not yet approved for Minecraft)"))?;
    let token = mc["access_token"].as_str().context("Minecraft token")?.to_string();
    let expires_in = mc["expires_in"].as_u64().unwrap_or(86400);
    let profile = net::client().get("https://api.minecraftservices.com/minecraft/profile").bearer_auth(&token).send().await?;
    if profile.status().as_u16() == 404 {
        bail!("this Microsoft account does not own Minecraft: Java Edition");
    }
    let profile: Value = profile.json().await?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    Ok(Account::Microsoft {
        name: profile["name"].as_str().unwrap_or_default().to_string(),
        uuid: profile["id"].as_str().unwrap_or_default().to_string(),
        access_token: token,
        refresh_token: refresh.to_string(),
        expires_at: now + expires_in,
    })
}
