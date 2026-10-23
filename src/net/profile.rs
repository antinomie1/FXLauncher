//! Minecraft profile (档案) of a Microsoft account: player name, skin and cape.

use super::auth::{AuthError, MC_API, parse, request, send};
use super::client::url_encode;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct McProfile {
    /// UUID without hyphens.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<Skin>,
    #[serde(default)]
    pub capes: Vec<Cape>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Skin {
    pub id: String,
    /// `ACTIVE` or `INACTIVE`.
    pub state: String,
    pub url: String,
    /// `CLASSIC` (Steve, 4-pixel arms) or `SLIM` (Alex, 3-pixel arms).
    #[serde(default)]
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Cape {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub alias: String,
}

impl McProfile {
    pub fn hyphenated_id(&self) -> String {
        uuid::Uuid::parse_str(&self.id).map_or_else(|_| self.id.clone(), |u| u.to_string())
    }

    pub fn active_skin(&self) -> Option<&Skin> {
        self.skins.iter().find(|s| s.state == "ACTIVE")
    }

    pub fn active_cape(&self) -> Option<&Cape> {
        self.capes.iter().find(|c| c.state == "ACTIVE")
    }
}

impl Skin {
    pub fn is_slim(&self) -> bool {
        self.variant.eq_ignore_ascii_case("slim")
    }
}

fn authed(method: &str, path: &str, token: &str) -> ureq::Request {
    request(method, &format!("{MC_API}{path}")).set("Authorization", &format!("Bearer {token}"))
}

/// Reads a profile response, mapping the common failures.
fn profile_response(status: u16, body: &str, action: &str) -> Result<McProfile, AuthError> {
    match status {
        200 => parse(body),
        401 => Err(AuthError::InvalidGrant),
        404 => Err(AuthError::NoProfile),
        429 => Err(AuthError::Service("请求过于频繁，请稍后再试".into())),
        _ => {
            let v: Value = serde_json::from_str(body).unwrap_or_default();
            let message = v["errorMessage"].as_str().or(v["error"].as_str()).unwrap_or("");
            Err(AuthError::Service(format!("{action}失败（HTTP {status}）{message}")))
        }
    }
}

/// Fetches the profile of the signed-in player.
pub fn fetch_profile(token: &str) -> Result<McProfile, AuthError> {
    let (status, body) = send(|| authed("GET", "/minecraft/profile", token).call())?;
    profile_response(status, &body, "读取档案")
}

/// Changes the player name. Mojang allows one change every 30 days.
pub fn change_name(token: &str, name: &str) -> Result<McProfile, AuthError> {
    let (status, body) = send(|| authed("PUT", &format!("/minecraft/profile/name/{}", url_encode(name)), token).call())?;
    let v: Value = serde_json::from_str(&body).unwrap_or_default();
    match (status, v["details"]["status"].as_str()) {
        (_, Some("DUPLICATE")) => Err(AuthError::Service(format!("玩家名 {name} 已被占用"))),
        (_, Some("NOT_ALLOWED")) | (400, _) => Err(AuthError::Service(format!("玩家名 {name} 不可用"))),
        (403, _) => Err(AuthError::Service("距离上次修改玩家名还不到 30 天".into())),
        _ => profile_response(status, &body, "修改玩家名"),
    }
}

/// Checks that `png` is a PNG skin of 64×64 (or legacy 64×32) pixels.
pub fn validate_skin_png(png: &[u8]) -> Result<(), String> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 24 || !png.starts_with(SIGNATURE) || &png[12..16] != b"IHDR" {
        return Err("不是 PNG 图片".into());
    }
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    if width == 64 && (height == 64 || height == 32) {
        Ok(())
    } else {
        Err(format!("皮肤尺寸应为 64×64 或 64×32，这张图片是 {width}×{height}"))
    }
}

/// Uploads a skin image and makes it active.
pub fn upload_skin(token: &str, png: &[u8], slim: bool) -> Result<McProfile, AuthError> {
    validate_skin_png(png).map_err(AuthError::Service)?;
    let boundary = format!("----fxlauncher{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::with_capacity(png.len() + 512);
    let variant = if slim { "slim" } else { "classic" };
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"variant\"\r\n\r\n{variant}\r\n").as_bytes());
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = authed("POST", "/minecraft/profile/skins", token).set("Content-Type", &format!("multipart/form-data; boundary={boundary}"));
    let (status, text) = send(|| req.clone().send_bytes(&body))?;
    profile_response(status, &text, "上传皮肤")
}

/// Sets the skin to one hosted at `url` (e.g. a texture already on textures.minecraft.net).
pub fn set_skin_url(token: &str, url: &str, slim: bool) -> Result<McProfile, AuthError> {
    let variant = if slim { "slim" } else { "classic" };
    let (status, body) = send(|| authed("POST", "/minecraft/profile/skins", token).send_json(json!({ "variant": variant, "url": url })))?;
    profile_response(status, &body, "更换皮肤")
}

/// Resets the skin to the default one.
pub fn reset_skin(token: &str) -> Result<McProfile, AuthError> {
    let (status, body) = send(|| authed("DELETE", "/minecraft/profile/skins/active", token).call())?;
    profile_response(status, &body, "重置皮肤")
}

/// Shows the cape with this id, or hides the cape when `cape_id` is `None`.
pub fn set_cape(token: &str, cape_id: Option<&str>) -> Result<McProfile, AuthError> {
    let (status, body) = send(|| match cape_id {
        Some(id) => authed("PUT", "/minecraft/profile/capes/active", token).send_json(json!({ "capeId": id })),
        None => authed("DELETE", "/minecraft/profile/capes/active", token).call(),
    })?;
    profile_response(status, &body, "更换披风")
}
