use anyhow::Result;
use std::env;

mod config;
mod html_generator;
mod leaderboard;
mod models;
mod utils;

use html_generator::HtmlGenerator;
use leaderboard::LeaderboardFetcher;
use models::SortBy;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    let mut test_mode = false;
    let mut arg_offset = 1;
    
    // Check for test flag
    if args.len() > 1 && args[1] == "--test" {
        test_mode = true;
        arg_offset = 2;
        println!("🧪 TEST MODE: Will only process first 10 users");
        println!();
    }
    
    let sort_by = if args.len() > arg_offset {
        match args[arg_offset].to_lowercase().as_str() {
            "beetles" => SortBy::Beetles,
            "pokes" => SortBy::Pokes,
            "social" | "socialcredit" | "credit" => SortBy::SocialCredit,
            _ => {
                eprintln!("Invalid sort criteria. Using default (beetles).");
                eprintln!("Valid options: beetles, pokes, social");
                SortBy::Beetles
            }
        }
    } else {
        SortBy::Beetles
    };

    let output_file = if args.len() > arg_offset + 1 {
        args[arg_offset + 1].clone()
    } else {
        "leaderboard.html".to_string()
    };

    println!("╔══════════════════════════════════════╗");
    println!("║   Remilia Leaderboard Generator     ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("Sort by: {:?}", sort_by);
    println!("Output file: {}", output_file);
    println!();

    // Fetch leaderboard data
    let mut fetcher = LeaderboardFetcher::new();
    if test_mode {
        fetcher.set_test_mode(10);
    }
    let stats = fetcher.generate_leaderboard(sort_by).await?;

    println!();
    println!("Generating HTML...");

    // Generate HTML
    let html = HtmlGenerator::generate_html(&stats, sort_by);

    // Save to file
    HtmlGenerator::save_to_file(&html, &output_file)?;

    println!("✓ Leaderboard saved to: {}", output_file);
    println!();
    println!("Top 5 users:");
    for (idx, entry) in stats.entries.iter().take(5).enumerate() {
        println!("  {}. {} (@{}) - Beetles: {}, Pokes: {}, Social Credit: {}",
            idx + 1,
            entry.display_name,
            entry.username,
            entry.beetles,
            entry.pokes,
            entry.social_credit
        );
    }

    Ok(())
}
