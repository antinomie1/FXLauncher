//! Microsoft account sign-in for Minecraft: device-code OAuth → Xbox Live → XSTS → Minecraft services.

use super::client::{DEFAULT_TIMEOUT, DEFAULT_USER_AGENT};
use super::profile::{McProfile, fetch_profile};
use crate::game::account::Account;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Azure application (client) ID of FXLauncher.
pub const CLIENT_ID: &str = "508f8b36-3caa-44cd-be00-3ba2967c541d";
/// Personal Microsoft accounts only: Minecraft cannot be owned by work or school accounts.
const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
/// Verified to sign in with this app ID. Users type the code at `verification_uri` themselves:
/// the `?otc=` prefilled link ends in a "first party application … consent" error.
const SCOPE: &str = "XboxLive.SignIn XboxLive.offline_access";
const XBL_AUTH: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
pub(super) const MC_API: &str = "https://api.minecraftservices.com";

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("网络错误：{0}")]
    Network(String),
    #[error("登录码已过期，请重新登录")]
    Expired,
    #[error("登录已取消")]
    Cancelled,
    #[error("微软账户登录已失效，请重新登录")]
    InvalidGrant,
    #[error("这个微软账户还没有 Xbox 档案，请先登录 xbox.com 创建")]
    NoXboxAccount,
    #[error("Xbox Live 在该账户所在的国家或地区不可用")]
    XboxUnavailable,
    #[error("未成年账户需要家长在 Microsoft 家庭组中允许后才能登录")]
    ChildAccount,
    #[error("Xbox 验证失败（XErr {0}）")]
    Xsts(u64),
    #[error("FXLauncher 还没有获得 Minecraft 服务的使用许可（应用 ID 需经 Mojang 审核）")]
    AppNotApproved,
    #[error("这个账户没有购买 Minecraft: Java 版")]
    NotOwned,
    #[error("还没有创建 Minecraft 档案，请先在 minecraft.net 设置玩家名")]
    NoProfile,
    #[error("{0}")]
    Service(String),
}

/// A pending device-code sign-in: the user enters `user_code` at `verification_uri`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Microsoft OAuth tokens.
#[derive(Debug, Clone, Deserialize)]
pub struct MsToken {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
}

/// Sends a request and returns the status and body; HTTP error statuses are not errors here.
/// Sign-in hosts now and then drop the TLS handshake, so failed connections are retried:
/// nothing has reached the server yet, which keeps this safe for any method.
pub(super) fn send(call: impl Fn() -> Result<ureq::Response, ureq::Error>) -> Result<(u16, String), AuthError> {
    let mut attempt = 1;
    loop {
        match call() {
            Ok(r) => {
                let status = r.status();
                return Ok((status, r.into_string().map_err(|e| AuthError::Network(e.to_string()))?));
            }
            Err(ureq::Error::Status(status, r)) => return Ok((status, r.into_string().unwrap_or_default())),
            Err(ureq::Error::Transport(t)) if attempt < 3 && matches!(t.kind(), ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Dns) => {
                attempt += 1;
                thread::sleep(Duration::from_secs(1));
            }
            Err(e) => return Err(AuthError::Network(e.to_string())),
        }
    }
}

pub(super) fn request(method: &str, url: &str) -> ureq::Request {
    ureq::request(method, url)
        .set("User-Agent", DEFAULT_USER_AGENT)
        .set("Accept", "application/json")
        .timeout(DEFAULT_TIMEOUT)
}

pub(super) fn parse<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, AuthError> {
    serde_json::from_str(body).map_err(|e| AuthError::Service(format!("无法解析服务器响应：{e}")))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Starts a device-code sign-in.
pub fn request_device_code() -> Result<DeviceCode, AuthError> {
    let (status, body) = send(|| request("POST", &format!("{AUTHORITY}/devicecode")).send_form(&[("client_id", CLIENT_ID), ("scope", SCOPE)]))?;
    if status != 200 {
        return Err(oauth_error(&body));
    }
    parse(&body)
}

fn oauth_error(body: &str) -> AuthError {
    let v: Value = serde_json::from_str(body).unwrap_or_default();
    match v["error"].as_str().unwrap_or("") {
        "expired_token" => AuthError::Expired,
        "authorization_declined" => AuthError::Cancelled,
        "invalid_grant" => AuthError::InvalidGrant,
        _ => AuthError::Service(v["error_description"].as_str().unwrap_or(body).lines().next().unwrap_or("").to_string()),
    }
}

/// Waits until the user finishes signing in in the browser. Setting `cancel` stops waiting.
pub fn poll_device_code(code: &DeviceCode, cancel: &AtomicBool) -> Result<MsToken, AuthError> {
    let deadline = Instant::now() + Duration::from_secs(code.expires_in);
    let mut interval = Duration::from_secs(code.interval.max(1));
    loop {
        let wake = Instant::now() + interval;
        while Instant::now() < wake {
            if cancel.load(Ordering::SeqCst) {
                return Err(AuthError::Cancelled);
            }
            thread::sleep(Duration::from_millis(200));
        }
        if Instant::now() >= deadline {
            return Err(AuthError::Expired);
        }
        let form = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", CLIENT_ID),
            ("device_code", code.device_code.as_str()),
        ];
        let (status, body) = send(|| request("POST", &format!("{AUTHORITY}/token")).send_form(&form))?;
        if status == 200 {
            return parse(&body);
        }
        let v: Value = serde_json::from_str(&body).unwrap_or_default();
        match v["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += Duration::from_secs(5),
            _ => return Err(oauth_error(&body)),
        }
    }
}

/// Exchanges a refresh token for new Microsoft tokens.
pub fn refresh_ms_token(refresh_token: &str) -> Result<MsToken, AuthError> {
    let form = [("grant_type", "refresh_token"), ("client_id", CLIENT_ID), ("scope", SCOPE), ("refresh_token", refresh_token)];
    let (status, body) = send(|| request("POST", &format!("{AUTHORITY}/token")).send_form(&form))?;
    if status != 200 {
        return Err(oauth_error(&body));
    }
    parse(&body)
}

/// Xbox token and user hash from an Xbox Live or XSTS response.
fn xbox_token(body: &str) -> Result<(String, String), AuthError> {
    let v: Value = parse(body)?;
    let token = v["Token"].as_str().unwrap_or_default().to_string();
    let uhs = v["DisplayClaims"]["xui"][0]["uhs"].as_str().unwrap_or_default().to_string();
    if token.is_empty() || uhs.is_empty() {
        return Err(AuthError::Service("Xbox Live 返回的令牌不完整".into()));
    }
    Ok((token, uhs))
}

/// Maps an XSTS `XErr` code to a readable error.
pub fn xsts_error(xerr: u64) -> AuthError {
    match xerr {
        2148916233 => AuthError::NoXboxAccount,
        2148916235 => AuthError::XboxUnavailable,
        2148916236..=2148916238 => AuthError::ChildAccount,
        other => AuthError::Xsts(other),
    }
}

/// Signs in to Minecraft services with a Microsoft access token and returns the Minecraft token,
/// its lifetime in seconds, and the player profile.
pub fn login_minecraft(ms_access_token: &str) -> Result<(String, u64, McProfile), AuthError> {
    let xbl = json!({
        "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access_token}") },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let (status, body) = send(|| request("POST", XBL_AUTH).send_json(&xbl))?;
    if status != 200 {
        return Err(AuthError::Service(format!("Xbox Live 验证失败（HTTP {status}）")));
    }
    let (xbl_token, _) = xbox_token(&body)?;

    let xsts = json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });
    let (status, body) = send(|| request("POST", XSTS_AUTH).send_json(&xsts))?;
    if status == 401 {
        let v: Value = serde_json::from_str(&body).unwrap_or_default();
        return Err(xsts_error(v["XErr"].as_u64().unwrap_or(0)));
    }
    if status != 200 {
        return Err(AuthError::Service(format!("XSTS 验证失败（HTTP {status}）")));
    }
    let (xsts_token, uhs) = xbox_token(&body)?;

    let identity = json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") });
    let (status, body) = send(|| request("POST", &format!("{MC_API}/authentication/login_with_xbox")).send_json(&identity))?;
    if status == 403 && body.contains("Invalid app registration") {
        return Err(AuthError::AppNotApproved);
    }
    if status != 200 {
        return Err(AuthError::Service(format!("Minecraft 服务登录失败（HTTP {status}）")));
    }
    let v: Value = parse(&body)?;
    let mc_token = v["access_token"].as_str().unwrap_or_default().to_string();
    let expires_in = v["expires_in"].as_u64().unwrap_or(86_400);

    match fetch_profile(&mc_token) {
        Err(AuthError::NoProfile) if !owns_minecraft(&mc_token)? => Err(AuthError::NotOwned),
        Err(e) => Err(e),
        Ok(profile) => Ok((mc_token, expires_in, profile)),
    }
}

/// Whether the entitlements list Minecraft: Java Edition (a purchase or Game Pass).
fn owns_minecraft(mc_token: &str) -> Result<bool, AuthError> {
    let (status, body) = send(|| request("GET", &format!("{MC_API}/entitlements/mcstore")).set("Authorization", &format!("Bearer {mc_token}")).call())?;
    if status != 200 {
        return Err(AuthError::Service(format!("无法查询游戏所有权（HTTP {status}）")));
    }
    let v: Value = parse(&body)?;
    Ok(v["items"].as_array().is_some_and(|items| {
        items.iter().any(|i| matches!(i["name"].as_str(), Some("game_minecraft" | "product_minecraft")))
    }))
}

/// Runs the whole chain after the Microsoft sign-in and builds the account.
pub fn account_from_ms_token(ms: &MsToken) -> Result<Account, AuthError> {
    let (access_token, expires_in, profile) = login_minecraft(&ms.access_token)?;
    Ok(Account::Microsoft {
        username: profile.name.clone(),
        uuid: profile.hyphenated_id(),
        access_token,
        refresh_token: ms.refresh_token.clone(),
        expires_at: now() + expires_in,
    })
}

/// Signs a Microsoft account in again with its refresh token. Offline accounts are returned as is.
pub fn refresh_account(account: &Account) -> Result<Account, AuthError> {
    let Account::Microsoft { refresh_token, .. } = account else {
        return Ok(account.clone());
    };
    if refresh_token.is_empty() {
        return Err(AuthError::InvalidGrant);
    }
    let mut ms = refresh_ms_token(refresh_token)?;
    if ms.refresh_token.is_empty() {
        ms.refresh_token = refresh_token.clone();
    }
    account_from_ms_token(&ms)
}

/// Returns the account with a usable token, refreshing it when it is about to expire.
/// `Some` carries a refreshed account that the caller should save.
pub fn ensure_fresh(account: &Account) -> Result<Option<Account>, AuthError> {
    if account.needs_refresh(now()) { refresh_account(account).map(Some) } else { Ok(None) }
}
