//! Automatic re-authentication module
//!
//! Handles automatic token renewal when the current token expires
//! by using headless Chrome to perform SSO login and extract a new token.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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

/// Cookie structure matching remilia_cookies.json format
#[derive(Debug, Serialize, Deserialize)]
struct RemiliaCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    same_site: String,
    expiry: u64,
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
async fn perform_sso_login(driver: &WebDriver, config: &ReauthConfig) -> Result<(String, String, String)> {
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
    let mut beetle_sid = None;
    
    for cookie in &cookies {
        println!("   🍪 Cookie: {} = {}", cookie.name, &cookie.value[..cookie.value.len().min(20)]);
        match cookie.name.as_str() {
            "profile.sid" => profile_sid = Some(cookie.clone()),
            "beetle.sid" => beetle_sid = Some(cookie.clone()),
            _ => {}
        }
    }

    let profile_sid_cookie = profile_sid.context("Failed to extract profile.sid cookie from browser")?;
    
    // If beetle.sid is not found, navigate to the beetle game to initialize it
    let beetle_sid_cookie = if beetle_sid.is_none() {
        println!("⚠️  beetle.sid not found, navigating to beetle game to initialize session...");
        
        // Try multiple approaches to get beetle.sid
        let mut beetle_cookie = None;
        
        // Approach 1: Try direct beetle API endpoint
        println!("🎮 Trying beetle API endpoint...");
        driver.goto("https://api.remilia.com/beetle/user").await?;
        sleep(Duration::from_millis(2000)).await;
        
        for cookie in &driver.get_all_cookies().await? {
            if cookie.name == "beetle.sid" {
                println!("   ✅ Found beetle.sid from API endpoint!");
                beetle_cookie = Some(cookie.clone());
                break;
            }
        }
        
        // Approach 2: Try beetle cartridge page if still not found
        if beetle_cookie.is_none() {
            println!("🎮 Trying beetle cartridge page...");
            driver.goto("https://www.remilia.com/home?cartridge=beetle").await?;
            sleep(Duration::from_millis(5000)).await; // Wait longer for page to fully load
            
            for cookie in &driver.get_all_cookies().await? {
                if cookie.name == "beetle.sid" {
                    println!("   ✅ Found beetle.sid from cartridge page!");
                    beetle_cookie = Some(cookie.clone());
                    break;
                }
            }
        }
        
        // Get cookies again after visiting beetle pages
        let cookies_after_beetle = driver.get_all_cookies().await?;
        println!("🍪 Found {} cookies after beetle initialization", cookies_after_beetle.len());
        
        for cookie in &cookies_after_beetle {
            println!("   🍪 Cookie: {} = {}", cookie.name, &cookie.value[..cookie.value.len().min(20)]);
        }
        
        // If we still don't have it, we need to make an authenticated request to beetle API
        if beetle_cookie.is_none() {
            println!("⚠️  beetle.sid still not found in browser, trying authenticated API request...");
            
            // Make a request to beetle API with profile.sid to initialize beetle session
            let client = reqwest::Client::builder()
                .cookie_store(true)
                .build()?;
            
            let mut headers = reqwest::header::HeaderMap::new();
            let cookie_str = format!("profile.sid={}", profile_sid_cookie.value);
            headers.insert(
                reqwest::header::COOKIE,
                reqwest::header::HeaderValue::from_str(&cookie_str)?,
            );
            
            let response = client
                .get("https://www.remilia.com/beetle/api/user")
                .headers(headers.clone())
                .send()
                .await?;
            
            println!("   📡 Beetle API response status: {}", response.status());
            
            // Check response headers for Set-Cookie
            if let Some(set_cookie) = response.headers().get("set-cookie") {
                if let Ok(cookie_str) = set_cookie.to_str() {
                    println!("   🍪 Set-Cookie header: {}", &cookie_str[..cookie_str.len().min(50)]);
                    
                    // Parse beetle.sid from Set-Cookie header
                    if cookie_str.contains("beetle.sid=") {
                        // Extract value between "beetle.sid=" and ";"
                        if let Some(start) = cookie_str.find("beetle.sid=") {
                            let value_start = start + "beetle.sid=".len();
                            let value_end = cookie_str[value_start..].find(';')
                                .map(|i| value_start + i)
                                .unwrap_or(cookie_str.len());
                            let beetle_sid_value = &cookie_str[value_start..value_end];
                            
                            println!("   ✅ Extracted beetle.sid from Set-Cookie header!");
                            
                            // Create a cookie object manually
                            beetle_cookie = Some(Cookie {
                                name: "beetle.sid".to_string(),
                                value: beetle_sid_value.to_string(),
                                domain: Some(".remilia.com".to_string()),
                                path: Some("/".to_string()),
                                secure: Some(true),
                                expiry: None,
                                same_site: None,
                            });
                        }
                    }
                }
            }
        }
        
        beetle_cookie.context("Failed to extract beetle.sid cookie - tried browser navigation and API request")?
    } else {
        beetle_sid.unwrap()
    };

    // Now use reqwest to make an authenticated request to get the token
    println!("🌐 Making authenticated request to /auth/status...");
    
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()?;
    
    let mut headers = reqwest::header::HeaderMap::new();
    
    // Build cookie string with profile.sid
    let cookie_str = format!("profile.sid={}", profile_sid_cookie.value);
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

    // Save cookies to remilia_cookies.json
    println!("💾 Saving cookies to remilia_cookies.json...");
    
    let cookies_to_save = vec![
        RemiliaCookie {
            name: "profile.sid".to_string(),
            value: profile_sid_cookie.value.clone(),
            domain: profile_sid_cookie.domain.clone().unwrap_or_else(|| ".remilia.com".to_string()),
            path: profile_sid_cookie.path.clone().unwrap_or_else(|| "/".to_string()),
            secure: profile_sid_cookie.secure.unwrap_or(true),
            http_only: true, // Session cookies are typically http_only
            same_site: profile_sid_cookie.same_site
                .map(|s| format!("{:?}", s))
                .unwrap_or_else(|| "Lax".to_string()),
            expiry: profile_sid_cookie.expiry.unwrap_or(0) as u64,
        },
        RemiliaCookie {
            name: "beetle.sid".to_string(),
            value: beetle_sid_cookie.value.clone(),
            domain: beetle_sid_cookie.domain.clone().unwrap_or_else(|| ".remilia.com".to_string()),
            path: beetle_sid_cookie.path.clone().unwrap_or_else(|| "/".to_string()),
            secure: beetle_sid_cookie.secure.unwrap_or(true),
            http_only: true, // Session cookies are typically http_only
            same_site: beetle_sid_cookie.same_site
                .map(|s| format!("{:?}", s))
                .unwrap_or_else(|| "Lax".to_string()),
            expiry: beetle_sid_cookie.expiry.unwrap_or(0) as u64,
        },
    ];
    
    let cookies_json = serde_json::to_string_pretty(&cookies_to_save)
        .context("Failed to serialize cookies to JSON")?;
    
    fs::write("remilia_cookies.json", cookies_json)
        .context("Failed to write cookies to remilia_cookies.json")?;
    
    println!("✅ Cookies saved successfully!");

    Ok((token, profile_sid_cookie.value, beetle_sid_cookie.value))
}

/// Automatically re-authenticate and save new token and cookies
/// Returns (formatted_token, profile_sid)
pub async fn auto_reauth() -> Result<(String, String, String)> {
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

    // Perform login and get token + cookie (this also saves cookies to remilia_cookies.json)
    let result = perform_sso_login(&driver, &config).await;

    // Clean up
    let _ = driver.quit().await;

    let (new_token, profile_sid, beetle_sid) = result?;

    // Format token with Bearer prefix (same format as auth.txt expects)
    let formatted_token = format!("Bearer {}", new_token);

    // Save new token to auth.txt
    println!("💾 Saving new token to auth.txt...");
    fs::write("auth.txt", &formatted_token)
        .context("Failed to write new token to auth.txt")?;

    println!("✅ New token saved successfully!");
    println!("🔄 ===== RE-AUTHENTICATION COMPLETED =====\n");

    Ok((formatted_token, profile_sid, beetle_sid))
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
