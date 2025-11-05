use anyhow::{Context, Result};
use std::fs;

use crate::models::Cookie;

/// Load authentication token from file
pub fn load_auth_token() -> Result<String> {
    let token = fs::read_to_string("auth.txt")
        .context("Failed to read auth.txt")?
        .trim()
        .to_string();
    println!("✅ Loaded authentication token: {}", token);
    Ok(token)
}

/// Load Remilia cookies from JSON file
/// Note: Cookies in the JSON file are URL-encoded and should be used as-is
pub fn load_remilia_cookies() -> Result<(String, String)> {
    let json_data = fs::read_to_string("remilia_cookies.json")
        .context("Failed to read remilia_cookies.json")?;

    let cookies: Vec<Cookie> =
        serde_json::from_str(&json_data).context("Failed to parse remilia_cookies.json")?;

    let mut profile_sid = None;
    let mut beetle_sid = None;

    for cookie in cookies {
        match cookie.name.as_str() {
            "profile.sid" => {
                // Use cookie value as-is (already URL-encoded)
                profile_sid = Some(cookie.value);
            }
            "beetle.sid" => {
                // Use cookie value as-is (already URL-encoded)
                beetle_sid = Some(cookie.value);
            }
            _ => {}
        }
    }

    let profile_sid =
        profile_sid.context("profile.sid cookie not found in remilia_cookies.json")?;
    let beetle_sid = beetle_sid.context("beetle.sid cookie not found in remilia_cookies.json")?;

    println!("✅ Loaded profile.sid and beetle.sid cookies");

    Ok((profile_sid, beetle_sid))
}
