use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;

use crate::models::{LeaderboardEntry, LeaderboardStats, ProfileResponse, SortBy};

const API_BASE_URL: &str = "https://www.remilia.com/api/profile/";
const FRIENDS_DB_PATH: &str = "friends_db.json";
const MAX_RETRIES: u32 = 30;
const INITIAL_RETRY_DELAY_MS: u64 = 2000;

pub struct LeaderboardFetcher {
    client: Client,
    test_mode_limit: Option<usize>,
}

impl LeaderboardFetcher {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            test_mode_limit: None,
        }
    }

    pub fn set_test_mode(&mut self, limit: usize) {
        self.test_mode_limit = Some(limit);
    }

    /// Load usernames from friends_db.json
    pub fn load_usernames(&self) -> Result<Vec<String>> {
        let content =
            fs::read_to_string(FRIENDS_DB_PATH).context("Failed to read friends_db.json")?;

        let json: HashMap<String, Value> =
            serde_json::from_str(&content).context("Failed to parse friends_db.json")?;

        let mut usernames: Vec<String> = json.keys().cloned().collect();

        // Add own user if not already in the list
        if !usernames.contains(&"mao".to_string()) {
            usernames.push("mao".to_string());
        }

        Ok(usernames)
    }

    /// Fetch profile for a single user
    pub async fn fetch_profile(&self, username: &str) -> Result<ProfileResponse> {
        let url = format!("{}~{}", API_BASE_URL, username);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to fetch profile for {}", username))?;

        if !response.status().is_success() {
            anyhow::bail!(
                "API returned error status for {}: {}",
                username,
                response.status()
            );
        }

        let profile = response
            .json::<ProfileResponse>()
            .await
            .context(format!("Failed to parse profile response for {}", username))?;

        Ok(profile)
    }

    /// Fetch all profiles and build leaderboard
    pub async fn build_leaderboard(&self) -> Result<LeaderboardStats> {
        let mut usernames = self.load_usernames()?;

        // Apply test mode limit if set
        if let Some(limit) = self.test_mode_limit {
            usernames.truncate(limit);
        }

        let mut stats = LeaderboardStats::new();
        let total = usernames.len();

        println!("Fetching profiles for {} users...", total);
        println!("Processing in batches of 100 concurrent requests");

        // Process in chunks of 25 to avoid rate limiting (reduced from 50)
        const BATCH_SIZE: usize = 100;
        let mut processed = 0;
        let mut failed_429_count = 0;

        for chunk in usernames.chunks(BATCH_SIZE) {
            let chunk_size = chunk.len();
            println!(
                "Processing batch: {}-{}/{}",
                processed + 1,
                processed + chunk_size,
                total
            );

            // Create futures for all requests in this batch
            let mut fetch_tasks = Vec::new();
            for username in chunk {
                let username = username.clone();
                let client = self.client.clone();

                let task = tokio::spawn(async move {
                    let url = format!("{}~{}", API_BASE_URL, username);

                    // Retry logic for 429 errors
                    let mut attempts = 0;
                    loop {
                        let response = client.get(&url).send().await;

                        match response {
                            Ok(resp) if resp.status().is_success() => {
                                // First get the raw text to debug parsing errors
                                let text = match resp.text().await {
                                    Ok(t) => t,
                                    Err(e) => {
                                        return Err(format!(
                                            "Failed to read response for {}: {}",
                                            username, e
                                        ));
                                    }
                                };

                                match serde_json::from_str::<ProfileResponse>(&text) {
                                    Ok(profile) => return Ok((username, profile)),
                                    Err(e) => {
                                        // Save the problematic response for debugging
                                        let debug_file =
                                            format!("debug_response_{}.json", username);
                                        let _ = std::fs::write(&debug_file, &text);
                                        return Err(format!(
                                            "Parse error for {}: {} (saved to {})",
                                            username, e, debug_file
                                        ));
                                    }
                                }
                            }
                            Ok(resp) if resp.status().as_u16() == 429 => {
                                attempts += 1;
                                if attempts >= MAX_RETRIES {
                                    return Err(format!(
                                        "HTTP 429 Too Many Requests for {} (max retries exceeded)",
                                        username
                                    ));
                                }

                                // Exponential backoff: 2s, 4s, 8s
                                let delay = INITIAL_RETRY_DELAY_MS * 2_u64.pow(attempts - 1);
                                eprintln!(
                                    "Rate limited for {}. Retrying in {}ms... (attempt {}/{})",
                                    username, delay, attempts, MAX_RETRIES
                                );
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                                continue;
                            }
                            Ok(resp) => {
                                return Err(format!("HTTP {} for {}", resp.status(), username));
                            }
                            Err(e) => return Err(format!("Request error for {}: {}", username, e)),
                        }
                    }
                });

                fetch_tasks.push(task);
            }

            // Wait for all tasks in this batch to complete
            let results = futures::future::join_all(fetch_tasks).await;

            // Process results
            for result in results {
                match result {
                    Ok(Ok((username, profile))) => {
                        let entry = LeaderboardEntry {
                            username: profile.user.username.clone(),
                            display_name: profile.user.display_name.clone(),
                            beetles: profile.user.beetles,
                            pokes: profile.user.pokes,
                            social_credit: profile.user.social_credit.score,
                            pfp_url: Some(profile.user.pfp_url.clone()),
                        };
                        stats.add_entry(entry);
                    }
                    Ok(Err(e)) => {
                        if e.contains("429") {
                            failed_429_count += 1;
                        }
                        eprintln!("Error: {}", e);
                    }
                    Err(e) => {
                        eprintln!("Task error: {}", e);
                    }
                }
            }

            processed += chunk_size;
            println!(
                "  ✓ Completed batch. Total fetched so far: {} (failed 429s: {})",
                stats.total_users, failed_429_count
            );

            // Dynamic delay based on 429 errors
            if processed < total {
                let delay = if failed_429_count > 10 {
                    // If we're getting a lot of 429s, slow down significantly
                    5000
                } else if failed_429_count > 5 {
                    4000
                } else {
                    3000
                };

                if failed_429_count > 0 {
                    println!(
                        "  ⏳ Waiting {}ms before next batch (rate limit issues detected)...",
                        delay
                    );
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }
        }

        println!(
            "Completed! Successfully fetched {} profiles",
            stats.total_users
        );
        Ok(stats)
    }

    /// Generate leaderboard sorted by criteria
    pub async fn generate_leaderboard(&self, sort_by: SortBy) -> Result<LeaderboardStats> {
        let mut stats = self.build_leaderboard().await?;
        stats.sort_by(sort_by);
        Ok(stats)
    }
}

impl Default for LeaderboardFetcher {
    fn default() -> Self {
        Self {
            client: Client::new(),
            test_mode_limit: None,
        }
    }
}
