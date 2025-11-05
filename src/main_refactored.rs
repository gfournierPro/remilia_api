//! Beetle Hunt Bot - Refactored Main
//!
//! This application provides automated interactions with the Beetle game API,
//! including beetle catching, cheese claiming, friend management, and more.

mod config;
mod models;
mod utils;
mod reauth;

use anyhow::{Context, Result};
use config::{load_auth_token, load_remilia_cookies};
use models::{
    beetle::*, database::FriendsDatabase, profile::*,
};
use rand::rngs::OsRng;
use rand::Rng;
use reauth::{auto_reauth, is_sso_redirect};
use reqwest::{header, Client, StatusCode};
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::signal;
use tokio::sync::Mutex; // Use tokio::sync::Mutex for async
use tokio::time::{interval, Duration};
use utils::{extract_cooldown_seconds, format_duration};

// ===== AUTH STATUS STRUCTS =====

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AuthStatusResponse {
    authenticated: bool,
    #[serde(default)]
    user: Option<AuthUser>,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AuthUser {
    sub: String,
    username: String,
    email: String,
    #[serde(rename = "tokenExpiration")]
    token_expiration: u64,
    pfp: Pfp,
    pfp_url: String,
    display_name: String,
    #[serde(default)]
    credentials: Vec<Credential>,
    #[serde(default)]
    connections: Vec<Connection>,
    #[serde(default)]
    wallets: Vec<Wallet>,
    theme: String,
    cover: String,
    color: u32,
    onboarded: bool,
    pokes: u32,
    page_views: u32,
    friend_count: u32,
}

#[derive(Debug, Deserialize, Clone)]
struct Pfp {
    project: String,
    id: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Credential {
    #[serde(rename = "type")]
    credential_type: String,
    category: String,
    display_name: String,
    #[serde(default)]
    helptext: String,
    #[serde(default)]
    icon_css_class: String,
    #[serde(default)]
    update_action: Option<String>,
    #[serde(default)]
    create_action: Option<String>,
    removeable: bool,
    #[serde(default)]
    user_credential_metadatas: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Connection {
    #[serde(rename = "type")]
    connection_type: String,
    username: String,
}

#[derive(Debug, Deserialize, Clone)]
struct Wallet {
    address: String,
    added: String,
}

impl AuthStatusResponse {
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    pub fn username(&self) -> Option<&str> {
        self.user.as_ref().map(|u| u.username.as_str())
    }

    pub fn token_expires_in(&self) -> Option<u64> {
        self.user.as_ref().map(|u| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            
            if u.token_expiration > now {
                u.token_expiration - now
            } else {
                0
            }
        })
    }

    pub fn display_info(&self) {
        if !self.authenticated {
            println!("❌ Not authenticated");
            return;
        }

        if let Some(user) = &self.user {
            println!("\n╔═════════════════════════════════════════════╗");
            println!("║           🔐 AUTH STATUS                    ║");
            println!("╠═════════════════════════════════════════════╣");
            println!("║ Username:         {:26} ║", user.username);
            println!("║ Display Name:     {:26} ║", user.display_name);
            println!("║ Email:            {:26} ║", user.email);
            println!("╠═════════════════════════════════════════════╣");
            println!("║ 👉 Pokes:                      {:>12} ║", user.pokes);
            println!("║ 👥 Friends:                    {:>12} ║", user.friend_count);
            println!("║ 👀 Page Views:                 {:>12} ║", user.page_views);
            println!("╠═════════════════════════════════════════════╣");
            println!("║ PFP:              {:26} ║", format!("{} #{}", user.pfp.project, user.pfp.id));
            println!("║ Theme:            {:26} ║", user.theme);
            println!("║ Cover:            {:26} ║", user.cover);
            println!("╠═════════════════════════════════════════════╣");
            
            if !user.connections.is_empty() {
                println!("║ 🔗 CONNECTIONS:                             ║");
                for conn in &user.connections {
                    println!("║   {:<10} {:28} ║", 
                        format!("{}:", conn.connection_type), 
                        conn.username
                    );
                }
                println!("╠═════════════════════════════════════════════╣");
            }

            if !user.wallets.is_empty() {
                println!("║ 💰 WALLETS:                                 ║");
                for wallet in &user.wallets {
                    let short_addr = format!("{}...{}", 
                        &wallet.address[0..6], 
                        &wallet.address[wallet.address.len()-4..]
                    );
                    println!("║   {:39} ║", short_addr);
                }
                println!("╠═════════════════════════════════════════════╣");
            }

            let expires_in = self.token_expires_in().unwrap_or(0);
            if expires_in > 0 {
                println!("║ 🔑 Token expires in: {:>19} ║", 
                    format_duration(expires_in));
            } else {
                println!("║ 🔑 Token:                      ⚠️  EXPIRED  ║");
            }

            println!("╚═════════════════════════════════════════════╝\n");
        }
    }
}

// ===== API CLIENT =====

struct BeetleApiClient {
    client: Client,
    auth_token: Arc<Mutex<String>>, // Make mutable for token renewal
    profile_sid: Arc<Mutex<String>>,
    beetle_sid: Arc<Mutex<String>>,
    token_expiration: Arc<AtomicU64>, // Unix timestamp when token expires
    reauth_in_progress: Arc<Mutex<bool>>, // Prevent multiple simultaneous reauth attempts
}

impl BeetleApiClient {
    fn new(auth_token: &str, profile_sid: &str, beetle_sid: &str) -> Result<Self> {
        let client = Client::builder()
            .cookie_store(true)
            .build()?;

        Ok(Self {
            client,
            auth_token: Arc::new(Mutex::new(auth_token.to_string())),
            profile_sid: Arc::new(Mutex::new(profile_sid.to_string())),
            beetle_sid: Arc::new(Mutex::new(beetle_sid.to_string())),
            token_expiration: Arc::new(AtomicU64::new(0)), // Will be set after first auth check
            reauth_in_progress: Arc::new(Mutex::new(false)),
        })
    }

    /// Check if the auth token is expired or will expire soon (within 5 minutes)
    fn is_token_expired(&self) -> bool {
        let expiration = self.token_expiration.load(Ordering::Relaxed);
        if expiration == 0 {
            return false; // Not set yet, assume valid
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Consider expired if less than 5 minutes remaining
        expiration < now + 300
    }

    /// Get time until token expiration in seconds
    fn time_until_expiration(&self) -> Option<u64> {
        let expiration = self.token_expiration.load(Ordering::Relaxed);
        if expiration == 0 {
            return None;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        if expiration > now {
            Some(expiration - now)
        } else {
            Some(0)
        }
    }

    async fn build_headers_remilia(&self) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();

        // Build cookie string with both session IDs
        let profile_sid = self.profile_sid.lock().await.clone();
        let beetle_sid = self.beetle_sid.lock().await.clone();
        
        let cookie_str = format!(
            "profile.sid={}; beetle.sid={}",
            profile_sid, beetle_sid
        );

        headers.insert(
            header::COOKIE,
            header::HeaderValue::from_str(&cookie_str)
                .unwrap_or_else(|_| header::HeaderValue::from_static("")),
        );

        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("*/*"),
        );

        headers.insert(
            header::ORIGIN,
            header::HeaderValue::from_static("https://www.remilia.com"),
        );

        headers.insert(
            header::REFERER,
            header::HeaderValue::from_static("https://www.remilia.com/"),
        );
        
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            ),
        );

        headers
    }

    async fn build_headers_beetle(&self) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();

        // Beetle API uses Authorization header, not cookies
        let auth_token = self.auth_token.lock().await.clone();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&auth_token)
                .unwrap_or_else(|_| header::HeaderValue::from_static("")),
        );

        headers.insert(
            header::ORIGIN,
            header::HeaderValue::from_static("https://remilia.com"),
        );

        headers.insert(
            header::REFERER,
            header::HeaderValue::from_static("https://remilia.com/"),
        );

        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("*/*"),
        );

        headers.insert(
            "sec-fetch-site",
            header::HeaderValue::from_static("cross-site"),
        );

        headers
    }

    pub async fn get_beetle_user(&self) -> Result<User> {
        let headers = self.build_headers_beetle().await;
        let text = self
            .api_request_with_retry(|| {
                self.client
                    .get("https://www.remilia.com/beetle/api/user")
                    .headers(headers.clone())
            })
            .await?;

        let user = serde_json::from_str(&text)
            .context("Failed to parse user JSON")?;
        Ok(user)
    }

    async fn beetle_catch(&self) -> Result<CatchBeetleApiResponse> {
        let headers = self.build_headers_beetle().await;
        let text = self
            .api_request_with_retry(|| {
                self.client
                    .post("https://www.remilia.com/beetle/api/action/catchBeetle")
                    .headers(headers.clone())
                    .json(&CatchBeetleRequest {})
            })
            .await?;

        let catch_response = serde_json::from_str(&text)
            .context("Failed to parse catch beetle JSON")?;
        Ok(catch_response)
    }

    async fn claim_ubc(&self) -> Result<ClaimUBCApiResponse> {
        let headers = self.build_headers_beetle().await;
        let text = self
            .api_request_with_retry(|| {
                self.client
                    .post("https://www.remilia.com/beetle/api/action/claimUBC")
                    .headers(headers.clone())
                    .json(&ClaimUBCRequest {})
            })
            .await?;

        let claim_response = serde_json::from_str(&text)
            .context("Failed to parse claim UBC JSON")?;
        Ok(claim_response)
    }

    async fn get_cooldowns(&self) -> Result<CooldownsResponse> {
        println!("📡 Fetching cooldowns...");
        
        let headers = self.build_headers_beetle().await;
        let text = self
            .api_request_with_retry(|| {
                self.client
                    .get("https://www.remilia.com/beetle/api/cooldowns")
                    .headers(headers.clone())
            })
            .await?;

        println!("📦 Raw cooldowns response: {}", &text[..text.len().min(200)]);

        let cooldowns = serde_json::from_str(&text)
            .context("Failed to parse cooldowns JSON")?;
        Ok(cooldowns)
    }

    async fn beetle_hunt(&self) -> Result<BeetleHuntApiResponse> {
        let headers = self.build_headers_beetle().await;
        let text = self
            .api_request_with_retry(|| {
                self.client
                    .post("https://www.remilia.com/beetle/api/action/beetleHunt")
                    .headers(headers.clone())
                    .json(&BeetleHuntRequest {})
            })
            .await?;

        let hunt_response = serde_json::from_str(&text)
            .context("Failed to parse beetle hunt JSON")?;
        Ok(hunt_response)
    }

    async fn poke_user(&self, username: &str) -> Result<PokeResponse> {
        let url = "https://www.remilia.com/api/poke";
        let body = PokeRequest {
            poke_username: username.to_string(),
        };

        let headers = self.build_headers_remilia().await;
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let poke_response = response.json::<PokeResponse>().await?;
        Ok(poke_response)
    }

    async fn send_friend_request(&self, username: &str) -> Result<FriendsResponse> {
        let url = "https://www.remilia.com/api/friends/request";
        let body = FriendsRequest {
            friend_username: username.to_string(),
        };

        let headers = self.build_headers_remilia().await;
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let friend_response = response.json::<FriendsResponse>().await?;
        Ok(friend_response)
    }

    async fn get_auth_status(&self) -> Result<AuthStatusResponse> {
        println!("📡 Fetching auth status...");

        let headers = self.build_headers_remilia().await;
        
        // Debug: Print the cookies being sent
        if let Some(cookie_header) = headers.get(header::COOKIE) {
            if let Ok(cookie_str) = cookie_header.to_str() {
                println!("🍪 Sending cookies: {}", cookie_str);
            }
        }

        let response = self
            .client
            .get("https://www.remilia.com/auth/status")
            .headers(headers)
            .send()
            .await
            .context("Failed to send auth status request")?;

        let status = response.status();
        println!("✅ Response status: {}", status);

        // Extract and update cookies if provided
        if let Some(cookie_header) = response.headers().get(header::SET_COOKIE) {
            if let Ok(cookie_str) = cookie_header.to_str() {
                println!("🍪 Server sent new cookie: {}", cookie_str);

                // Parse and update the appropriate cookie
                if cookie_str.contains("profile.sid=") {
                    // Extract just the cookie value (before the semicolon)
                    if let Some(value) = cookie_str.split(';').next() {
                        if let Some(cookie_value) = value.strip_prefix("profile.sid=") {
                            *self.profile_sid.lock().await = cookie_value.to_string();
                            println!("✅ Updated profile.sid");
                        }
                    }
                }
                if cookie_str.contains("beetle.sid=") {
                    if let Some(value) = cookie_str.split(';').next() {
                        if let Some(cookie_value) = value.strip_prefix("beetle.sid=") {
                            *self.beetle_sid.lock().await = cookie_value.to_string();
                            println!("✅ Updated beetle.sid");
                        }
                    }
                }
            }
        }

        // Handle 304 Not Modified (means we're still authenticated)
        if status == StatusCode::NOT_MODIFIED {
            println!("✅ Auth status unchanged (304)");
            return Ok(AuthStatusResponse {
                authenticated: true,
                user: None,
                token: None,
            });
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        println!("📦 Raw response: {}", text);

        let auth_status: AuthStatusResponse =
            serde_json::from_str(&text).context("Failed to parse auth status JSON")?;

        // Update token expiration if we have user info
        if let Some(user) = &auth_status.user {
            self.token_expiration.store(user.token_expiration, Ordering::Relaxed);
            
            let time_until = if user.token_expiration > SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() {
                user.token_expiration - SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
            } else {
                0
            };
            
            println!("🔑 Token expiration updated: {} ({} remaining)", 
                user.token_expiration,
                format_duration(time_until)
            );
        }

        Ok(auth_status)
    }

    /// Check if we need to refresh authentication and do so if necessary
    async fn ensure_authenticated(&self) -> Result<()> {
        if self.is_token_expired() {
            println!("⚠️  Token expired or expiring soon, refreshing authentication...");
            
            match self.get_auth_status().await {
                Ok(auth) => {
                    if !auth.is_authenticated() {
                        anyhow::bail!("Failed to refresh authentication - not authenticated");
                    }
                    println!("✅ Authentication refreshed successfully");
                    Ok(())
                }
                Err(e) => {
                    anyhow::bail!("Failed to refresh authentication: {}", e)
                }
            }
        } else {
            if let Some(time_left) = self.time_until_expiration() {
                if time_left < 3600 && time_left % 600 == 0 {
                    // Log every 10 minutes when less than 1 hour remains
                    println!("⏰ Token expires in: {}", format_duration(time_left));
                }
            }
            Ok(())
        }
    }

    /// Renew the authentication token using automatic re-authentication
    async fn renew_token(&self) -> Result<()> {
        // Check if reauth is already in progress
        {
            let mut in_progress = self.reauth_in_progress.lock().await;
            if *in_progress {
                println!("⏳ Re-authentication already in progress, waiting...");
                drop(in_progress);
                
                // Wait for the other reauth to complete
                for _ in 0..30 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if !*self.reauth_in_progress.lock().await {
                        println!("✅ Re-authentication completed by another worker");
                        return Ok(());
                    }
                }
                anyhow::bail!("Timeout waiting for re-authentication to complete");
            }
            
            // Set the flag to indicate we're starting reauth
            *in_progress = true;
        }
        
        println!("🔄 Attempting automatic token renewal...");
        
        let result = match auto_reauth().await {
            Ok((new_token, new_profile_sid)) => {
                // Update the token and profile_sid
                *self.auth_token.lock().await = new_token;
                *self.profile_sid.lock().await = new_profile_sid;
                println!("✅ Token and cookies renewed successfully!");
                
                // Reset expiration to trigger a fresh check
                self.token_expiration.store(0, Ordering::Relaxed);
                
                // Verify new token by checking auth status
                match self.get_auth_status().await {
                    Ok(auth) => {
                        if auth.is_authenticated() {
                            println!("✅ New token verified and working!");
                            Ok(())
                        } else {
                            anyhow::bail!("New token verification failed - not authenticated")
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("Failed to verify new token: {}", e)
                    }
                }
            }
            Err(e) => {
                anyhow::bail!("Failed to renew token: {}", e)
            }
        };
        
        // Clear the reauth in progress flag
        *self.reauth_in_progress.lock().await = false;
        
        result
    }

    /// Check if response body is an SSO redirect and handle token renewal if needed
    async fn check_and_handle_sso_redirect(&self, body: &str) -> Result<bool> {
        if is_sso_redirect(body) {
            println!("⚠️  Detected SSO redirect - token has expired!");
            println!("📡 Response starts with: {}", &body[..body.len().min(100)]);
            
            // Attempt automatic token renewal
            self.renew_token().await?;
            
            Ok(true) // Renewed
        } else {
            Ok(false) // Not an SSO redirect
        }
    }

    /// Helper method to make API request with automatic SSO redirect handling
    /// Returns the response text, automatically retrying once if SSO redirect is detected
    async fn api_request_with_retry(
        &self,
        request_builder: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<String> {
        let response = request_builder().send().await?;
        let text = response.text().await?;

        // Check if this is an SSO redirect
        if self.check_and_handle_sso_redirect(&text).await? {
            println!("🔄 Token renewed, retrying request...");
            
            // Retry the request with new token
            let retry_response = request_builder().send().await?;
            let retry_text = retry_response.text().await?;
            return Ok(retry_text);
        }

        Ok(text)
    }

    async fn get_friends_page(
        &self,
        username: &str,
        page: u32,
        limit: u32,
    ) -> Result<FriendsListResponse> {
        let url = format!(
            "https://www.remilia.com/identity/friends?username={}&page={}&limit={}",
            username, page, limit
        );

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers_remilia().await)
            .header("Referer", format!("https://www.remilia.com/~{}", username))
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get friends: {}", response.status());
        }

        let data: FriendsListResponse = response.json().await?;
        Ok(data)
    }

    async fn scrape_all_friends(&self, db_file: &str) -> Result<()> {
        let username = "remilia_jackson";
        let my_username = "mao";
        println!("🔍 Scraping friends of ~{}...\n", username);

        let db = FriendsDatabase::new(db_file);
        let initial_count = db.count()?;
        println!("📊 Database currently has {} usernames\n", initial_count);

        // Get first page to determine total
        let first_page = self.get_friends_page(username, 1, 1000).await?;
        let total_friends = first_page.total;
        let total_pages = (total_friends as f32 / 1000.0).ceil() as u32;

        println!("👥 Target user has {} friends", total_friends);
        println!("📄 Will scrape {} pages\n", total_pages);

        let mut all_usernames = Vec::new();
        let mut errors = 0;
        let mut filtered_self = 0;

        // Scrape all pages
        for page in 1..=total_pages {
            print!("📄 Page {}/{} ... ", page, total_pages);
            std::io::Write::flush(&mut std::io::stdout())?;

            match self.get_friends_page(username, page, 1000).await {
                Ok(response) => {
                    let usernames: Vec<String> = response
                        .friends
                        .iter()
                        .filter_map(|f| {
                            let username = f.display_username.clone();
                            // Filter out your own username (with or without ~)
                            if username == my_username
                                || username == format!("~{}", my_username)
                                || username.trim_start_matches('~') == my_username
                            {
                                filtered_self += 1;
                                None
                            } else {
                                Some(username)
                            }
                        })
                        .collect();

                    println!("✅ Got {} friends", usernames.len());
                    all_usernames.extend(usernames);

                    // Rate limiting
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Err(e) => {
                    println!("❌ Error: {}", e);
                    errors += 1;

                    // If we get a parsing error, log the raw response for debugging
                    if errors < 3 {
                        println!("   ⚠️  Continuing with next page...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    } else {
                        println!("   ❌ Too many errors, stopping scrape");
                        break;
                    }
                }
            }
        }

        println!("\n💾 Syncing database...");

        // Sync database: add new friends and remove unfriended users
        let sync_stats = db.sync_with_friends(&all_usernames)?;

        println!("\n✨ ===== SCRAPING COMPLETE =====");
        println!("📥 Total friends scraped: {}", all_usernames.len());
        if filtered_self > 0 {
            println!("🚫 Filtered out self: {} instance(s)", filtered_self);
        }
        println!("➕ New users added: {}", sync_stats.added);
        println!("➖ Unfriended users removed: {}", sync_stats.removed);
        println!(
            "📊 Database size: {} → {}",
            sync_stats.initial_count, sync_stats.final_count
        );
        println!("💾 Saved to: {}", db_file);
        if errors > 0 {
            println!("⚠️  Encountered {} page errors", errors);
        }

        Ok(())
    }

    async fn auto_engage_from_db_with_stats(
        &self,
        db_file: &str,
        stats: &WorkerStats,
    ) -> Result<(u64, u64)> {
        let db = FriendsDatabase::new(db_file);
        let pokeable = db.get_pokeable_users()?;
        let users: Vec<String> = pokeable.into_iter().collect();

        if users.is_empty() {
            println!("\n🎯 No pokeable users right now!");

            // Show when next user will be available
            if let Ok(Some(next)) = db.get_next_pokeable_user() {
                let time_str = format_duration(next.time_until_pokeable);
                let ready_at_str = format_timestamp(next.time_until_pokeable);

                println!("\n╔═══════════════════════════════════════════╗");
                println!("║      ⏰ NEXT POKEABLE USER                ║");
                println!("╠═══════════════════════════════════════════╣");
                println!("║ Username:          {:>22} ║", next.username);
                println!("║ Ready in:          {:>22} ║", time_str);
                println!("║ Ready at:          {:>22} ║", ready_at_str);
                println!("╚═══════════════════════════════════════════╝\n");
            }

            return Ok((0, 0));
        }

        println!("\n🚀 ===== AUTO-ENGAGE FROM DATABASE =====");
        println!("📊 Processing {} users\n", users.len());

        let total_seconds_in_24h = 10.0 * 60.0 * 60.0;
        let db_count = db.count()? as f64;
        let mean_delay = (total_seconds_in_24h / db_count.max(1.0)).max(5.0);

        let min_delay = (mean_delay * 0.6).max(2.0);
        let max_delay = mean_delay * 1.4;

        println!(
            "⏱️  Delay range: {:.1}s - {:.1}s (mean: {:.1}s)\n",
            min_delay, max_delay, mean_delay
        );

        let mut poke_success = 0;
        let mut poke_on_cooldown = 0;
        let mut friend_success = 0;
        let mut rng = OsRng;

        for (i, username) in users.iter().enumerate() {
            println!("[{}/{}] 🎯 Processing ~{}...", i + 1, users.len(), username);

            if let Some(record) = db.get_user(username)? {
                match self.poke_user(username).await {
                    Ok(response) if response.success => {
                        println!("   👉 Poked!");
                        db.update_poke(username)?;
                        poke_success += 1;
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        if err_msg.contains("Still on cooldown") {
                            println!("   ⏳ On cooldown - updating timestamp");
                            if let Some(cooldown_secs) = extract_cooldown_seconds(&err_msg) {
                                db.update_poke_with_cooldown(username, cooldown_secs as i64)?;
                                poke_on_cooldown += 1;
                            } else {
                                db.update_poke(username)?;
                                poke_on_cooldown += 1;
                            }
                        } else {
                            println!("   ⚠️  Poke failed: {}", e);
                        }
                    }
                    _ => println!("   ⚠️  Poke failed"),
                }

                tokio::time::sleep(Duration::from_millis(800)).await;

                if !record.friend_request_sent {
                    match self.send_friend_request(username).await {
                        Ok(response) if response.success => {
                            println!("   🤝 Friend request sent!");
                            db.mark_friend_request_sent(username)?;
                            friend_success += 1;
                        }
                        _ => println!("   ⚠️  Friend request failed"),
                    }
                }
            }

            if i < users.len() - 1 {
                let delay_secs = rng.gen_range(min_delay..=max_delay);
                println!("   ⏳ Next in {:.1}s...\n", delay_secs);
                tokio::time::sleep(Duration::from_secs_f64(delay_secs)).await;
            }
        }

        // ✅ Update stats
        stats.increment_pokes(poke_success);
        stats.increment_friends(friend_success);

        println!("\n✨ ===== ENGAGEMENT SUMMARY =====");
        println!("✅ Pokes successful: {}", poke_success);
        println!("⏳ Pokes on cooldown: {}", poke_on_cooldown);
        println!("🤝 Friend Requests: {}", friend_success);

        // Show next pokeable user after completing all current pokes
        if let Ok(Some(next)) = db.get_next_pokeable_user() {
            let time_str = format_duration(next.time_until_pokeable);
            println!("\n⏰ Next user ready: {} in {}", next.username, time_str);
        }

        Ok((poke_success, friend_success))
    }
}

// ===== WORKER STATS =====

#[derive(Clone)]
struct WorkerStats {
    beetles_caught: Arc<AtomicU64>,
    cheese_claimed: Arc<AtomicU64>,
    users_poked: Arc<AtomicU64>,
    users_friended: Arc<AtomicU64>,
    catch_completed: Arc<AtomicU64>,
    hunts_completed: Arc<AtomicU64>,
    hunts_missed: Arc<AtomicU64>,
    last_beetle_time: Arc<AtomicU64>,
    last_cheese_time: Arc<AtomicU64>,
    last_poke_time: Arc<AtomicU64>,
    beetle_inventory: Arc<Mutex<SessionBeetleInventory>>,
}

impl WorkerStats {
    fn new() -> Self {
        Self {
            beetles_caught: Arc::new(AtomicU64::new(0)),
            cheese_claimed: Arc::new(AtomicU64::new(0)),
            users_poked: Arc::new(AtomicU64::new(0)),
            users_friended: Arc::new(AtomicU64::new(0)),
            catch_completed: Arc::new(AtomicU64::new(0)),
            hunts_completed: Arc::new(AtomicU64::new(0)),
            hunts_missed: Arc::new(AtomicU64::new(0)),
            last_beetle_time: Arc::new(AtomicU64::new(0)),
            last_cheese_time: Arc::new(AtomicU64::new(0)),
            last_poke_time: Arc::new(AtomicU64::new(0)),
            beetle_inventory: Arc::new(Mutex::new(SessionBeetleInventory::default())),
        }
    }

    fn increment_beetles(&self) {
        self.beetles_caught.fetch_add(1, Ordering::Relaxed);
        self.last_beetle_time.store(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    fn increment_cheese(&self) {
        self.cheese_claimed.fetch_add(1, Ordering::Relaxed);
        self.last_cheese_time.store(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    fn increment_catch(&self) {
        self.catch_completed.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_hunts(&self) {
        self.hunts_completed.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_missed_hunts(&self) {
        self.hunts_missed.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_pokes(&self, count: u64) {
        self.users_poked.fetch_add(count, Ordering::Relaxed);
        self.last_poke_time.store(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    fn increment_friends(&self, count: u64) {
        self.users_friended.fetch_add(count, Ordering::Relaxed);
    }

    async fn init_inventory(&self, inventory: BeetleInventory) {
        let mut inv = self.beetle_inventory.lock().await;
        *inv = SessionBeetleInventory::new(inventory);
    }

    async fn update_inventory(&self, inventory: BeetleInventory) {
        let mut inv = self.beetle_inventory.lock().await;
        inv.update(inventory);
    }

    async fn get_inventory(&self) -> SessionBeetleInventory {
        self.beetle_inventory.lock().await.clone()
    }
}

// ===== HELPER FUNCTIONS =====

fn format_time_ago(timestamp_secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if timestamp_secs == 0 || timestamp_secs > now {
        return "Never".to_string();
    }

    let elapsed = now - timestamp_secs;

    if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{}h ago", elapsed / 3600)
    } else {
        format!("{}d ago", elapsed / 86400)
    }
}

fn format_timestamp(seconds_from_now: u64) -> String {
    use chrono::{Duration, Local};

    let now = Local::now();
    let target = now + Duration::seconds(seconds_from_now as i64);
    target.format("at %H:%M:%S").to_string()
}

// ===== DISPLAY FUNCTIONS =====

fn display_catch_result(response: &CatchBeetleApiResponse) {
    match response {
        CatchBeetleApiResponse::Success { result, user, .. } => {
            println!("\n🎉 ===== CATCH SUCCESS! =====");
            println!("🪲 Beetle: {} ({})", result.beetle_name, result.beetle);
            println!("✨ XP Gained: +{}", result.xp);
            println!("⏰ Cooldown: {}ms", result.cooldown_ms);
            println!("🎯 Species: {}", result.beetle_card.species);
            println!("\n📊 Your Stats:");
            println!("   Level: {}", user.level);
            println!("   XP: {}", user.xp);
            println!("   Cheese: {}", user.cheese);
            println!("   Total Beetles: {}", user.total_beetles());
        }
        CatchBeetleApiResponse::Error { error, user, .. } => {
            println!("\n❌ ===== CATCH FAILED =====");
            println!("Error: {}", error);
            println!("\n📊 Your Stats:");
            println!("   Level: {}", user.level);
            println!("   Cheese: {}", user.cheese);
        }
    }
}

fn display_claim_ubc_result(response: &ClaimUBCApiResponse) {
    match response {
        ClaimUBCApiResponse::Success { result, user, .. } => {
            println!("\n🎉 ===== CHEESE CLAIMED! =====");
            println!("🧀 Cheese: +{}", result.cheese);
            println!("✨ XP: +{}", result.xp);
            println!("🔥 Streak: {}", result.streak);
            println!("\n📊 Your Stats:");
            println!("   Level: {}", user.level);
            println!("   XP: {}", user.xp);
            println!("   Cheese: {}", user.cheese);
        }
        ClaimUBCApiResponse::Error { error, user, .. } => {
            println!("\n❌ ===== CLAIM FAILED =====");
            println!("Error: {}", error);
            println!("\n📊 Your Stats:");
            println!("   Level: {}", user.level);
            println!("   Cheese: {}", user.cheese);
        }
    }
}

fn display_hunt_result(result: &BeetleHuntApiResponse) {
    match result {
        BeetleHuntApiResponse::Success { result, user, .. } => {
            println!("\n🎉 ===== HUNT SUCCESS! =====");
            println!("🪲 Caught: {} ({})", result.beetle_name, result.beetle_card.species);
            println!("✨ XP Gained: +{}", result.xp);
            println!("🎯 Hunts used: {}/3", user.beetle_hunts_used);
            println!("🧀 Cheese remaining: {}", user.cheese);
        }
        BeetleHuntApiResponse::FailedHunt { user, .. } => {
            println!("\n💨 ===== BEETLE ESCAPED! =====");
            println!("The beetle got away this time...");
            println!("🎯 Hunts used: {}/3", user.beetle_hunts_used);
            println!("🧀 Cheese remaining: {}", user.cheese);
        }
        BeetleHuntApiResponse::Error { error, user, .. } => {
            println!("\n❌ ===== HUNT FAILED =====");
            println!("Error: {}", error);
            println!("🎯 Hunts used: {}/3", user.beetle_hunts_used);
        }
    }
}

// ===== WORKER FUNCTIONS =====

async fn beetle_auto_claim_worker(client: Arc<BeetleApiClient>, stats: WorkerStats) -> Result<()> {
    println!("🪲 [BEETLE WORKER] Starting...\n");

    match client.get_beetle_user().await {
        Ok(user) => {
            stats.init_inventory(user.inventory.clone()).await;
            let total = stats.get_inventory().await.total_beetles();
            println!(
                "✅ Session initialized with {} total beetles",
                total
            );
        }
        Err(e) => {
            eprintln!("⚠️  Could not initialize inventory: {}", e);
        }
    }

    loop {
        // Check authentication before each iteration
        if let Err(e) = client.ensure_authenticated().await {
            eprintln!("❌ Authentication check failed: {}", e);
            println!("⏳ Waiting 5 minutes before retry...\n");
            tokio::time::sleep(Duration::from_secs(300)).await;
            continue;
        }

        match client.get_beetle_user().await {
            Ok(user) => {
                user.display_status();
                stats.update_inventory(user.inventory.clone()).await;

                // === TRY TO CATCH BEETLE ===
                if user.can_catch_beetle() {
                    println!("🎯 Attempting to catch beetle...");
                    match client.beetle_catch().await {
                        Ok(result) => match result {
                            CatchBeetleApiResponse::Success { result, user, .. } => {
                                display_catch_result(&CatchBeetleApiResponse::Success {
                                    success: true,
                                    result: result.clone(),
                                    user: user.clone(),
                                });
                                stats.increment_beetles();
                                stats.increment_catch();
                                stats.update_inventory(user.inventory).await;
                            }
                            CatchBeetleApiResponse::Error { error, .. } => {
                                eprintln!("❌ Catch failed: {}", error);
                            }
                        },
                        Err(e) => eprintln!("❌ Catch error: {}", e),
                    }
                }

                // === TRY TO DO BEETLE HUNTS ===
                if user.has_hunts_remaining() {
                    if user.cheese < 20 {
                        println!(
                            "🧀 Insufficient cheese ({}/20) - skipping hunts",
                            user.cheese
                        );
                        println!("   Waiting for cheese to regenerate...\n");
                    } else {
                        let hunts_to_do = user.hunts_remaining();
                        println!("🎯 Starting {} beetle hunt(s)...", hunts_to_do);

                        for hunt_num in 1..=hunts_to_do {
                            println!("\n🎯 Hunt {}/{}", hunt_num, hunts_to_do);

                            match client.beetle_hunt().await {
                                Ok(result) => {
                                    match result {
                                        BeetleHuntApiResponse::Success { result, user, .. } => {
                                            display_hunt_result(&BeetleHuntApiResponse::Success {
                                                success: true,
                                                result: result.clone(),
                                                user: user.clone(),
                                            });
                                            stats.increment_hunts();
                                            stats.increment_beetles();
                                            stats.update_inventory(user.inventory).await;
                                        }
                                        BeetleHuntApiResponse::FailedHunt { user, .. } => {
                                            display_hunt_result(
                                                &BeetleHuntApiResponse::FailedHunt {
                                                    success: true,
                                                    result: FailedHuntResult { success: false },
                                                    user: user.clone(),
                                                },
                                            );
                                            stats.increment_hunts();
                                            stats.increment_missed_hunts();
                                            stats.update_inventory(user.inventory).await;
                                        }
                                        BeetleHuntApiResponse::Error { error, user, .. } => {
                                            display_hunt_result(&BeetleHuntApiResponse::Error {
                                                success: false,
                                                error: error.clone(),
                                                user: user.clone(),
                                            });
                                        }
                                    }

                                    if hunt_num < hunts_to_do {
                                        println!("   ⏳ Waiting 60s before next hunt...");
                                        tokio::time::sleep(Duration::from_secs(60)).await;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("❌ Hunt error: {}", e);
                                    if hunt_num < hunts_to_do {
                                        println!("   ⏳ Waiting 60s before next hunt...");
                                        tokio::time::sleep(Duration::from_secs(60)).await;
                                    }
                                }
                            }
                        }
                        println!("✅ Completed all available hunts\n");
                    }
                }

                // === CALCULATE NEXT CHECK TIME ===
                let next_check = if user.can_catch_beetle() {
                    0
                } else {
                    if user.cheese < 20 {
                        let wait_time = user.time_until_catch_ready();
                        println!(
                            "🧀 [BEETLE] Low cheese ({}/20) - waiting for next catch opportunity",
                            user.cheese
                        );
                        wait_time
                    } else {
                        let catch_cooldown = user.time_until_catch_ready();
                        let hunt_reset = user.time_until_hunt_reset();
                        catch_cooldown.min(hunt_reset)
                    }
                };
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!(
                    "[BEETLE] ⏳ Next check in: {} ({})\n",
                    format_duration(next_check),
                    format_timestamp(next_check)
                );

                tokio::time::sleep(Duration::from_secs(next_check)).await;
            }
            Err(e) => {
                eprintln!("❌ Failed to fetch beetle user: {}", e);
                println!("⏳ Retrying in 60 seconds...\n");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    }
}

async fn cheese_auto_claim_worker(client: Arc<BeetleApiClient>, stats: WorkerStats) -> Result<()> {
    println!("🧀 [CHEESE WORKER] Starting...\n");

    loop {
        // Check authentication before each iteration
        if let Err(e) = client.ensure_authenticated().await {
            eprintln!("❌ Authentication check failed: {}", e);
            println!("⏳ Waiting 5 minutes before retry...\n");
            tokio::time::sleep(Duration::from_secs(300)).await;
            continue;
        }

        match client.get_cooldowns().await {
            Ok(cooldowns) => {
                let claim_ubc_seconds = cooldowns.time_until_ubc_ready();

                if cooldowns.can_claim_ubc() {
                    println!("🎯 UBC claim is ready!");

                    match client.claim_ubc().await {
                        Ok(result) => {
                            display_claim_ubc_result(&result);
                            stats.increment_cheese();

                            println!("\n⏳ Waiting 1 hour before next cooldown check...");
                            tokio::time::sleep(Duration::from_secs(3600)).await;
                        }
                        Err(e) => {
                            eprintln!("❌ Claim failed: {}", e);
                            println!("⏳ Retrying in 5 minutes...");
                            tokio::time::sleep(Duration::from_secs(300)).await;
                        }
                    }
                } else {
                    println!(
                        "⏰ UBC claim on cooldown for: {}",
                        format_duration(claim_ubc_seconds)
                    );

                    let wait_time = if claim_ubc_seconds > 3600 {
                        (claim_ubc_seconds - 300).max(60)
                    } else {
                        claim_ubc_seconds.max(60)
                    };

                    println!(
                        "[CHEESE] ⏳ Next check in: {} ({})\n",
                        format_duration(wait_time),
                        format_timestamp(wait_time)
                    );

                    tokio::time::sleep(Duration::from_secs(wait_time)).await;
                }
            }
            Err(e) => {
                eprintln!("❌ Failed to fetch cooldowns: {}", e);
                println!("⏳ Retrying in 1 minute...");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    }
}

async fn daily_poke_worker(client: Arc<BeetleApiClient>, stats: WorkerStats) -> Result<()> {
    println!("👉 [POKE WORKER] Starting...\n");

    let mut interval = interval(Duration::from_secs(30 * 60));

    loop {
        interval.tick().await;

        // Check authentication before each cycle
        if let Err(e) = client.ensure_authenticated().await {
            eprintln!("❌ Authentication check failed: {}", e);
            continue;
        }

        println!("👉 [POKE] Checking for pokeable users...");

        match client
            .auto_engage_from_db_with_stats("friends_db.json", &stats)
            .await
        {
            Ok((pokes, friends)) => {
                println!(
                    "👉 [POKE] ✅ Cycle complete! Poked: {}, Friended: {}",
                    pokes, friends
                );
            }
            Err(e) => {
                println!("👉 [POKE] ❌ Error: {}", e);
            }
        }

        let next_check = chrono::Local::now() + chrono::Duration::minutes(30);
        println!(
            "👉 [POKE] ⏰ Next check in 30 minutes, at {}",
            next_check.format("%Y-%m-%d %H:%M:%S")
        );
    }
}

async fn daily_scrape_worker(client: Arc<BeetleApiClient>) -> Result<()> {
    println!("📥 [SCRAPE WORKER] Starting...\n");

    let mut interval = interval(Duration::from_secs(24 * 60 * 60));

    loop {
        interval.tick().await;

        println!("📥 [SCRAPE] Pulling new friends from remilia_jackson...");

        match client.scrape_all_friends("friends_db.json").await {
            Ok(_) => {
                println!("📥 [SCRAPE] ✅ Database updated!");
            }
            Err(e) => {
                println!("📥 [SCRAPE] ❌ Error: {}", e);
            }
        }

        println!("📥 [SCRAPE] ⏰ Next scrape in 24 hours\n");
    }
}

async fn status_dashboard_worker(stats: WorkerStats, client: Arc<BeetleApiClient>) -> Result<()> {
    println!("📊 [DASHBOARD] Starting status worker...\n");

    let mut interval = interval(Duration::from_secs(5 * 60));

    loop {
        interval.tick().await;

        // Check authentication periodically
        if let Err(e) = client.ensure_authenticated().await {
            eprintln!("⚠️  Dashboard: Authentication check failed: {}", e);
        }

        match client.get_beetle_user().await {
            Ok(user) => {
                stats.update_inventory(user.inventory.clone()).await;
            }
            Err(e) => {
                eprintln!("⚠️  Failed to fetch beetle user: {}", e);
            }
        }

        let inventory = stats.get_inventory().await;
        let total_beetles = inventory.total_beetles();
        let session_gain = inventory.total_session_gain();

        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║                  📊 WORKER STATUS DASHBOARD               ║");
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!(
            "║ 🪲 Beetles Caught (Session):      {:>24} ║",
            stats.beetles_caught.load(Ordering::Relaxed)
        );
        println!(
            "║ 🧀 Cheese Claimed (Session):      {:>24} ║",
            stats.cheese_claimed.load(Ordering::Relaxed)
        );
        println!(
            "║ 🪤 Catch Completed (Session):     {:>24} ║",
            stats.catch_completed.load(Ordering::Relaxed)
        );
        println!(
            "║ 🎯 Hunts Completed (Session):     {:>24} ║",
            stats.hunts_completed.load(Ordering::Relaxed)
        );

        let missed_hunts = stats.hunts_missed.load(Ordering::Relaxed);
        if missed_hunts > 0 {
            println!("║ 💨 Hunts Failed (Escaped):        {:>24} ║", missed_hunts);
        }

        println!(
            "║ 👉 Users Poked (Session):         {:>24} ║",
            stats.users_poked.load(Ordering::Relaxed)
        );
        println!(
            "║ 🤝 Friend Requests (Session):     {:>24} ║",
            stats.users_friended.load(Ordering::Relaxed)
        );
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║               🪲 BEETLE INVENTORY (TOTAL)                 ║");
        println!("╠═══════════════════════════════════════════════════════════╣");

        if session_gain > 0 {
            println!(
                "║ Total Beetles:                     {:>5} (+{:<4})      ║",
                total_beetles, session_gain
            );
        } else {
            println!(
                "║ Total Beetles:                          {:>10} ║",
                total_beetles
            );
        }
        println!("║                                                           ║");

        for (emoji, rarity, count, delta, _) in inventory.get_sorted_beetles() {
            let percentage = if total_beetles > 0 {
                (count as f64 / total_beetles as f64) * 100.0
            } else {
                0.0
            };

            if delta > 0 {
                println!(
                    "║ {} {:10} {:>3} ({:>5.1}%) [+{:<2}]                    ║",
                    emoji, rarity, count, percentage, delta
                );
            } else {
                println!(
                    "║ {} {:10} {:>3} ({:>5.1}%)                          ║",
                    emoji, rarity, count, percentage
                );
            }
        }

        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║                    ⏰ LAST ACTIVITY                       ║");
        println!("╠═══════════════════════════════════════════════════════════╣");

        let last_beetle = stats.last_beetle_time.load(Ordering::Relaxed);
        let last_cheese = stats.last_cheese_time.load(Ordering::Relaxed);
        let last_poke = stats.last_poke_time.load(Ordering::Relaxed);

        println!(
            "║ 🪲 Last Beetle:                   {:>24} ║",
            if last_beetle > 0 {
                format_time_ago(last_beetle)
            } else {
                "Never".to_string()
            }
        );
        println!(
            "║ � Last Cheese:                   {:>24} ║",
            if last_cheese > 0 {
                format_time_ago(last_cheese)
            } else {
                "Never".to_string()
            }
        );
        println!(
            "║ 👉 Last Poke:                     {:>24} ║",
            if last_poke > 0 {
                format_time_ago(last_poke)
            } else {
                "Never".to_string()
            }
        );

        println!("╠═══════════════════════════════════════════════════════════╣");
        println!(
            "║ 🕐 Updated:                       {:>24} ║",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        
        // Show token expiration status
        if let Some(time_left) = client.time_until_expiration() {
            if time_left > 0 {
                println!(
                    "║ 🔑 Token expires in:              {:>24} ║",
                    format_duration(time_left)
                );
            } else {
                println!("║ 🔑 Token:                         ⚠️  EXPIRED         ║");
            }
        }
        
        println!("╚═══════════════════════════════════════════════════════════╝\n");
    }
}

async fn run_all_workers(client: BeetleApiClient) -> Result<()> {
    println!("🤖 ===== STARTING ALL WORKERS =====\n");

    let client = Arc::new(client);
    let stats = WorkerStats::new();

    // Initialize session inventory
    if let Ok(user) = client.get_beetle_user().await {
        stats.init_inventory(user.inventory).await;
    }

    let stats_clone = stats.clone();
    let client_clone = client.clone();
    let dashboard_handle = tokio::spawn(async move {
        status_dashboard_worker(stats_clone, client_clone).await
    });

    let stats_clone = stats.clone();
    let client_clone = client.clone();
    let beetle_handle = tokio::spawn(async move {
        beetle_auto_claim_worker(client_clone, stats_clone).await
    });

    let stats_clone = stats.clone();
    let client_clone = client.clone();
    let cheese_handle = tokio::spawn(async move {
        cheese_auto_claim_worker(client_clone, stats_clone).await
    });

    let stats_clone = stats.clone();
    let client_clone = client.clone();
    let poke_handle = tokio::spawn(async move {
        daily_poke_worker(client_clone, stats_clone).await
    });

    let client_clone = client.clone();
    let scrape_handle = tokio::spawn(async move {
        daily_scrape_worker(client_clone).await
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            println!("\n🛑 Shutting down...");
        }
        _ = dashboard_handle => {}
        _ = beetle_handle => {}
        _ = cheese_handle => {}
        _ = poke_handle => {}
        _ = scrape_handle => {}
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 Beetle Hunt Bot Starting...\n");
    println!("🤖 Starting ALL workers in automated mode...\n");

    // Load environment variables for auto-reauth
    dotenv::dotenv().ok();
    
    // Check if auto-reauth is available
    println!("🔧 Checking automatic re-authentication setup...");
    reauth::check_chromedriver_available()?;
    println!();

    let token = load_auth_token()?;
    let (profile_sid, beetle_sid) = load_remilia_cookies()?;
    let client = BeetleApiClient::new(&token, &profile_sid, &beetle_sid)?;

    // Check auth status at startup
    println!("🔐 Checking authentication status...\n");
    match client.get_auth_status().await {
        Ok(auth) => {
            auth.display_info();
            if !auth.is_authenticated() {
                println!("⚠️  Authentication failed - attempting automatic token renewal...\n");
                
                // Try to renew the token
                match client.renew_token().await {
                    Ok(()) => {
                        println!("✅ Token renewed successfully!");
                        
                        // Verify the new token works
                        match client.get_auth_status().await {
                            Ok(renewed_auth) => {
                                renewed_auth.display_info();
                                if !renewed_auth.is_authenticated() {
                                    anyhow::bail!("❌ Token renewal failed - still not authenticated");
                                }
                            }
                            Err(e) => {
                                anyhow::bail!("❌ Failed to verify renewed token: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("❌ Failed to renew token: {}\n\nPlease update your auth.txt and remilia_cookies.json files", e);
                    }
                }
            }
        }
        Err(e) => {
            println!("⚠️  Failed to fetch auth status: {}", e);
            println!("⚠️  Attempting automatic token renewal...\n");
            
            // Try to renew the token even if the request failed
            match client.renew_token().await {
                Ok(()) => {
                    println!("✅ Token renewed successfully!");
                    
                    // Verify the new token works
                    match client.get_auth_status().await {
                        Ok(renewed_auth) => {
                            renewed_auth.display_info();
                            if !renewed_auth.is_authenticated() {
                                anyhow::bail!("❌ Token renewal failed - still not authenticated");
                            }
                        }
                        Err(e) => {
                            anyhow::bail!("❌ Failed to verify renewed token: {}", e);
                        }
                    }
                }
                Err(e) => {
                    anyhow::bail!("❌ Failed to renew token: {}\n\nPlease update your auth.txt and remilia_cookies.json files manually", e);
                }
            }
        }
    }

    run_all_workers(client).await?;

    Ok(())
}
