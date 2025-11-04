use anyhow::Result;
use colored::*;
use rand::Rng;
use std::time::Duration;
use thirtyfour::prelude::*;
use tokio::time::sleep;

use crate::config::Config;
use crate::history::ProcessedProfiles;

// Parse time string like "1h 50m" or "11h 27m 21s" into total seconds
fn parse_time_to_seconds(time_str: &str) -> Option<u64> {
    // Clean the input: remove "to next claim" and other suffixes
    let time_str = time_str
        .trim()
        .to_lowercase()
        .replace("to next claim", "")
        .trim()
        .to_string();
    
    let mut total_seconds: u64 = 0;
    
    // Extract hours
    if let Some(h_pos) = time_str.find('h') {
        if let Ok(hours) = time_str[..h_pos].trim().parse::<u64>() {
            total_seconds += hours * 3600;
        }
    }
    
    // Extract minutes
    if let Some(m_pos) = time_str.find('m') {
        // Find the start of the minutes part (after hours if present)
        let start = if let Some(h_pos) = time_str.find('h') {
            h_pos + 1
        } else {
            0
        };
        if let Ok(minutes) = time_str[start..m_pos].trim().parse::<u64>() {
            total_seconds += minutes * 60;
        }
    }
    
    // Extract seconds
    if let Some(s_pos) = time_str.find('s') {
        // Find the start of the seconds part (after minutes if present)
        let start = if let Some(m_pos) = time_str.find('m') {
            m_pos + 1
        } else if let Some(h_pos) = time_str.find('h') {
            h_pos + 1
        } else {
            0
        };
        if let Ok(seconds) = time_str[start..s_pos].trim().parse::<u64>() {
            total_seconds += seconds;
        }
    }
    
    if total_seconds > 0 {
        Some(total_seconds)
    } else {
        None
    }
}

// Format seconds into human-readable time string
fn format_duration(seconds: u64) -> String {
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

// Helper function to detect if a WebDriver error is due to window closure
fn is_window_closed_error(error: &WebDriverError) -> bool {
    match error {
        WebDriverError::NoSuchWindow(_) => true,
        WebDriverError::SessionNotCreated(_) => true,
        WebDriverError::InvalidSessionId(_) => true,
        WebDriverError::UnknownCommand(info) => {
            let msg = format!("{:?}", info);
            msg.contains("target window already closed") ||
                msg.contains("web view not found") ||
                msg.contains("no such window")
        }
        _ => {
            let error_str = format!("{:?}", error);
            error_str.contains("target window already closed") ||
                error_str.contains("web view not found") ||
                error_str.contains("no such window")
        }
    }
}

// Safely execute WebDriver operations, returning an error if the window is closed
async fn safe_driver_operation<F, T>(operation: F) -> Result<T>
    where F: std::future::Future<Output = Result<T, WebDriverError>>
{
    match operation.await {
        Ok(result) => Ok(result),
        Err(e) if is_window_closed_error(&e) => {
            Err(anyhow::anyhow!("Browser window was closed during operation"))
        }
        Err(e) => Err(anyhow::anyhow!("WebDriver error: {}", e)),
    }
}

// Wait for an element to be present and visible with timeout
async fn wait_for_element(driver: &WebDriver, by: By, timeout_secs: u64) -> Result<WebElement> {
    let timeout = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(500);
    let start_time = tokio::time::Instant::now();

    loop {
        if start_time.elapsed() > timeout {
            return Err(anyhow::anyhow!("Timeout waiting for element: {:?}", by));
        }

        match safe_driver_operation(driver.find(by.clone())).await {
            Ok(element) => {
                // Check if element is displayed
                if safe_driver_operation(element.is_displayed()).await.unwrap_or(false) {
                    return Ok(element);
                }
            }
            Err(_) => {
                // Element not found yet, continue waiting
            }
        }

        sleep(poll_interval).await;
    }
}

// Wait for an element using multiple selectors, returns the first one found
async fn wait_for_element_multiple(
    driver: &WebDriver,
    selectors: Vec<By>,
    timeout_secs: u64
) -> Result<WebElement> {
    let timeout = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(500);
    let start_time = tokio::time::Instant::now();

    loop {
        if start_time.elapsed() > timeout {
            return Err(anyhow::anyhow!("Timeout waiting for any of the elements: {:?}", selectors));
        }

        for selector in &selectors {
            match safe_driver_operation(driver.find(selector.clone())).await {
                Ok(element) => {
                    // Check if element is displayed
                    if safe_driver_operation(element.is_displayed()).await.unwrap_or(false) {
                        return Ok(element);
                    }
                }
                Err(_) => {
                    // Element not found with this selector, try next
                    continue;
                }
            }
        }

        sleep(poll_interval).await;
    }
}

// Check and claim daily cheese if ready, returns (claimed, wait_time_seconds)
pub async fn check_and_claim_cheese_with_wait(driver: &WebDriver) -> Result<ClaimCheckResult> {
    println!("{}", "🧀 Checking for daily cheese claim...".cyan());

    // Try to find the cheese navigation item
    let cheese_nav_selectors = vec![
        By::Css("div.nav-item.cheese-claim-nav"),
        By::Css("div[class*='cheese-claim-nav']"),
        By::Css("div.nav-item:has(.icon.cheese)"),
    ];

    let cheese_nav = match wait_for_element_multiple(driver, cheese_nav_selectors, 5).await {
        Ok(element) => element,
        Err(_) => {
            println!("{}", "⚠️ Cheese nav item not found, skipping...".yellow());
            return Ok((false, None));
        }
    };

    // Check if cheese is ready by looking for the status text within the cheese nav element
    // Wait a bit to ensure the status text is loaded
    sleep(Duration::from_millis(500)).await;
    
    let status_text = match safe_driver_operation(cheese_nav.find(By::Css("div.info"))).await {
        Ok(info_div) => {
            // Find all spans within the info div
            match safe_driver_operation(info_div.find_all(By::Tag("span"))).await {
                Ok(spans) if spans.len() >= 2 => {
                    // Get the second span (index 1) which contains the status
                    match safe_driver_operation(spans[1].text()).await {
                        Ok(text) => {
                            let trimmed_text = text.trim().to_string();
                            println!("{}", format!("📊 Cheese status: '{}'", trimmed_text).bright_black());
                            
                            if trimmed_text.is_empty() {
                                println!("{}", "⚠️ Cheese status text is empty, trying inner_html...".yellow());
                                // Try getting the inner HTML as fallback
                                if let Ok(inner_html) = safe_driver_operation(spans[1].inner_html()).await {
                                    println!("{}", format!("📊 Cheese status (HTML): '{}'", inner_html).bright_black());
                                    inner_html.to_lowercase()
                                } else {
                                    trimmed_text.to_lowercase()
                                }
                            } else {
                                trimmed_text.to_lowercase()
                            }
                        }
                        Err(_) => {
                            println!("{}", "⚠️ Could not read cheese status text".yellow());
                            return Ok((false, None));
                        }
                    }
                }
                Ok(spans) => {
                    println!("{}", format!("⚠️ Found {} spans instead of 2 in cheese info", spans.len()).yellow());
                    return Ok((false, None));
                }
                Err(e) => {
                    println!("{}", format!("⚠️ Could not find spans in cheese info: {}", e).yellow());
                    return Ok((false, None));
                }
            }
        }
        Err(_) => {
            println!("{}", "⚠️ Cheese info div not found".yellow());
            return Ok((false, None));
        }
    };

    // Check if cheese is ready
    if !status_text.contains("ready") {
        let wait_time = parse_time_to_seconds(&status_text);
        println!(
            "{}",
            format!("⏳ Daily cheese not ready yet (status: {})", status_text).bright_black()
        );
        return Ok((false, wait_time));
    }

    println!("{}", "✅ Daily cheese is ready! Opening cheese module...".green());

    // Click on the cheese nav item to open the module
    if let Err(e) = safe_driver_operation(cheese_nav.click()).await {
        println!("{}", format!("⚠️ Failed to click cheese nav: {}", e).yellow());
        return Ok((false, None));
    }

    // Wait a bit for the module to appear
    sleep(Duration::from_millis(1500)).await;

    // Try to find the cheese module (uses beetleModule class with ubc-module-interface)
    let cheese_module_selectors = vec![
        By::Css("div.beetleModule div.ubc-module-interface"),
        By::Css("div[class*='beetleModule'] div[class*='ubc-module-interface']"),
        By::Css("div.ubc-module-interface"),
    ];

    let _cheese_module = match wait_for_element_multiple(driver, cheese_module_selectors, 5).await {
        Ok(element) => element,
        Err(_) => {
            println!("{}", "⚠️ Cheese module did not appear, closing...".yellow());
            // Try to press Escape to close any modal
            if let Err(_) = safe_driver_operation(
                driver.action_chain().send_keys(Key::Escape).perform()
            ).await {
                println!("{}", "⚠️ Could not send Escape key".yellow());
            }
            return Ok((false, None));
        }
    };

    println!("{}", "🎯 Cheese module opened, looking for claim button...".cyan());

    // Try to find the claim button (it's an image button)
    let claim_button_selectors = vec![
        By::Css("div.ubc-module-interface button.claim-button"),
        By::Css("button.claim-button"),
        By::Css("button:has(img[src*='claim-button'])"),
    ];

    let claim_button = match wait_for_element_multiple(driver, claim_button_selectors, 5).await {
        Ok(element) => element,
        Err(_) => {
            println!("{}", "⚠️ Claim button not found in cheese module".yellow());
            // Try to close the module by pressing Escape
            if let Err(_) = safe_driver_operation(
                driver.action_chain().send_keys(Key::Escape).perform()
            ).await {
                println!("{}", "⚠️ Could not send Escape key".yellow());
            }
            return Ok((false, None));
        }
    };

    // Check if the button is disabled
    let is_disabled = safe_driver_operation(claim_button.class_name()).await
        .map(|class| class.unwrap_or_default().contains("disabled"))
        .unwrap_or(false);

    if is_disabled {
        println!("{}", "⏳ Claim button is disabled (already claimed today)".yellow());
        
        // Close the module by pressing Escape
        if let Err(_) = safe_driver_operation(
            driver.action_chain().send_keys(Key::Escape).perform()
        ).await {
            println!("{}", "⚠️ Could not send Escape key".yellow());
        }
        
        return Ok((false, None));
    }

    println!("{}", "🎯 Clicking cheese claim button...".green());

    // Scroll the button into view first to avoid interception
    if let Err(e) = safe_driver_operation(
        claim_button.scroll_into_view()
    ).await {
        println!("{}", format!("⚠️ Could not scroll button into view: {}", e).yellow());
    }

    // Wait a bit after scrolling
    sleep(Duration::from_millis(500)).await;

    // Try to click using JavaScript as the regular click might be intercepted
    let js_click = r#"arguments[0].click();"#;
    match safe_driver_operation(driver.execute(js_click, vec![claim_button.to_json()?])).await {
        Ok(_) => {
            println!("{}", "✅ Clicked cheese claim button using JavaScript".bright_black());
        }
        Err(e) => {
            println!("{}", format!("⚠️ JavaScript click failed: {}, trying regular click...", e).yellow());
            
            // Fallback to regular click
            if let Err(e) = safe_driver_operation(claim_button.click()).await {
                println!("{}", format!("⚠️ Failed to click claim button: {}", e).yellow());
                // Try to close the module
                if let Err(_) = safe_driver_operation(
                    driver.action_chain().send_keys(Key::Escape).perform()
                ).await {
                    println!("{}", "⚠️ Could not send Escape key".yellow());
                }
                return Ok((false, None));
            }
        }
    }

    // Wait for the claim to process
    sleep(Duration::from_millis(2000)).await;

    println!("{}", "✅ Daily cheese claimed successfully! 🧀".bright_green());

    // Try to read the cheese count
    let cheese_count_selectors = vec![
        By::Css("div.ubc-module-interface div span:contains('CHEESE PIECES')"),
        By::Css("div.ubc-module-interface span"),
    ];

    if let Ok(cheese_elem) = wait_for_element_multiple(driver, cheese_count_selectors, 2).await {
        if let Ok(cheese_text) = safe_driver_operation(cheese_elem.text()).await {
            if cheese_text.contains("CHEESE") {
                println!(
                    "{}",
                    format!("🧀 {}", cheese_text).cyan()
                );
            }
        }
    }

    // Wait a bit before closing
    sleep(Duration::from_millis(1000)).await;

    // Close the module by pressing Escape
    if let Err(_) = safe_driver_operation(
        driver.action_chain().send_keys(Key::Escape).perform()
    ).await {
        println!("{}", "⚠️ Could not send Escape key to close module".yellow());
    }

    // Wait for module to close
    sleep(Duration::from_millis(500)).await;

    Ok((true, None))
}

// Check and claim daily cheese if ready (legacy version for backward compatibility)
pub async fn check_and_claim_cheese(driver: &WebDriver) -> Result<bool> {
    let (claimed, _) = check_and_claim_cheese_with_wait(driver).await?;
    Ok(claimed)
}

// Result of a claim check: (claimed_successfully, wait_time_seconds)
pub type ClaimCheckResult = (bool, Option<u64>);

// Check and claim beetle if ready, returns (claimed, wait_time_seconds)
pub async fn check_and_claim_beetle_with_wait(driver: &WebDriver) -> Result<ClaimCheckResult> {
    println!("{}", "🪲 Checking for beetle claim...".cyan());

    // Try to find the beetle navigation item
    let beetle_nav_selectors = vec![
        By::Css("div.nav-item.beetle-game-nav"),
        By::Css("div[class*='beetle-game-nav']"),
        By::Css("div.nav-item:has(.icon.beetle)"),
    ];

    let beetle_nav = match wait_for_element_multiple(driver, beetle_nav_selectors, 5).await {
        Ok(element) => element,
        Err(_) => {
            println!("{}", "⚠️ Beetle nav item not found, skipping...".yellow());
            return Ok((false, None));
        }
    };

    // Check if beetle is ready by looking for the status text within the beetle nav element
    // Wait a bit to ensure the status text is loaded
    sleep(Duration::from_millis(500)).await;
    
    let status_text = match safe_driver_operation(beetle_nav.find(By::Css("div.info"))).await {
        Ok(info_div) => {
            // Find all spans within the info div
            match safe_driver_operation(info_div.find_all(By::Tag("span"))).await {
                Ok(spans) if spans.len() >= 2 => {
                    // Get the second span (index 1) which contains the status
                    match safe_driver_operation(spans[1].text()).await {
                        Ok(text) => {
                            let trimmed_text = text.trim().to_string();
                            println!("{}", format!("📊 Beetle status: '{}'", trimmed_text).bright_black());
                            
                            if trimmed_text.is_empty() {
                                println!("{}", "⚠️ Beetle status text is empty, trying inner_html...".yellow());
                                // Try getting the inner HTML as fallback
                                if let Ok(inner_html) = safe_driver_operation(spans[1].inner_html()).await {
                                    println!("{}", format!("📊 Beetle status (HTML): '{}'", inner_html).bright_black());
                                    inner_html.to_lowercase()
                                } else {
                                    trimmed_text.to_lowercase()
                                }
                            } else {
                                trimmed_text.to_lowercase()
                            }
                        }
                        Err(_) => {
                            println!("{}", "⚠️ Could not read beetle status text".yellow());
                            return Ok((false, None));
                        }
                    }
                }
                Ok(spans) => {
                    println!("{}", format!("⚠️ Found {} spans instead of 2 in beetle info", spans.len()).yellow());
                    return Ok((false, None));
                }
                Err(e) => {
                    println!("{}", format!("⚠️ Could not find spans in beetle info: {}", e).yellow());
                    return Ok((false, None));
                }
            }
        }
        Err(_) => {
            println!("{}", "⚠️ Beetle info div not found".yellow());
            return Ok((false, None));
        }
    };

    // Check if beetle is ready
    if !status_text.contains("ready") {
        let wait_time = parse_time_to_seconds(&status_text);
        println!(
            "{}",
            format!("⏳ Beetle not ready yet (status: {})", status_text).bright_black()
        );
        if let Some(seconds) = wait_time {
            println!(
                "{}",
                format!("⏰ Next claim in: {} TO NEXT CLAIM", format_duration(seconds)).cyan()
            );
        }
        return Ok((false, wait_time));
    }

    println!("{}", "✅ Beetle is ready! Opening beetle module...".green());

    // Click on the beetle nav item to open the module
    if let Err(e) = safe_driver_operation(beetle_nav.click()).await {
        println!("{}", format!("⚠️ Failed to click beetle nav: {}", e).yellow());
        return Ok((false, None));
    }

    // Wait a bit for the module to appear
    sleep(Duration::from_millis(1500)).await;

    // Try to find the beetle module
    let beetle_module_selectors = vec![
        By::Css("div.beetleModule"),
        By::Css("div[class*='beetleModule']"),
        By::Css("div.encounter-module-interface"),
    ];

    let _beetle_module = match wait_for_element_multiple(driver, beetle_module_selectors, 5).await {
        Ok(element) => element,
        Err(_) => {
            println!("{}", "⚠️ Beetle module did not appear, closing...".yellow());
            // Try to press Escape to close any modal
            if let Err(_) = safe_driver_operation(
                driver.action_chain().send_keys(Key::Escape).perform()
            ).await {
                println!("{}", "⚠️ Could not send Escape key".yellow());
            }
            return Ok((false, None));
        }
    };

    println!("{}", "🎯 Beetle module opened, checking toggle state...".cyan());

    // Check if toggle needs to be switched from "ejected" to "connected"
    let toggle_selectors = vec![
        By::Css("div.toggle-bar"),
        By::Css("div[class*='toggle-bar']"),
    ];

    if let Ok(toggle_bar) = wait_for_element_multiple(driver, toggle_selectors.clone(), 3).await {
        let toggle_class = safe_driver_operation(toggle_bar.class_name()).await
            .map(|class| class.unwrap_or_default())
            .unwrap_or_default();

        if toggle_class.contains("ejected") {
            println!("{}", "🔄 Toggle is ejected, switching to connected...".yellow());
            
            // Scroll toggle into view first
            if let Err(e) = safe_driver_operation(toggle_bar.scroll_into_view()).await {
                println!("{}", format!("⚠️ Could not scroll toggle into view: {}", e).yellow());
            }
            sleep(Duration::from_millis(500)).await;
            
            // Use JavaScript click directly to avoid interception
            let js_click = r#"arguments[0].click();"#;
            match safe_driver_operation(driver.execute(js_click, vec![toggle_bar.to_json()?])).await {
                Ok(_) => {
                    println!("{}", "✅ Clicked toggle using JavaScript".bright_black());
                }
                Err(e) => {
                    println!("{}", format!("⚠️ Failed to toggle with JS: {}, trying regular click...", e).yellow());
                    
                    // Fallback to regular click
                    if let Err(e) = safe_driver_operation(toggle_bar.click()).await {
                        println!("{}", format!("⚠️ Failed to click toggle: {}", e).yellow());
                    }
                }
            }
            
            // Wait for the toggle animation to complete
            sleep(Duration::from_millis(1000)).await;
            
            // Verify the toggle is now connected
            if let Ok(toggle_bar_after) = wait_for_element_multiple(driver, toggle_selectors, 2).await {
                let toggle_class_after = safe_driver_operation(toggle_bar_after.class_name()).await
                    .map(|class| class.unwrap_or_default())
                    .unwrap_or_default();
                
                if toggle_class_after.contains("connected") {
                    println!("{}", "✅ Toggle switched to connected!".green());
                } else {
                    println!("{}", "⚠️ Toggle may not have switched properly".yellow());
                }
            }
        } else if toggle_class.contains("connected") {
            println!("{}", "✅ Toggle already connected".green());
        }
    } else {
        println!("{}", "⚠️ Toggle bar not found, continuing anyway...".yellow());
    }

    println!("{}", "🎯 Looking for claim button...".cyan());

    // Try to find the claim button
    let claim_button_selectors = vec![
        By::Css("button.catch-button"),
        By::Css("button[class*='catch-button']"),
        By::Css("button:has(.button-text span:contains('CLAIM BEETLE'))"),
    ];

    let claim_button = match wait_for_element_multiple(driver, claim_button_selectors, 5).await {
        Ok(element) => element,
        Err(_) => {
            println!("{}", "⚠️ Claim button not found in beetle module".yellow());
            // Try to close the module by pressing Escape
            if let Err(_) = safe_driver_operation(
                driver.action_chain().send_keys(Key::Escape).perform()
            ).await {
                println!("{}", "⚠️ Could not send Escape key".yellow());
            }
            return Ok((false, None));
        }
    };


    // Robust check: class and disabled attribute
    let class_disabled = safe_driver_operation(claim_button.class_name()).await
        .map(|class| class.unwrap_or_default().contains("disabled"))
        .unwrap_or(false);
    let attr_disabled = safe_driver_operation(claim_button.attr("disabled")).await
        .map(|attr| attr.is_some())
        .unwrap_or(false);

    if class_disabled || attr_disabled {
        println!("{}", "⏳ Claim button is disabled (cooldown active)".yellow());
        // Try to read the cooldown timer
        let timer_selectors = vec![
            By::Css("button.catch-button .cooldown-timer"),
            By::Css("span.cooldown-timer"),
        ];
        let mut wait_time = None;
        if let Ok(timer_elem) = wait_for_element_multiple(driver, timer_selectors, 2).await {
            if let Ok(cooldown_text) = safe_driver_operation(timer_elem.text()).await {
                wait_time = parse_time_to_seconds(&cooldown_text);
                println!(
                    "{}",
                    format!("⏰ Cooldown: {}", cooldown_text).bright_black()
                );
            }
        }
        // Close the module by pressing Escape
        if let Err(_) = safe_driver_operation(
            driver.action_chain().send_keys(Key::Escape).perform()
        ).await {
            println!("{}", "⚠️ Could not send Escape key".yellow());
        }
        return Ok((false, wait_time));
    }

    println!("{}", "🎯 Clicking claim button...".green());

    // Scroll the button into view first to avoid interception
    if let Err(e) = safe_driver_operation(claim_button.scroll_into_view()).await {
        println!("{}", format!("⚠️ Could not scroll button into view: {}", e).yellow());
    }
    sleep(Duration::from_millis(500)).await;

    // Try to click using JavaScript as the regular click might be intercepted
    let js_click = r#"arguments[0].click();"#;
    match safe_driver_operation(driver.execute(js_click, vec![claim_button.to_json()?])).await {
        Ok(_) => {
            println!("{}", "✅ Clicked beetle claim button using JavaScript".bright_black());
        }
        Err(e) => {
            println!("{}", format!("⚠️ JavaScript click failed: {}, trying regular click...", e).yellow());
            // Fallback to regular click
            if let Err(e) = safe_driver_operation(claim_button.click()).await {
                println!("{}", format!("⚠️ Failed to click claim button: {}", e).yellow());
                // Try to close the module
                if let Err(_) = safe_driver_operation(
                    driver.action_chain().send_keys(Key::Escape).perform()
                ).await {
                    println!("{}", "⚠️ Could not send Escape key".yellow());
                }
                return Ok((false, None));
            }
        }
    }

    // Wait for the claim to process
    sleep(Duration::from_millis(2000)).await;

    println!("{}", "✅ Beetle claimed successfully! 🪲".bright_green());

    // Try to read the new cooldown timer
    let timer_selectors = vec![
        By::Css("button.catch-button .cooldown-timer"),
        By::Css("span.cooldown-timer"),
    ];

    let mut next_wait_time = None;
    if let Ok(timer_elem) = wait_for_element_multiple(driver, timer_selectors, 2).await {
        if let Ok(cooldown_text) = safe_driver_operation(timer_elem.text()).await {
            if !cooldown_text.trim().is_empty() {
                next_wait_time = parse_time_to_seconds(&cooldown_text);
                println!(
                    "{}",
                    format!("⏰ Next claim in: {} TO NEXT CLAIM", cooldown_text).cyan()
                );
            }
        }
    }

    // Wait a bit before closing
    sleep(Duration::from_millis(1000)).await;

    // Close the module by pressing Escape
    if let Err(_) = safe_driver_operation(
        driver.action_chain().send_keys(Key::Escape).perform()
    ).await {
        println!("{}", "⚠️ Could not send Escape key to close module".yellow());
    }

    // Wait for module to close
    sleep(Duration::from_millis(500)).await;

    Ok((true, next_wait_time))
}

// Check and claim beetle if ready (legacy version for backward compatibility)
pub async fn check_and_claim_beetle(driver: &WebDriver) -> Result<bool> {
    let (claimed, _) = check_and_claim_beetle_with_wait(driver).await?;
    Ok(claimed)
}

// Robust navigation to home page by clicking logo or fallback methods
pub async fn navigate_to_home_robust(driver: &WebDriver, config: &Config) -> Result<bool> {
    println!("{}", "🏠 Attempting to navigate to home page...".blue());

    // Method 1: Click on the Remilia logo (most natural way)
    println!("{}", "🎯 Method 1: Trying to click Remilia logo...".blue());

    let logo_selectors = vec![
        "a[href*='auroratielafamille'] img.logo.desktop-logo", // Specific desktop logo
        "a[href*='auroratielafamille'] img.logo", // Any logo in profile link
        "img.logo.desktop-logo", // Desktop logo
        "img.logo", // Any logo
        "a[data-discover='true'] img", // Logo with data-discover
        "img[alt='Remilia']", // Logo by alt text
        "a[href='/~auroratielafamille'] img" // Logo in profile link
    ];

    for selector in &logo_selectors {
        match wait_for_element(driver, By::Css(*selector), 3).await {
            Ok(logo_element) => {
                println!("{}", format!("✅ Found logo with selector: {}", selector).green());

                // Try to click the logo
                if let Ok(()) = safe_driver_operation(logo_element.click()).await {
                    println!("{}", "🎯 Clicked on logo, waiting for navigation...".yellow());
                    sleep(Duration::from_millis(3000)).await;

                    // Check if we successfully navigated
                    if let Ok(current_url) = safe_driver_operation(driver.current_url()).await {
                        println!(
                            "{}",
                            format!("📍 After logo click: {}", current_url.as_str()).bright_black()
                        );

                        if
                            current_url.as_str().contains("remilia.com/home") ||
                            (!current_url.as_str().contains("remilia.com/~") &&
                                current_url.as_str().contains("remilia.com"))
                        {
                            println!("{}", "✅ Logo click successfully navigated to home!".green());
                            return Ok(true);
                        }
                    }
                } else {
                    println!(
                        "{}",
                        format!("⚠️ Could not click logo with selector: {}", selector).yellow()
                    );
                }
            }
            Err(_) => {
                // Try next selector
                continue;
            }
        }
    }

    // Method 2: Try direct URL navigation as fallback
    println!("{}", "🔧 Method 2: Trying direct URL navigation...".blue());

    let home_variants = vec![&config.home_url, "https://remilia.com/", "https://remilia.com/home"];

    for url_to_try in &home_variants {
        println!("{}", format!("📍 Trying URL: {}", url_to_try).bright_black());

        if let Ok(()) = safe_driver_operation(driver.goto(*url_to_try)).await {
            sleep(Duration::from_millis(3000)).await;

            if let Ok(current_url) = safe_driver_operation(driver.current_url()).await {
                println!(
                    "{}",
                    format!("📍 After navigation: {}", current_url.as_str()).bright_black()
                );

                if
                    current_url.as_str().contains("remilia.com/home") ||
                    (!current_url.as_str().contains("remilia.com/~") &&
                        current_url.as_str().contains("remilia.com"))
                {
                    println!("{}", "✅ URL navigation succeeded!".green());
                    return Ok(true);
                }
            }
        }
    }

    // Method 3: JavaScript navigation as last resort
    println!("{}", "🔧 Method 3: Trying JavaScript navigation...".yellow());

    let js_commands = vec![
        "window.location.href = '/';".to_string(),
        "window.location.href = '/home';".to_string(),
        format!("window.location.href = '{}';", config.home_url)
    ];

    for js_cmd in &js_commands {
        if let Ok(_) = safe_driver_operation(driver.execute(js_cmd, vec![])).await {
            sleep(Duration::from_millis(3000)).await;

            if let Ok(current_url) = safe_driver_operation(driver.current_url()).await {
                if
                    current_url.as_str().contains("remilia.com/home") ||
                    (!current_url.as_str().contains("remilia.com/~") &&
                        current_url.as_str().contains("remilia.com"))
                {
                    println!("{}", "✅ JavaScript navigation succeeded!".green());
                    return Ok(true);
                }
            }
        }
    }

    println!("{}", "❌ All navigation methods failed".red());
    Ok(false)
}

pub async fn get_activity_lines(driver: &WebDriver) -> Result<Vec<WebElement>> {
    // Wait for activity feed to load with multiple attempts and alternative selectors
    let mut attempts = 0;
    let max_attempts = 3;

    // Try different selectors for activity feed
    let activity_selectors = vec![
        "div.activityfeed-body",
        "div[class*='activity']",
        "div[class*='feed']",
        "div[class*='timeline']",
        ".activity-feed",
        ".home-feed",
        ".main-feed"
    ];

    while attempts < max_attempts {
        // Try each selector
        for selector in &activity_selectors {
            match safe_driver_operation(driver.find(By::Css(*selector))).await {
                Ok(activity_body) => {
                    // Try different line selectors
                    let line_selectors = vec![
                        "div.activityfeed-line",
                        "div[class*='activity-line']",
                        "div[class*='feed-line']",
                        "div[class*='timeline-item']",
                        ".activity-item",
                        ".feed-item",
                        "div[class*='line']"
                    ];

                    for line_selector in &line_selectors {
                        if
                            let Ok(lines) = safe_driver_operation(
                                activity_body.find_all(By::Css(*line_selector))
                            ).await
                        {
                            if !lines.is_empty() {
                                // Only print on first attempt or when retrying
                                if attempts == 0 {
                                    println!(
                                        "{}",
                                        format!(
                                            "✅ Found {} activity lines",
                                            lines.len()
                                        ).bright_black()
                                    );
                                }
                                return Ok(lines);
                            }
                        }
                    }

                    // If we found the container but no lines, it might be loading
                    if attempts < max_attempts - 1 {
                        println!(
                            "{}",
                            format!(
                                "⏳ Activity container found but empty, waiting... (attempt {}/{})",
                                attempts + 1,
                                max_attempts
                            ).blue()
                        );
                        sleep(Duration::from_millis(2000)).await;
                        break; // Break from selector loop, continue with next attempt
                    }
                }
                Err(_) => {
                    // Try next selector
                    continue;
                }
            }
        }

        if attempts < max_attempts - 1 {
            println!(
                "{}",
                format!(
                    "⏳ No activity feed found with any selector, retrying... (attempt {}/{})",
                    attempts + 1,
                    max_attempts
                ).blue()
            );
            sleep(Duration::from_millis(2000)).await;
        }

        attempts += 1;
    }

    // Last resort: check current URL for debugging
    println!(
        "{}",
        "🔍 Activity feed not found with any selector. Let me check what's on the page...".yellow()
    );

    if let Ok(current_url) = safe_driver_operation(driver.current_url()).await {
        println!("{}", format!("Current URL: {}", current_url.as_str()).bright_black());

        // Check if we're on a profile page and need different approach
        if current_url.as_str().contains("remilia.com/~") {
            println!("{}", "💡 Still on profile page - maybe activity feed is here too?".blue());

            // Try to find any feed-like content on profile page
            if
                let Ok(profile_elements) = safe_driver_operation(
                    driver.find_all(
                        By::Css(
                            "div[class*='activity'], div[class*='feed'], div[class*='post'], div[class*='line']"
                        )
                    )
                ).await
            {
                if !profile_elements.is_empty() {
                    println!(
                        "{}",
                        format!(
                            "Found {} potential activity elements on profile page",
                            profile_elements.len()
                        ).blue()
                    );
                    return Ok(profile_elements);
                }
            }
        }

        return Err(
            anyhow::anyhow!(
                "Could not find activity feed after {} attempts with multiple selectors. URL: {}",
                max_attempts,
                current_url.as_str()
            )
        );
    }

    Err(anyhow::anyhow!("Could not find activity feed and couldn't determine current URL"))
}

// Check if current URL is an SSO login page and handle login automatically
pub async fn handle_sso_login_if_needed(driver: &WebDriver, config: &Config) -> Result<bool> {
    let current_url = safe_driver_operation(driver.current_url()).await?;

    // Check if we're on the SSO login page
    if current_url.as_str().contains("sso.remilia.org/realms/remilia/protocol/openid-connect") {
        println!("{}", "🔐 Detected SSO login page, attempting automatic login...".cyan());

        // First, check if we need to click the initial login button with wait
        match
            wait_for_element(
                driver,
                By::XPath(
                    "//*[@id=\"kc-content-wrapper\"]/div[2]/div/div[3]/div/div[2]/div[1]/button"
                ),
                5
            ).await
        {
            Ok(initial_btn) => {
                println!("{}", "🔄 Found and clicking initial login button...".yellow());
                safe_driver_operation(initial_btn.click()).await?;
                sleep(Duration::from_millis(3000)).await; // Give more time for form to load
            }
            Err(_) => {
                // Button not found, might already be on the form page
                println!("{}", "ℹ️ Initial login button not found, proceeding to form...".blue());
            }
        }

        // Wait for the login form to appear and find the username field
        println!("{}", "⏳ Waiting for login form to load...".blue());
        let username_selectors = vec![
            By::XPath("//*[@id=\"username\"]"),
            By::Id("username"),
            By::Css("input[name=\"username\"]"),
            By::Css("input[type=\"text\"]")
        ];
        match wait_for_element_multiple(driver, username_selectors, 15).await {
            Ok(username_field) => {
                println!("{}", "📧 Found username field, filling email...".yellow());
                safe_driver_operation(username_field.clear()).await?;
                safe_driver_operation(username_field.send_keys(&config.email)).await?;
            }
            Err(e) => {
                println!(
                    "{}",
                    format!("❌ Could not find username field after waiting: {}", e).red()
                );
                // Let's try to get the page source for debugging
                if let Ok(page_source) = safe_driver_operation(driver.source()).await {
                    println!("{}", "🔍 Page source (first 500 chars):".blue());
                    println!("{}", &page_source[..page_source.len().min(500)]);
                }
                return Ok(false);
            }
        }

        // Wait for and fill the password field
        let password_selectors = vec![
            By::XPath("//*[@id=\"password\"]"),
            By::Id("password"),
            By::Css("input[name=\"password\"]"),
            By::Css("input[type=\"password\"]")
        ];
        match wait_for_element_multiple(driver, password_selectors, 5).await {
            Ok(password_field) => {
                println!("{}", "🔑 Found password field, filling password...".yellow());
                safe_driver_operation(password_field.clear()).await?;
                safe_driver_operation(password_field.send_keys(&config.password)).await?;
            }
            Err(e) => {
                println!("{}", format!("❌ Could not find password field: {}", e).red());
                return Ok(false);
            }
        }

        // Wait for and click the Sign In button
        let signin_selectors = vec![
            By::XPath("//*[@id=\"kc-form-login\"]/div[4]/button"),
            By::Css("button[type=\"submit\"]"),
            By::Css(".login-form-button"),
            By::XPath("//button[contains(text(), 'Sign In')]"),
            By::XPath("//button[contains(@class, 'btn-primary')]")
        ];
        match wait_for_element_multiple(driver, signin_selectors, 5).await {
            Ok(signin_btn) => {
                println!("{}", "🚀 Found Sign In button, clicking...".yellow());
                safe_driver_operation(signin_btn.click()).await?;

                // Wait for login to process with longer timeout
                println!("{}", "⏳ Waiting for login to complete...".blue());
                sleep(Duration::from_millis(5000)).await;

                // Check if we're redirected away from the SSO page
                let new_url = safe_driver_operation(driver.current_url()).await?;
                if !new_url.as_str().contains("sso.remilia.org") {
                    println!("{}", "✅ Login successful! Redirected to main site.".green());
                    return Ok(true);
                } else {
                    println!("{}", "⚠️ Login may have failed - still on SSO page".yellow());
                    println!("{}", format!("Current URL: {}", new_url.as_str()).bright_black());
                    return Ok(false);
                }
            }
            Err(e) => {
                println!("{}", format!("❌ Could not find Sign In button: {}", e).red());
                return Ok(false);
            }
        }
    }

    Ok(false) // Not an SSO page
}

pub async fn extract_second_profile(
    line: &WebElement,
    processed_profiles: &ProcessedProfiles,
    config: &Config
) -> Result<Option<String>> {
    let anchors = line.find_all(By::Tag("a")).await.unwrap_or_default();

    if anchors.len() < 2 {
        // Not enough anchors, probably not a poke/friend activity
        return Ok(None);
    }

    let first = anchors[0].text().await.unwrap_or_default().trim().to_string();
    let second = anchors[1].text().await.unwrap_or_default().trim().to_string();

    // Debug: show what we found
    // println!("{}", format!("    🔍 Anchors: '{}' → '{}'", first, second).bright_black());

    // Ignore if initiated by us or target is us
    if first == config.my_username || second == config.my_username {
        // println!("{}", format!("    ⏭️  Skipping: involves our username '{}'", config.my_username).bright_black());
        return Ok(None);
    }

    // Ignore if already processed
    if processed_profiles.contains(&second) {
        // println!("{}", format!("    ⏭️  Skipping: '{}' already processed", second).bright_black());
        return Ok(None);
    }

    Ok(Some(second))
}

// Extract ALL profiles from an activity line (including previously seen ones)
// We'll check poke availability when we visit the profile
pub async fn extract_all_profiles_from_line(
    line: &WebElement,
    _processed_profiles: &ProcessedProfiles, // Keep parameter for API compatibility but don't filter by it
    config: &Config
) -> Result<Vec<String>> {
    let mut profiles = Vec::new();
    let anchors = line.find_all(By::Tag("a")).await.unwrap_or_default();

    if anchors.is_empty() {
        return Ok(profiles);
    }

    // Extract all profile links from the line
    for anchor in &anchors {
        if let Ok(username) = anchor.text().await {
            let username = username.trim().to_string();
            
            // Skip empty usernames
            if username.is_empty() {
                continue;
            }
            
            // Skip our own username (can't poke ourselves!)
            if username == config.my_username {
                continue;
            }
            
            // Skip duplicates within this line
            if profiles.contains(&username) {
                continue;
            }
            
            // ADD EVERYONE - we'll check poke availability when visiting the profile
            profiles.push(username);
        }
    }

    Ok(profiles)
}


pub async fn poke_and_add(
    driver: &WebDriver,
    profile_name: &str,
    processed_profiles: &mut ProcessedProfiles,
    config: &Config,
    session_stats: &mut crate::progress::SessionStats,
) -> Result<()> {
    // NOTE: We don't skip if profile was already processed!
    // We want to check if poke is available again (cooldown might have expired)
    
    // Open new tab
    if
        let Err(e) = safe_driver_operation(
            driver.execute("window.open('about:blank','_blank');", vec![])
        ).await
    {
        return Err(anyhow::anyhow!("Failed to open new tab: {}", e));
    }

    let handles = safe_driver_operation(driver.windows()).await?;

    // Check if we have any handles (browser might have been closed)
    let last_handle = handles
        .last()
        .ok_or_else(|| {
            anyhow::anyhow!("No browser windows available - browser may have been closed")
        })?;

    safe_driver_operation(driver.switch_to_window(last_handle.clone())).await?;

    // Load profile (URL with ~)
    let profile_url = format!("https://remilia.com/{}", profile_name);
    if let Err(e) = safe_driver_operation(driver.goto(&profile_url)).await {
        return Err(anyhow::anyhow!("Failed to navigate to profile {}: {}", profile_name, e));
    }

    // Delay for DOM & JS execution
    let mut rng = rand::thread_rng();
    let delay = Duration::from_millis(2000 + rng.gen_range(500..1500));
    sleep(delay).await;

    // Wait for friendship-row (optional)
    let _ = driver.find(By::Css("div.friendship-row")).await;

    // --- POKE ---
    // Try to find any poke button (be more flexible with selectors)
    let poke_selectors = vec![
        By::Css("button.btn.poke:not(.poke-cooldown):not([disabled])"), // Any enabled poke button
        By::Css("button.btn.poke[data-state='can-poke']"),
        By::Css("button.btn.poke.inverted:not(.poke-cooldown):not([disabled])"),
        By::Css("div.poke-friend-join > button.btn.poke"),
    ];

    let mut poke_found = false;
    let mut poke_status = String::new(); // For debugging
    
    for selector in &poke_selectors {
        match driver.find(selector.clone()).await {
            Ok(poke_btn) => {
                // Get button state info for debugging
                let is_displayed = poke_btn.is_displayed().await.unwrap_or(false);
                let is_enabled = poke_btn.is_enabled().await.unwrap_or(false);
                let data_state = safe_driver_operation(poke_btn.attr("data-state")).await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let class_name = safe_driver_operation(poke_btn.class_name()).await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let button_text = poke_btn.text().await.unwrap_or_default();
                
                poke_status = format!(
                    "display:{} enabled:{} state:'{}' class:'{}' text:'{}'",
                    is_displayed, is_enabled, data_state, class_name, button_text
                );
                
                // Check if button is clickable (displayed, enabled, and NOT on cooldown)
                let is_cooldown = class_name.contains("poke-cooldown") || 
                                  class_name.contains("disabled") ||
                                  data_state == "already-poked";
                
                if is_displayed && is_enabled && !is_cooldown {
                    poke_found = true;
                    println!(
                        "{}",
                        format!("🎯 [Debug] Poke button found for {}: {}", profile_name, poke_status).bright_black()
                    );
                    
                    if let Err(_) = poke_btn.click().await {
                        println!(
                            "{}",
                            format!("⚠️ [Warn] Error clicking poke for {}", profile_name).yellow()
                        );
                        session_stats.increment_pokes_unavailable();
                    } else {
                        println!(
                            "{}",
                            format!("🌸💖 [Proc] {} — poke sent ✨", profile_name).bright_magenta()
                        );
                        session_stats.increment_pokes_sent();
                    }
                    break;
                }
            }
            Err(_) => continue,
        }
    }

    if !poke_found {
        // Try to find any poke button to check its state
        if let Ok(any_poke_btn) = driver.find(By::Css("button.btn.poke")).await {
            let class_name = safe_driver_operation(any_poke_btn.class_name()).await
                .ok()
                .flatten()
                .unwrap_or_default();
            let data_state = safe_driver_operation(any_poke_btn.attr("data-state")).await
                .ok()
                .flatten()
                .unwrap_or_default();
            
            if class_name.contains("poke-cooldown") || data_state == "already-poked" {
                println!(
                    "{}",
                    format!("⏰ [Skip] Poke on cooldown for {} (class:'{}', state:'{}')", 
                        profile_name, class_name, data_state).bright_black()
                );
                session_stats.increment_pokes_cooldown();
            } else {
                println!(
                    "{}",
                    format!("💔 [Debug] Poke button found but not clickable for {}: {}", 
                        profile_name, poke_status).bright_black()
                );
            }
        } else {
            println!(
                "{}",
                format!("💔 [Skip] No poke button found for {}", profile_name).bright_black()
            );
        }
    }

    // Small delay after action
    let delay = Duration::from_millis(rng.gen_range(800..1600));
    sleep(delay).await;

    // --- ADD FRIEND ---
    // Try to find any add friend button (be more flexible)
    let friend_selectors = vec![
        By::Css("button.btn.friend:not([disabled])"), // Any enabled friend button
        By::Css("button.btn.friend[data-state='can-request']"),
        By::Css("button.btn.friend.inverted:not([disabled])"),
        By::Css("div.poke-friend-join > button.btn.friend"),
    ];

    let mut friend_found = false;
    let mut friend_status = String::new(); // For debugging
    
    for selector in &friend_selectors {
        match driver.find(selector.clone()).await {
            Ok(add_btn) => {
                // Get button state info
                let is_displayed = add_btn.is_displayed().await.unwrap_or(false);
                let is_enabled = add_btn.is_enabled().await.unwrap_or(false);
                let data_state = safe_driver_operation(add_btn.attr("data-state")).await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let class_name = safe_driver_operation(add_btn.class_name()).await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let text = add_btn.text().await.unwrap_or_default();
                
                friend_status = format!(
                    "display:{} enabled:{} state:'{}' class:'{}' text:'{}'",
                    is_displayed, is_enabled, data_state, class_name, text
                );
                
                // Check if already friends or pending
                let already_connected = data_state == "already-friends" || 
                                       data_state == "pending-approval" ||
                                       text.to_lowercase().contains("friends");
                
                if is_displayed && is_enabled && !already_connected {
                    friend_found = true;
                    println!(
                        "{}",
                        format!("🎯 [Debug] Friend button found for {}: {}", profile_name, friend_status).bright_black()
                    );
                    
                    if let Err(_) = add_btn.click().await {
                        println!(
                            "{}",
                            format!("⚠️ [Warn] Error clicking add friend for {}", profile_name).yellow()
                        );
                    } else {
                        println!(
                            "{}",
                            format!("💞🌈 [Proc] {} — new friend on the computer 🌸", profile_name).bright_cyan()
                        );
                        session_stats.increment_friends_added();
                    }
                    break;
                } else if already_connected {
                    friend_found = true;
                    println!(
                        "{}",
                        format!("✅ [Skip] Already friends with {} (state:'{}')", profile_name, data_state).bright_black()
                    );
                    break;
                }
            }
            Err(_) => continue,
        }
    }

    if !friend_found {
        // Try to find any friend button to check its state
        if let Ok(any_friend_btn) = driver.find(By::Css("button.btn.friend")).await {
            let _data_state = safe_driver_operation(any_friend_btn.attr("data-state")).await
                .ok()
                .flatten()
                .unwrap_or_default();
            
            println!(
                "{}",
                format!("💔 [Debug] Friend button found but not clickable for {}: {}", 
                    profile_name, friend_status).bright_black()
            );
        } else {
            println!(
                "{}",
                format!("💔 [Skip] No friend button found for {}", profile_name).bright_black()
            );
        }
    }

    // Mark as processed and save immediately
    processed_profiles.insert(profile_name.to_string());
    if let Err(e) = processed_profiles.save_to_file(&config.history_file) {
        println!("{}", format!("[Warn] Cannot save history: {}", e).yellow());
    }

    // Random pause then close tab
    let delay = Duration::from_millis(rng.gen_range(1000..2200));
    sleep(delay).await;

    // Safely close the current tab
    let _ = safe_driver_operation(driver.close_window()).await;

    // Return to main tab (home) - safely
    match safe_driver_operation(driver.windows()).await {
        Ok(handles) if !handles.is_empty() => {
            // Switch to the first available window
            if
                let Err(e) = safe_driver_operation(
                    driver.switch_to_window(handles[0].clone())
                ).await
            {
                println!(
                    "{}",
                    format!("⚠️ [Warn] Could not switch back to main window: {}", e).yellow()
                );
                // Try to navigate to home page as fallback
                let _ = safe_driver_operation(driver.goto(&config.home_url)).await;
            }
        }
        Ok(_) => {
            // No windows available - try to reopen home
            println!(
                "{}",
                "⚠️ [Warn] No browser windows available, attempting to navigate to home".yellow()
            );
            let _ = safe_driver_operation(driver.goto(&config.home_url)).await;
        }
        Err(e) => {
            // Browser might be completely closed
            return Err(anyhow::anyhow!("Browser appears to be closed: {}", e));
        }
    }

    Ok(())
}

pub async fn run_session(
    driver: &WebDriver,
    processed_profiles: &mut ProcessedProfiles,
    config: &Config,
    session_stats: &mut crate::progress::SessionStats,
) -> Result<()> {
    let mut action_count = 0;
    let mut beetle_next_check: Option<tokio::time::Instant> = None;
    let mut cheese_next_check: Option<tokio::time::Instant> = None;
    
    // Track profiles checked in THIS session to avoid duplicates within the same session
    let mut session_checked_profiles: std::collections::HashSet<String> = std::collections::HashSet::new();
    
    println!(
        "{}",
        format!(
            "✨🌸 New session started — limit {} actions 🌸✨",
            config.max_actions_per_session
        ).bright_yellow()
    );

    while action_count < config.max_actions_per_session {
        // Ensure we're on /home - with safe operation
        match safe_driver_operation(driver.current_url()).await {
            Ok(current_url) => {
                // Check if we've been redirected to SSO login
                if current_url.as_str().contains("sso.remilia.org") {
                    println!("{}", "🔐 Session redirected to SSO, handling login...".cyan());
                    let login_handled = handle_sso_login_if_needed(driver, config).await?;
                    if login_handled {
                        // Navigate back to home after successful login
                        safe_driver_operation(driver.goto(&config.home_url)).await?;
                    } else {
                        return Err(anyhow::anyhow!("SSO login failed during session"));
                    }
                } else if current_url.as_str().contains("remilia.com/~") {
                    // We're on a profile page, use robust navigation to home
                    println!(
                        "{}",
                        "🏠 On profile page, using robust navigation to reach activity feed...".blue()
                    );
                    let navigation_success = navigate_to_home_robust(driver, config).await?;
                    if !navigation_success {
                        println!(
                            "{}",
                            "⚠️ Could not reach home page with robust navigation, trying to continue...".yellow()
                        );
                    }
                } else if !current_url.as_str().starts_with(&config.home_url) {
                    println!(
                        "{}",
                        format!(
                            "🔄 Not on home page ({}), using robust navigation...",
                            current_url.as_str()
                        ).blue()
                    );
                    let navigation_success = navigate_to_home_robust(driver, config).await?;
                    if !navigation_success {
                        println!(
                            "{}",
                            "⚠️ Robust navigation failed, falling back to direct navigation...".yellow()
                        );
                        if let Err(e) = safe_driver_operation(driver.goto(&config.home_url)).await {
                            println!(
                                "{}",
                                format!("⚠️ [Error] Browser window closed: {}", e).red()
                            );
                            return Err(
                                anyhow::anyhow!("Browser session ended - window was closed")
                            );
                        }
                        sleep(Duration::from_millis(2000)).await;
                    }
                }
            }
            Err(e) => {
                println!("{}", format!("⚠️ [Error] Browser window closed: {}", e).red());
                return Err(anyhow::anyhow!("Browser session ended - window was closed"));
            }
        }

        // Check and claim beetle if ready (non-blocking, with timing control)
        let now = tokio::time::Instant::now();
        if beetle_next_check.is_none() || now >= beetle_next_check.unwrap() {
            match check_and_claim_beetle_with_wait(driver).await {
                Ok((claimed, wait_seconds)) => {
                    if claimed {
                        println!("{}", "🪲 Beetle claim completed!".bright_green());
                        // Reset the timer, it will be claimed again in ~8 hours
                        beetle_next_check = Some(now + Duration::from_secs(8 * 3600));
                    } else if let Some(seconds) = wait_seconds {
                        // Set next check time based on when it will be ready
                        let next_check = now + Duration::from_secs(seconds);
                        beetle_next_check = Some(next_check);
                        println!(
                            "{}",
                            format!("⏰ Next beetle check in: {} (at {:?})", format_duration(seconds), next_check).bright_black()
                        );
                    }
                    // Continue regardless of result - this is a bonus action
                }
                Err(e) => {
                    println!(
                        "{}",
                        format!("⚠️ [Warn] Beetle check failed: {}, continuing...", e).yellow()
                    );
                    // Don't stop the session for beetle errors, retry in 5 minutes
                    beetle_next_check = Some(now + Duration::from_secs(300));
                }
            }
        }

        // Check and claim daily cheese if ready (non-blocking, with timing control)
        if cheese_next_check.is_none() || now >= cheese_next_check.unwrap() {
            match check_and_claim_cheese_with_wait(driver).await {
                Ok((claimed, wait_seconds)) => {
                    if claimed {
                        println!("{}", "🧀 Daily cheese claim completed!".bright_green());
                        // Reset the timer, it will be claimed again in ~24 hours
                        cheese_next_check = Some(now + Duration::from_secs(24 * 3600));
                    } else if let Some(seconds) = wait_seconds {
                        // Set next check time based on when it will be ready
                        let next_check = now + Duration::from_secs(seconds);
                        cheese_next_check = Some(next_check);
                        println!(
                            "{}",
                            format!("⏰ Next cheese check in: {} (at {:?})", format_duration(seconds), next_check).bright_black()
                        );
                    }
                    // Continue regardless of result - this is a bonus action
                }
                Err(e) => {
                    println!(
                        "{}",
                        format!("⚠️ [Warn] Cheese check failed: {}, continuing...", e).yellow()
                    );
                    // Don't stop the session for cheese errors, retry in 5 minutes
                    cheese_next_check = Some(now + Duration::from_secs(300));
                }
            }
        }

        let lines = match get_activity_lines(driver).await {
            Ok(lines) => lines,
            Err(e) => {
                println!("{}", format!("⚠️ [Warn] Failed to get activity lines: {}", e).yellow());

                // Check if we're on the wrong page and try to navigate to home
                if let Ok(current_url) = safe_driver_operation(driver.current_url()).await {
                    if !current_url.as_str().contains("remilia.com/home") {
                        println!(
                            "{}",
                            format!(
                                "🔄 Not on home page ({}), trying to navigate...",
                                current_url.as_str()
                            ).blue()
                        );
                        if safe_driver_operation(driver.goto(&config.home_url)).await.is_ok() {
                            sleep(Duration::from_millis(3000)).await;
                            // Try one more time after navigation
                            match get_activity_lines(driver).await {
                                Ok(lines) => lines,
                                Err(_) => {
                                    println!(
                                        "{}",
                                        "⚠️ [Warn] Still can't find activity feed after navigation, waiting...".yellow()
                                    );
                                    sleep(Duration::from_millis(5000)).await;
                                    continue;
                                }
                            }
                        } else {
                            return Err(
                                anyhow::anyhow!(
                                    "Browser session ended - could not navigate to home"
                                )
                            );
                        }
                    } else {
                        println!(
                            "{}",
                            "⚠️ [Warn] On home page but no activity feed found, waiting...".yellow()
                        );
                        sleep(Duration::from_millis(5000)).await;
                        continue;
                    }
                } else {
                    return Err(
                        anyhow::anyhow!("Browser session ended - could not access page content")
                    );
                }
            }
        };
        if lines.is_empty() {
            let mut rng = rand::thread_rng();
            let wait = rng.gen_range(4.0..7.0);
            println!(
                "{}",
                format!("⏳ [Info] No visible events — waiting {:.1}s...", wait).bright_black()
            );
            sleep(Duration::from_secs_f64(wait)).await;
            continue;
        }

        // Extract ALL profiles from all activity lines (even previously seen ones)
        // We'll check poke availability when visiting each profile
        let mut candidate_profiles = Vec::new();
        let mut profile_seen_count = 0;
        let mut profile_new_count = 0;
        let mut profile_session_skip_count = 0;
        
        for (_idx, line) in lines.iter().enumerate() {
            // Use the new function that extracts ALL profiles from each line
            match extract_all_profiles_from_line(line, processed_profiles, config).await {
                Ok(line_profiles) => {
                    for profile in line_profiles {
                        // Skip if already checked in this session
                        if session_checked_profiles.contains(&profile) {
                            profile_session_skip_count += 1;
                            continue;
                        }
                        
                        // Check if profile is already in our batch (deduplicate within this scan)
                        if !candidate_profiles.contains(&profile) {
                            // Track if this is a profile we've seen before
                            if processed_profiles.contains(&profile) {
                                profile_seen_count += 1;
                            } else {
                                profile_new_count += 1;
                            }
                            
                            candidate_profiles.push(profile);
                            
                            if candidate_profiles.len() >= config.max_profiles_per_pass {
                                break;
                            }
                        }
                        // Skip duplicates within this batch silently
                    }
                    
                    if candidate_profiles.len() >= config.max_profiles_per_pass {
                        break;
                    }
                }
                Err(_e) => {
                    // Error extracting from this line, skip it
                }
            }
        }

        // Summary of analysis
        if !candidate_profiles.is_empty() {
            let mut summary = format!(
                "🎯 Found {} profile(s): {} new, {} previously seen",
                candidate_profiles.len(),
                profile_new_count,
                profile_seen_count
            );
            if profile_session_skip_count > 0 {
                summary.push_str(&format!(" ({} already checked this session)", profile_session_skip_count));
            }
            println!("{}", summary.bright_green());
        } else {
            let mut summary = format!("📭 No profiles found ({} lines analyzed)", lines.len());
            if profile_session_skip_count > 0 {
                summary.push_str(&format!(" - {} already checked this session", profile_session_skip_count));
            }
            println!("{}", summary.bright_black());
        }

        if candidate_profiles.is_empty() {
            // No profiles at all
            let mut rng = rand::thread_rng();
            let wait = rng.gen_range(3.0..6.0);
            sleep(Duration::from_secs_f64(wait)).await;
            continue;
        }

        // Process the profiles list
        for prof in candidate_profiles {
            // Break if limit reached
            if action_count >= config.max_actions_per_session {
                break;
            }
            
            let was_processed = processed_profiles.contains(&prof);
            if was_processed {
                println!("{}", format!("➡️ [Revisit] Checking: {} (seen before, checking poke availability)...", prof).cyan());
            } else {
                println!("{}", format!("➡️ [New] Processing: {} ...", prof).magenta());
            }

            // Action - handle window closure gracefully
            match poke_and_add(driver, &prof, processed_profiles, config, session_stats).await {
                Ok(()) => {
                    action_count += 1;
                    // Mark as checked in this session
                    session_checked_profiles.insert(prof.clone());
                    println!(
                        "{}",
                        format!("✅ [Success] Completed processing for {}", prof).green()
                    );
                }
                Err(e) if
                    e.to_string().contains("Browser window was closed") ||
                    e.to_string().contains("Browser appears to be closed")
                => {
                    println!(
                        "{}",
                        format!("⚠️ [Info] Browser window closed while processing {}, ending session gracefully", prof).yellow()
                    );
                    return Ok(()); // End session gracefully instead of erroring
                }
                Err(e) => {
                    println!("{}", format!("⚠️ [Warn] Error processing {}: {}", prof, e).yellow());
                    // Still mark as checked to avoid retrying in this session
                    session_checked_profiles.insert(prof.clone());
                    // Continue with next profile instead of stopping the whole session
                    continue;
                }
            }

            // Pause between profiles
            let mut rng = rand::thread_rng();
            let delay = Duration::from_millis(rng.gen_range(1000..3000));
            sleep(delay).await;
        }
    }

    println!(
        "{}",
        format!("✨🌸 [Session] Limit reached ({} actions). 🌸✨", action_count).bright_yellow()
    );
    
    // Display session summary
    session_stats.display_summary();
    
    Ok(())
}
