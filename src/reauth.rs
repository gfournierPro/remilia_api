//! Automatic re-authentication module
//!
//! Handles automatic token renewal when the current token expires
//! by using headless Chrome to perform SSO login and extract a new token.

use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::time::Duration;
use thirtyfour::prelude::*;
use tokio::time::sleep;

/// Configuration for automatic re-authentication
pub struct ReauthConfig {
    pub email: String,
    pub password: String,
    pub chrome_binary_path: String,
    pub chromedriver_url: String,
}

impl ReauthConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            email: std::env::var("EMAIL").context("EMAIL not set in environment")?,
            password: std::env::var("PASSWORD").context("PASSWORD not set in environment")?,
            chrome_binary_path: std::env::var("CHROME_BINARY_PATH")
                .unwrap_or_else(|_| "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".to_string()),
            chromedriver_url: std::env::var("CHROMEDRIVER_URL")
                .unwrap_or_else(|_| "http://localhost:9515".to_string()),
        })
    }
}

/// Check if a response body is HTML (SSO redirect) instead of JSON
pub fn is_sso_redirect(body: &str) -> bool {
    body.trim_start().starts_with("<!DOCTYPE html") || 
    body.trim_start().starts_with("<html") ||
    body.contains("sso.remilia.org") ||
    body.contains("kcContext")
}

/// Wait for an element with timeout
async fn wait_for_element(driver: &WebDriver, by: By, timeout_secs: u64) -> Result<WebElement> {
    let timeout = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(500);
    let start_time = tokio::time::Instant::now();

    loop {
        if start_time.elapsed() > timeout {
            anyhow::bail!("Timeout waiting for element");
        }

        match driver.find(by.clone()).await {
            Ok(elem) if elem.is_displayed().await.unwrap_or(false) => {
                return Ok(elem);
            }
            _ => {}
        }

        sleep(poll_interval).await;
    }
}

/// Wait for element using multiple selectors
async fn wait_for_element_multiple(
    driver: &WebDriver,
    selectors: Vec<By>,
    timeout_secs: u64,
) -> Result<WebElement> {
    let timeout = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(500);
    let start_time = tokio::time::Instant::now();

    loop {
        if start_time.elapsed() > timeout {
            anyhow::bail!("Timeout waiting for any of the elements");
        }

        for selector in &selectors {
            if let Ok(elem) = driver.find(selector.clone()).await {
                if elem.is_displayed().await.unwrap_or(false) {
                    return Ok(elem);
                }
            }
        }

        sleep(poll_interval).await;
    }
}

/// Perform SSO login and extract new auth token
async fn perform_sso_login(driver: &WebDriver, config: &ReauthConfig) -> Result<(String, String)> {
    println!("🔐 Clearing cookies and navigating to Remilia to trigger SSO login...");
    
    // Clear all cookies to force SSO login
    driver.delete_all_cookies().await?;
    sleep(Duration::from_millis(500)).await;
    
    // Navigate to a protected endpoint to trigger SSO
    println!("🌐 Navigating to protected endpoint...");
    driver.goto("https://remilia.com/").await?;
    sleep(Duration::from_millis(2000)).await;

    let current_url = driver.current_url().await?;
    println!("📍 Current URL: {}", current_url.as_str());
    
    if !current_url.as_str().contains("sso.remilia.org") {
        // Try navigating to auth/status instead
        println!("⚠️  Not on SSO page, trying /auth/status...");
        driver.goto("https://remilia.com/").await?;
        sleep(Duration::from_millis(2000)).await;
        
        let auth_url = driver.current_url().await?;
        println!("📍 Auth URL: {}", auth_url.as_str());
        
        if !auth_url.as_str().contains("sso.remilia.org") {
            anyhow::bail!("Expected SSO login page but got: {}", auth_url.as_str());
        }
    }

    println!("✅ On SSO login page, attempting automatic login...");

    // Check for initial login button
    match wait_for_element(
        driver,
        By::XPath("//*[@id=\"kc-content-wrapper\"]/div[2]/div/div[3]/div/div[2]/div[1]/button"),
        5,
    )
    .await
    {
        Ok(initial_btn) => {
            println!("🔄 Clicking initial login button...");
            initial_btn.click().await?;
            sleep(Duration::from_millis(3000)).await;
        }
        Err(_) => {
            println!("ℹ️ Initial login button not found, proceeding to form...");
        }
    }

    // Wait for and fill username
    println!("📧 Filling email...");
    let username_selectors = vec![
        By::Id("username"),
        By::Css("input[name=\"username\"]"),
        By::Css("input[type=\"text\"]"),
    ];
    let username_field = wait_for_element_multiple(driver, username_selectors, 15).await?;
    username_field.clear().await?;
    username_field.send_keys(&config.email).await?;

    // Wait for and fill password
    println!("🔑 Filling password...");
    let password_selectors = vec![
        By::Id("password"),
        By::Css("input[name=\"password\"]"),
        By::Css("input[type=\"password\"]"),
    ];
    let password_field = wait_for_element_multiple(driver, password_selectors, 5).await?;
    password_field.clear().await?;
    password_field.send_keys(&config.password).await?;

    // Click Sign In
    println!("🚀 Clicking Sign In button...");
    let signin_selectors = vec![
        By::Css("button[type=\"submit\"]"),
        By::XPath("//button[contains(text(), 'Sign In')]"),
        By::XPath("//button[contains(@class, 'btn-primary')]"),
    ];
    let signin_btn = wait_for_element_multiple(driver, signin_selectors, 5).await?;
    signin_btn.click().await?;

    println!("⏳ Waiting for login to complete...");
    sleep(Duration::from_millis(5000)).await;

    // Check if redirected away from SSO
    let new_url = driver.current_url().await?;
    println!("📍 After login URL: {}", new_url.as_str());
    
    if new_url.as_str().contains("sso.remilia.org") {
        anyhow::bail!("Login failed - still on SSO page: {}", new_url.as_str());
    }

    println!("✅ Login successful! Extracting cookies...");

    // Get all cookies from the browser
    let cookies = driver.get_all_cookies().await?;
    println!("🍪 Found {} cookies", cookies.len());
    
    // Look for session cookies
    let mut profile_sid = None;
    
    for cookie in &cookies {
        println!("   🍪 Cookie: {} = {}", cookie.name, &cookie.value[..cookie.value.len().min(20)]);
        match cookie.name.as_str() {
            "profile.sid" => profile_sid = Some(cookie.value.clone()),
            _ => {}
        }
    }

    let profile_sid = profile_sid.context("Failed to extract profile.sid cookie from browser")?;

    // Now use reqwest to make an authenticated request to get the token
    println!("🌐 Making authenticated request to /auth/status...");
    
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()?;
    
    let mut headers = reqwest::header::HeaderMap::new();
    
    // Build cookie string with profile.sid
    let cookie_str = format!("profile.sid={}", profile_sid);
    println!("🍪 Using cookies: {} chars", cookie_str.len());
    headers.insert(
        reqwest::header::COOKIE,
        reqwest::header::HeaderValue::from_str(&cookie_str)?,
    );
    
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
        ),
    );
    
    let response = client
        .get("https://www.remilia.com/auth/status")
        .headers(headers)
        .send()
        .await?;
    
    let status = response.status();
    println!("📡 Response status: {}", status);
    
    let body = response.text().await?;
    println!("📦 Response preview: {}", &body[..body.len().min(200)]);
    
    // Check if we got HTML (SSO redirect) instead of JSON
    if is_sso_redirect(&body) {
        anyhow::bail!("Still getting SSO redirect after login - cookies may not be working");
    }
    
    // Parse JSON to extract token
    let auth_data: Value = serde_json::from_str(&body)
        .context("Failed to parse auth status response")?;
    
    let token = auth_data
        .get("token")
        .and_then(|t| t.as_str())
        .context("Token not found in auth response")?
        .to_string();

    println!("🔑 Successfully extracted new token: {}...", &token[..token.len().min(20)]);

    Ok((token, profile_sid))
}

/// Automatically re-authenticate and save new token
/// Returns (formatted_token, profile_sid)
pub async fn auto_reauth() -> Result<(String, String)> {
    println!("\n🔄 ===== AUTOMATIC RE-AUTHENTICATION STARTED =====");
    println!("🔐 Token expired, attempting to get a new one...\n");

    let config = ReauthConfig::from_env()?;

    // Set up Chrome capabilities (headless)
    let mut caps = DesiredCapabilities::chrome();
    caps.set_binary(&config.chrome_binary_path)?;
    caps.add_arg("--headless")?;
    caps.add_arg("--mute-audio")?;
    caps.add_arg("--disable-gpu")?;
    caps.add_arg("--no-sandbox")?;
    
    println!("🌐 Starting headless Chrome...");
    let driver = WebDriver::new(&config.chromedriver_url, caps).await
        .context("Failed to start WebDriver - is ChromeDriver running?")?;

    // Perform login and get token + cookie
    let result = perform_sso_login(&driver, &config).await;

    // Clean up
    let _ = driver.quit().await;

    let (new_token, profile_sid) = result?;

    // Format token with Bearer prefix (same format as auth.txt expects)
    let formatted_token = format!("Bearer {}", new_token);

    // Save new token to auth.txt
    println!("💾 Saving new token to auth.txt...");
    fs::write("auth.txt", &formatted_token)
        .context("Failed to write new token to auth.txt")?;

    println!("✅ New token saved successfully!");
    println!("🔄 ===== RE-AUTHENTICATION COMPLETED =====\n");

    Ok((formatted_token, profile_sid))
}

/// Check if chromedriver is running, if not provide helpful error
pub fn check_chromedriver_available() -> Result<()> {
    // Try to connect to default ChromeDriver port
    let url = std::env::var("CHROMEDRIVER_URL")
        .unwrap_or_else(|_| "http://localhost:9515".to_string());
    
    println!("ℹ️ To enable automatic re-authentication, start ChromeDriver:");
    println!("   chromedriver --port=9515");
    println!("   Or set CHROMEDRIVER_URL environment variable");
    println!("   Current URL: {}", url);
    
    Ok(())
}
