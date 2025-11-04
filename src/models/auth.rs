use serde::Deserialize;

use super::profile::Pfp;

/// Response from the auth status endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatusResponse {
    pub authenticated: bool,
    #[serde(default)]
    pub user: Option<AuthUser>,
    #[serde(default)]
    pub token: Option<String>,
}

/// User information from auth endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub sub: String,
    pub username: String,
    pub email: String,
    #[serde(rename = "tokenExpiration")]
    pub token_expiration: u64,
    pub pfp: Pfp,
    pub pfp_url: String,
    pub display_name: String,
    pub theme: String,
    pub cover: String,
    pub color: u32,
    pub onboarded: bool,
    pub pokes: u32,
    pub page_views: u32,
    pub friend_count: u32,
}

/// Cookie structure for loading from JSON
#[derive(Debug, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: Option<bool>,
    pub same_site: String,
    pub expiry: u64,
}
