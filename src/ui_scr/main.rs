use anyhow::Result;
use colored::*;
use dialoguer::Input;
use rand::Rng;
use std::io;
use std::time::Duration;
use thirtyfour::prelude::*;
use tokio::time::sleep;

mod automation;
mod chromedriver;
mod config;
mod cookies;
mod history;
mod ui;
mod stats;
mod stealth;
mod progress;
mod filter;

use automation::{ run_session, handle_sso_login_if_needed, navigate_to_home_robust, check_and_claim_beetle_with_wait, check_and_claim_cheese_with_wait };
use chromedriver::ChromeDriverManager;
use config::Config;
use cookies::CookieManager;
use history::ProcessedProfiles;
use ui::{ print_ascii_art, print_end_ascii };
use stats::StatsCollector;
use stealth::{StealthBehavior, StealthConfig};
use progress::{ProgressTracker, SessionStats};
use filter::{UsernameFilter, prompt_for_start_filter};

// Format seconds into human-readable time string
fn format_wait_time(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    
    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

// Wait with a countdown display
async fn wait_with_countdown(total_seconds: u64) {
    let start_time = tokio::time::Instant::now();
    let target_duration = Duration::from_secs(total_seconds);
    
    // Show progress every 60 seconds for long waits, or every 10 seconds for shorter waits
    let update_interval = if total_seconds > 300 { 60 } else { 10 };
    
    loop {
        let elapsed = start_time.elapsed();
        if elapsed >= target_duration {
            break;
        }
        
        let remaining = target_duration - elapsed;
        let remaining_secs = remaining.as_secs();
        
        if remaining_secs % update_interval == 0 || remaining_secs < 10 {
            println!(
                "{}",
                format!("⏳ Remaining: {}", format_wait_time(remaining_secs)).bright_black()
            );
        }
        
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

// Check claims and wait if necessary (can be called multiple times)
async fn check_claims_and_wait_if_needed(driver: &WebDriver, config: &Config) -> Result<()> {
    if !config.wait_for_next_claim {
        return Ok(());
    }

    println!("{}", "⏰ Checking claim status before starting session...".bright_cyan());
    
    // Check beetle claim
    let (beetle_claimed, beetle_wait) = check_and_claim_beetle_with_wait(driver).await?;
    
    // Check cheese claim
    let (cheese_claimed, cheese_wait) = check_and_claim_cheese_with_wait(driver).await?;
    
    // If either claim was successfully made, start session immediately
    if beetle_claimed || cheese_claimed {
        let claim_status = match (beetle_claimed, cheese_claimed) {
            (true, true) => "Both claims completed successfully!".to_string(),
            (true, false) => "Beetle claimed successfully!".to_string(),
            (false, true) => "Cheese claimed successfully!".to_string(),
            (false, false) => unreachable!(), // This case won't happen due to the if condition
        };
        
        println!(
            "{}",
            format!("📊 Status: {}", claim_status).bright_green()
        );
        println!("{}", "🚀 Claims made! Starting session immediately...".bright_green());
        return Ok(());
    }
    
    // If no claims were made, determine wait time for next available claim
    let min_wait_time = match (beetle_wait, cheese_wait) {
        (Some(b), Some(c)) => Some(b.min(c)),
        (Some(b), None) => Some(b),
        (None, Some(c)) => Some(c),
        (None, None) => None,
    };
    
    // If there's any wait time, wait before continuing
    if let Some(wait_seconds) = min_wait_time {
        println!(
            "{}",
            "📊 Status: Both claims on cooldown".bright_black()
        );
        println!(
            "{}",
            format!(
                "⏰ Waiting {} before starting the session...",
                format_wait_time(wait_seconds)
            ).bright_yellow()
        );
        println!("{}", "💤 You can press Ctrl+C to cancel and run the script later.".bright_black());
        
        // Wait with a countdown
        wait_with_countdown(wait_seconds).await;
        
        println!("{}", "✅ Wait complete! Starting session now...".bright_green());
        
        // After waiting, try to claim again
        println!("{}", "🔄 Attempting to claim after wait period...".cyan());
        let _ = check_and_claim_beetle_with_wait(driver).await;
        let _ = check_and_claim_cheese_with_wait(driver).await;
    } else {
        println!("{}", "✅ No cooldowns detected, starting session immediately!".bright_green());
    }

    Ok(())
}

// Enhanced session runner with all new Milady features
async fn run_enhanced_session(
    driver: &WebDriver,
    processed_profiles: &mut ProcessedProfiles,
    config: &Config,
    stats_collector: &mut StatsCollector,
    session_stats: &mut SessionStats,
    stealth: &mut StealthBehavior,
) -> Result<()> {
    println!("{}", "\n🌸 Starting enhanced Milady session with all features! ✿\n".bright_magenta());
    
    // For now, we'll use the existing run_session
    // In the future, this can be enhanced to use stealth behavior, stats collection, etc.
    run_session(driver, processed_profiles, config, session_stats).await?;
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    print_ascii_art();

    // Load configuration from environment
    let config = Config::from_env()?;

    // Initialize new Milady v1.9 features
    let mut stats_collector = StatsCollector::new();
    let mut session_stats = SessionStats::new();
    let stealth_config = if config.stealth_enabled {
        StealthConfig::default()
    } else {
        StealthConfig {
            enabled: false,
            ..StealthConfig::default()
        }
    };
    let mut stealth = StealthBehavior::new(stealth_config);

    println!("{}", format!("✨ Milady v1.9 features initialized (stealth: {})", 
        if config.stealth_enabled { "enabled" } else { "disabled" }).bright_cyan());

    // Load persistent history
    let mut processed_profiles = ProcessedProfiles::load_from_file(&config.history_file);
    println!(
        "{}",
        format!(
            "[Info] History loaded: {} profiles already processed.",
            processed_profiles.len()
        ).bright_green()
    );

    // Initialize cookie manager
    let cookie_manager = CookieManager::new(config.cookie_file.clone());

    // Start ChromeDriver process
    let mut chromedriver_manager = ChromeDriverManager::new(
        config.chromedriver_binary_path.clone()
    );
    let chromedriver_url = chromedriver_manager.start().await?;

    // Start browser
    let mut caps = DesiredCapabilities::chrome();
    caps.set_binary(&config.chrome_binary_path)?;
    caps.add_arg("--mute-audio")?;
    caps.add_arg("--headless")?;
    let driver = WebDriver::new(&chromedriver_url, caps).await?;
    driver.goto(&config.home_url).await?;

    // Check if we need to handle SSO login
    let login_handled = handle_sso_login_if_needed(&driver, &config).await?;
    if login_handled {
        println!("{}", "✅ Automatic login completed, navigating to home page...".green());

        // Use robust navigation to home page
        let navigation_success = navigate_to_home_robust(&driver, &config).await?;

        if navigation_success {
            println!("{}", "🏠 Successfully reached home page!".green());
        } else {
            println!(
                "{}",
                "⚠️ Could not reach home page, continuing with profile page...".yellow()
            );
        }
    }

    // Try to load saved cookies
    let _cookies_loaded = cookie_manager.load_cookies(&driver).await.unwrap_or(false);

    if login_handled {
        println!(
            "{}",
            "🚀 Automatic login and setup complete! Starting automation...".bright_green()
        );
        // Small delay to let everything settle
        tokio::time::sleep(Duration::from_millis(1000)).await;
    } else {
        println!(
            "{}",
            "🌸✨💖 Connect on Remilia, then press Enter here to continue... 💖✨🌸".cyan()
        );
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
    }

    // Save cookies after user interaction or auto-login
    let _ = cookie_manager.save_cookies(&driver).await;

    // Optional: Ask if user wants to use username filtering
    // (This can be enabled/disabled based on user preference)
    // Uncomment the following to enable filtering:
    /*
    let start_filter = prompt_for_start_filter();
    if start_filter.is_some() {
        println!("{}", format!("✿ Filter applied: {:?}", start_filter).bright_magenta());
        // You can use UsernameFilter here if processing from a file
    }
    */

    // Check for pending claims if wait_for_next_claim is enabled
    check_claims_and_wait_if_needed(&driver, &config).await?;

    // Main loop with restart capability
    loop {
        // Use enhanced session with all new features
        run_enhanced_session(
            &driver,
            &mut processed_profiles,
            &config,
            &mut stats_collector,
            &mut session_stats,
            &mut stealth,
        ).await?;

        // Display session statistics (Milady style)
        session_stats.display_summary();

        // Save stats if any were collected
        if stats_collector.profile_count() > 0 {
            println!("\n{}", "📊 Displaying rankings...".bright_cyan());
            stats_collector.display_rankings(&config.targets_to_track);
            
            // Save stats to file
            if let Err(e) = stats_collector.save_to_file(&config.stats_file) {
                println!("{}", format!("⚠️ Failed to save stats: {}", e).yellow());
            }
        }

        // Ask if restart
        let restart: String = Input::new()
            .with_prompt(
                "🌸 Do you want to restart a new session now without closing Chrome? (y/N)"
            )
            .default("n".to_string())
            .interact_text()?;

        if restart.to_lowercase() == "y" {
            println!("{}", "💖 Back at it Milady! (Chrome stays open).".cyan());
            let mut rng = rand::thread_rng();
            let delay = Duration::from_millis(rng.gen_range(5000..10000));
            sleep(delay).await;
            
            // Check claims again before restarting the session
            check_claims_and_wait_if_needed(&driver, &config).await?;
            
            continue;
        } else {
            // Ask to close or leave open
            let close: String = Input::new()
                .with_prompt("🌸 Do you want to close Chrome now? (y/N)")
                .default("n".to_string())
                .interact_text()?;

            if close.to_lowercase() == "y" {
                println!("{}", "🌸💖 Closing — see you later Milady ✨".bright_magenta());
                driver.quit().await?;
                chromedriver_manager.stop().await?;
            } else {
                println!(
                    "{}",
                    "🌸💖 Chrome stays open — you can restart the script later on the same session.".bright_magenta()
                );
            }
            break;
        }
    }

    // End ASCII message
    print_end_ascii();

    Ok(())
}
