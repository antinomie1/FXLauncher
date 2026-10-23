//! Lightweight HTTP client utilities and parameter encoding.

use std::time::Duration;
use thiserror::Error;

pub const DEFAULT_USER_AGENT: &str = "fxlauncher/0.1.0";
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("Network error: {0}")]
    Network(#[from] Box<ureq::Error>),
    #[error("I/O error reading response: {0}")]
    Io(#[from] std::io::Error),
}

/// Executes a GET request with standard headers and timeout.
pub fn http_get(url: &str) -> Result<ureq::Response, Box<ureq::Error>> {
    http_get_with_timeout(url, DEFAULT_TIMEOUT)
}

/// Executes a GET request with custom timeout.
pub fn http_get_with_timeout(url: &str, timeout: Duration) -> Result<ureq::Response, Box<ureq::Error>> {
    ureq::get(url)
        .set("User-Agent", DEFAULT_USER_AGENT)
        .timeout(timeout)
        .call()
        .map_err(Box::new)
}

/// Fetches JSON data from URL into deserializable struct.
pub fn http_get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, HttpError> {
    let resp = http_get(url)?;
    let val: T = resp.into_json()?;
    Ok(val)
}

/// Fetches JSON from the first URL that answers successfully.
pub fn http_get_json_any<T: serde::de::DeserializeOwned>(urls: &[String]) -> Result<T, HttpError> {
    let mut last = None;
    for url in urls {
        match http_get_json(url) {
            Ok(v) => return Ok(v),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| HttpError::Io(std::io::Error::other("no URL to fetch"))))
}

/// Percent-encodes a string for URL query parameters without external crates.
pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}
