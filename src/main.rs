use anyhow::{Context, Result};
use rand::rngs::OsRng;
use rand::{Rng, distributions::Alphanumeric};
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::signal;
use tokio::time::{Duration, interval};

// ===== SOCKET.IO STRUCTS =====

#[derive(Debug, Deserialize)]
struct SocketInitResponse {
    sid: String,
    #[allow(dead_code)]
    upgrades: Vec<String>,
    #[serde(rename = "pingInterval")]
    #[allow(dead_code)]
    ping_interval: u32,
    #[serde(rename = "pingTimeout")]
    #[allow(dead_code)]
    ping_timeout: u32,
    #[serde(rename = "maxPayload")]
    #[allow(dead_code)]
    max_payload: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct FeedUser {
    username: String,
    avatar: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
enum FeedActivity {
    #[serde(rename = "poke")]
    Poke {
        id: String,
        timestamp: String,
        from: FeedUser,
        to: FeedUser,
    },
    #[serde(rename = "friendship")]
    Friendship {
        id: String,
        timestamp: String,
        users: Vec<FeedUser>,
    },
}

// Cookie structure matching your JSON
#[derive(Debug, Deserialize)]
struct Cookie {
    name: String,
    value: String,
    #[allow(dead_code)]
    domain: String,
    #[allow(dead_code)]
    path: String,
    #[allow(dead_code)]
    secure: bool,
    #[allow(dead_code)]
    http_only: Option<bool>,
    #[allow(dead_code)]
    same_site: String,
    #[allow(dead_code)]
    expiry: u64,
}

fn load_auth_token() -> Result<String> {
    let token = fs::read_to_string("auth.txt")
        .context("Failed to read auth.txt")?
        .trim()
        .to_string();

    Ok(token)
}

fn load_remilia_cookies() -> Result<(String, String)> {
    let json_data = fs::read_to_string("remilia_cookies.json")
        .context("Failed to read remilia_cookies.json")?;

    let cookies: Vec<Cookie> =
        serde_json::from_str(&json_data).context("Failed to parse remilia_cookies.json")?;

    let mut profile_sid = None;
    let mut beetle_sid = None;

    for cookie in cookies {
        match cookie.name.as_str() {
            "profile.sid" => profile_sid = Some(cookie.value.clone()),
            "beetle.sid" => beetle_sid = Some(cookie.value.clone()),
            _ => {}
        }
    }

    let profile_sid =
        profile_sid.context("profile.sid cookie not found in remilia_cookies.json")?;
    let beetle_sid = beetle_sid.context("beetle.sid cookie not found in remilia_cookies.json")?;

    println!("✅ Loaded profile.sid and beetle.sid cookies");

    Ok((profile_sid, beetle_sid))
}

// ===== AUTH STATUS ENDPOINT =====

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatusResponse {
    authenticated: bool,
    #[serde(default)]
    user: Option<AuthUser>,
    #[serde(default)]
    #[allow(dead_code)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthUser {
    #[allow(dead_code)]
    sub: String,
    username: String,
    #[allow(dead_code)]
    email: String,
    #[serde(rename = "tokenExpiration")]
    #[allow(dead_code)]
    token_expiration: u64,
    #[allow(dead_code)]
    pfp: Pfp,
    #[allow(dead_code)]
    pfp_url: String,
    display_name: String,
    #[allow(dead_code)]
    theme: String,
    #[allow(dead_code)]
    cover: String,
    #[allow(dead_code)]
    color: u32,
    #[allow(dead_code)]
    onboarded: bool,
    pokes: u32,
    page_views: u32,
    friend_count: u32,
}

// ===== PROFILE ENDPOINT =====

#[derive(Debug, Deserialize, Serialize)]
pub struct ProfileResponse {
    pub user: UserProfile,
    #[serde(rename = "isAuthenticated")]
    pub is_authenticated: bool,
    #[serde(rename = "isOwnProfile")]
    pub is_own_profile: bool,
    #[serde(rename = "currentUsername")]
    pub current_username: Option<String>,
    #[serde(rename = "extraContext")]
    pub extra_context: Option<ExtraContext>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UserProfile {
    pub username: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub pfp: Pfp,
    #[serde(rename = "pfpUrl")]
    pub pfp_url: String,
    pub bio: Option<String>,
    #[serde(rename = "friendCount")]
    pub friend_count: u32,
    pub pokes: u32,
    pub beetles: u32,
    pub theme: String,
    pub cover: String,
    pub color: u32,
    pub location: Option<String>,
    // Use flatten to capture all other fields without failing
    #[serde(flatten)]
    pub other: serde_json::Value,
}

// #[derive(Debug, Deserialize)]
// #[serde(rename_all = "camelCase")]
// struct Friend {
//     display_username: String,
//     display_name: String,
//     pfp: Pfp,
//     pfp_url: String,
// }

// Unused - kept for potential future use
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Achievement {
    #[allow(dead_code)]
    id: u32,
    granted_at: String,
    #[serde(rename = "_id")]
    mongo_id: String,
    title: String,
    description: String,
    grant_message: String,
    #[serde(rename = "type")]
    achievement_type: String,
    trigger: String,
    season: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ExtraContext {
    #[serde(rename = "areFriends")]
    pub are_friends: bool,
    #[serde(rename = "pendingRequestFrom")]
    pub pending_request_from: bool,
    #[serde(rename = "pendingRequestTo")]
    pub pending_request_to: bool,
    #[serde(rename = "canPoke")]
    pub can_poke: bool,
    #[serde(rename = "pokeCooldownSeconds")]
    pub poke_cooldown_seconds: u32,
    #[serde(rename = "mutualCount")]
    pub mutual_count: u32,
    // Capture everything else
    #[serde(flatten)]
    pub other: serde_json::Value,
}

// Unused - kept for potential future use
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AchievementContext {
    viewer_owns: bool,
    ownership_percentage: u32,
    season: u32,
}

// Unused - kept for potential future use
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TwitterMutuals {
    display: Vec<String>,
    count: u32,
}

// Unused - kept for potential future use
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SocialCredit {
    score: u32,
    last_calculated: String,
    components: SocialCreditComponents,
}

// Unused - kept for potential future use
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SocialCreditComponents {
    base: u32,
    onboarding: u32,
    aggregate_bonus: u32,
    aggregate_scores: AggregateScores,
    friend_bonus: u32,
    #[serde(rename = "final")]
    final_score: u32,
}

// Unused - kept for potential future use
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AggregateScores {
    miladychan: u32,
    twitter: u32,
    profiles: u32,
    beetle_game: u32,
    miladycraft: u32,
    ethereum: u32,
}

// #[derive(Debug, Deserialize)]
// struct Pfp {
//     project: String,
//     id: String,
// }

// ===== COOLDOWNS ENDPOINT =====

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CooldownsResponse {
    cooldowns: Cooldowns,
    hunt_info: HuntInfo,
}

impl CooldownsResponse {
    #[allow(dead_code)]
    pub fn can_beetle_catch(&self) -> bool {
        self.cooldowns.catch_beetle == 0
    }

    pub fn can_claim_ubc(&self) -> bool {
        self.cooldowns.claim_ubc == 0
    }

    #[allow(dead_code)]
    pub fn has_hunts_remaining(&self) -> bool {
        self.hunt_info.hunts_used < 3
    }

    #[allow(dead_code)]
    pub fn time_until_beetle_ready(&self) -> u64 {
        self.cooldowns.catch_beetle / 1000
    }

    pub fn time_until_ubc_ready(&self) -> u64 {
        self.cooldowns.claim_ubc / 1000
    }

    #[allow(dead_code)]
    pub fn time_until_hunt_reset(&self) -> u64 {
        // If resetTime is 0, hunts are already available
        if self.hunt_info.reset_time == 0 {
            return 0;
        }

        let reset_time_secs = self.hunt_info.reset_time / 1000;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if reset_time_secs > now {
            reset_time_secs - now
        } else {
            0 // Reset time has passed
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cooldowns {
    #[serde(rename = "catchBeetle", default)]
    catch_beetle: u64,
    #[serde(rename = "claimUBC", default)]
    claim_ubc: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HuntInfo {
    hunts_used: u32,
    reset_time: u64,
}

// ===== CLAIM UBC ENDPOINT (Daily cheese claim) =====

#[derive(Debug, Serialize)]
struct ClaimUBCRequest {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
enum ClaimUBCApiResponse {
    Success {
        success: bool,
        result: ClaimUBCResult,
        user: User,
    },
    Error {
        success: bool,
        error: String,
        user: User,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimUBCResult {
    cheese: u32,
    xp: u32,
    streak: u32,
}

// ===== CATCH BEETLE ENDPOINT (Cooldown-based) =====

#[derive(Debug, Serialize)]
struct CatchBeetleRequest {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)] // Try to match different response shapes
enum CatchBeetleApiResponse {
    Success {
        success: bool,
        result: CatchBeetleResult,
        user: User,
    },
    Error {
        success: bool,
        error: String,
        user: User,
    },
}

#[derive(Debug, Deserialize, Clone)]
struct CatchBeetleResult {
    beetle: String,
    beetle_name: String,
    xp: u32,
    #[serde(rename = "cooldownMs")]
    cooldown_ms: u64,
    #[serde(rename = "catchVideoInfo")]
    #[allow(dead_code)]
    catch_video_info: VideoInfo,
    #[serde(rename = "beetleCard")]
    #[allow(dead_code)]
    beetle_card: BeetleCard,
}

#[derive(Debug, Deserialize, Clone)]
struct BeetleCard {
    #[allow(dead_code)]
    beetle_name: String,
    #[allow(dead_code)]
    girl_name: String,
    species: String,
    #[allow(dead_code)]
    species_latin: String,
    #[allow(dead_code)]
    icon: String,
    #[allow(dead_code)]
    background: String,
    #[allow(dead_code)]
    character: String,
}

// ===== SHARED STRUCTS (use once, don't duplicate!) =====

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct User {
    #[allow(dead_code)]
    id: String,
    cheese: u32,
    xp: u32,
    level: u32,
    inventory: BeetleInventory,
    #[allow(dead_code)]
    discovered: Vec<String>,
    streaks: Streaks,
    #[serde(rename = "beetleHuntsUsed")]
    beetle_hunts_used: u32,
    #[serde(rename = "lastBeetleHuntDate")]
    last_beetle_hunt_date: u64,
    #[serde(rename = "levelInfo")]
    level_info: LevelInfo,
    cooldowns: Cooldowns,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct BeetleInventory {
    #[serde(default)]
    pub green: u32,
    #[serde(default)]
    pub ladybug: u32,
    #[serde(default)]
    pub monarch: u32,
    #[serde(default)]
    pub pond: u32,
    #[serde(default)]
    pub bombardier: u32,
    #[serde(default)]
    pub purple: u32,
    #[serde(default)]
    pub skull: u32,
}

impl BeetleInventory {
    pub fn total_beetles(&self) -> u32 {
        self.green
            + self.ladybug
            + self.monarch
            + self.pond
            + self.bombardier
            + self.purple
            + self.skull
    }

    pub fn get_sorted_beetles(&self) -> Vec<(&str, &str, u32, &str)> {
        vec![
            ("🟠 Monarch", "Rare", self.monarch, "monarch"),
            ("⚫ Bombardier", "Rare", self.bombardier, "bombardier"),
            ("🟣 Purple", "Uncommon", self.purple, "purple"),
            ("🔵 Pond", "Uncommon", self.pond, "pond"),
            ("🔴 Ladybug", "Common", self.ladybug, "ladybug"),
            ("🟢 Green", "Common", self.green, "green"),
            ("💀 Skull", "Rare", self.skull, "skull"),
        ]
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionBeetleInventory {
    pub current: BeetleInventory,
    pub session_start: BeetleInventory,
}

impl SessionBeetleInventory {
    pub fn new(initial: BeetleInventory) -> Self {
        Self {
            current: initial.clone(),
            session_start: initial,
        }
    }

    pub fn update(&mut self, new_inventory: BeetleInventory) {
        self.current = new_inventory;
    }

    pub fn get_session_delta(&self, beetle_type: &str) -> i32 {
        let current = self.get_count(&self.current, beetle_type);
        let start = self.get_count(&self.session_start, beetle_type);
        current as i32 - start as i32
    }

    fn get_count(&self, inv: &BeetleInventory, beetle_type: &str) -> u32 {
        match beetle_type {
            "green" => inv.green,
            "ladybug" => inv.ladybug,
            "monarch" => inv.monarch,
            "pond" => inv.pond,
            "bombardier" => inv.bombardier,
            "skull" => inv.skull,
            "purple" => inv.purple,
            _ => 0,
        }
    }

    pub fn total_beetles(&self) -> u32 {
        self.current.green
            + self.current.ladybug
            + self.current.monarch
            + self.current.pond
            + self.current.bombardier
            + self.current.purple
            + self.current.skull
    }

    pub fn total_session_gain(&self) -> i32 {
        let current_total = self.total_beetles();
        let start_total = self.session_start.green
            + self.session_start.ladybug
            + self.session_start.monarch
            + self.session_start.pond
            + self.session_start.bombardier
            + self.session_start.purple
            + self.session_start.skull;

        current_total as i32 - start_total as i32
    }

    pub fn get_sorted_beetles(&self) -> Vec<(&str, &str, u32, i32, &str)> {
        vec![
            (
                "🟠 Monarch",
                "Rare",
                self.current.monarch,
                self.get_session_delta("monarch"),
                "monarch",
            ),
            (
                "⚫ Bombardier",
                "Rare",
                self.current.bombardier,
                self.get_session_delta("bombardier"),
                "bombardier",
            ),
            (
                "💀 Skull",
                "Rare",
                self.current.skull,
                self.get_session_delta("skull"),
                "skull",
            ),
            (
                "🟣 Purple",
                "Uncommon",
                self.current.purple,
                self.get_session_delta("purple"),
                "purple",
            ),
            (
                "🔵 Pond",
                "Uncommon",
                self.current.pond,
                self.get_session_delta("pond"),
                "pond",
            ),
            (
                "🔴 Ladybug",
                "Common",
                self.current.ladybug,
                self.get_session_delta("ladybug"),
                "ladybug",
            ),
            (
                "🟢 Green",
                "Common",
                self.current.green,
                self.get_session_delta("green"),
                "green",
            ),
        ]
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Streaks {
    ubc: u32,
    #[allow(dead_code)]
    last_claim: u64,
    lousy_beetle: u32,
    #[serde(default)]
    pity_counter: u32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LevelInfo {
    #[allow(dead_code)]
    xp_in_current_level: i32,
    #[allow(dead_code)]
    xp_for_next_level: u32,
    xp_needed_for_next: u32,
    progress_percent: f64,
}

// ===== BEETLE HUNT ENDPOINT =====

#[derive(Debug, Serialize)]
struct BeetleHuntRequest {}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum BeetleHuntApiResponse {
    Success {
        success: bool,
        result: BeetleHuntResult,
        user: User,
    },
    FailedHunt {
        success: bool,
        result: FailedHuntResult,
        user: User,
    },
    Error {
        success: bool,
        error: String,
        user: User,
    },
}

#[derive(Debug, Deserialize, Clone)]
pub struct FailedHuntResult {
    pub success: bool, // This will be false
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BeetleHuntResult {
    #[allow(dead_code)]
    success: bool,
    #[allow(dead_code)]
    beetle: String,
    #[serde(rename = "beetle_name")]
    beetle_name: String,
    xp: u32,
    #[allow(dead_code)]
    catch_video_info: VideoInfo,
    beetle_card: BeetleCard,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VideoInfo {
    #[allow(dead_code)]
    src: String,
    #[allow(dead_code)]
    poster: String,
    #[serde(rename = "loop")]
    #[allow(dead_code)]
    loop_video: bool,
}

// ===== POKE ENDPOINT =====

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PokeRequest {
    poke_username: String,
}

#[derive(Debug, Deserialize)]
struct PokeResponse {
    success: bool,
}

// ===== POKE ENDPOINT =====

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FriendsRequest {
    friend_username: String,
}

#[derive(Debug, Deserialize)]
struct FriendsResponse {
    success: bool,
}

// ===== FRIENDS LIST STRUCTS =====

#[derive(Debug, Deserialize)]
struct FriendsListResponse {
    #[allow(dead_code)]
    page: u32,
    #[allow(dead_code)]
    limit: u32,
    friends: Vec<Friend>,
    total: u32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Friend {
    display_username: String,
    #[allow(dead_code)]
    display_name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pfp: Option<Pfp>,
    #[serde(default)]
    #[allow(dead_code)]
    pfp_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Pfp {
    project: String,
    #[serde(default)]
    id: Option<String>, // Make optional - some profiles might not have this
}

// ===== DATABASE FUNCTIONS =====

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserRecord {
    username: String,
    #[serde(default)]
    last_poke: Option<i64>, // Unix timestamp
    #[serde(default)]
    friend_request_sent: bool,
    #[serde(default)]
    friend_request_date: Option<i64>,
    #[serde(default)]
    notes: Option<String>,
}

struct FriendsDatabase {
    file_path: String,
}

impl FriendsDatabase {
    fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }

    // Load all records from JSON file
    fn load_records(&self) -> Result<HashMap<String, UserRecord>> {
        if !std::path::Path::new(&self.file_path).exists() {
            return Ok(HashMap::new());
        }

        let content = std::fs::read_to_string(&self.file_path)?;
        if content.trim().is_empty() {
            return Ok(HashMap::new());
        }

        let records: HashMap<String, UserRecord> = serde_json::from_str(&content)?;
        Ok(records)
    }

    fn count(&self) -> Result<usize> {
        Ok(self.load_records()?.len())
    }

    // Save all records to JSON file
    fn save_records(&self, records: &HashMap<String, UserRecord>) -> Result<()> {
        let json = serde_json::to_string_pretty(records)?;
        std::fs::write(&self.file_path, json)?;
        Ok(())
    }

    // Add new username
    #[allow(dead_code)]
    fn add_username(&self, username: &str) -> Result<bool> {
        let mut records = self.load_records()?;

        if records.contains_key(username) {
            return Ok(false); // Already exists
        }

        records.insert(
            username.to_string(),
            UserRecord {
                username: username.to_string(),
                last_poke: None,
                friend_request_sent: false,
                friend_request_date: None,
                notes: None,
            },
        );

        self.save_records(&records)?;
        Ok(true)
    }

    // Add multiple usernames
    #[allow(dead_code)]
    fn add_usernames(&self, usernames: &[String]) -> Result<usize> {
        let mut records = self.load_records()?;
        let mut added = 0;

        for username in usernames {
            if !records.contains_key(username) {
                records.insert(
                    username.to_string(),
                    UserRecord {
                        username: username.to_string(),
                        last_poke: None,
                        friend_request_sent: false,
                        friend_request_date: None,
                        notes: None,
                    },
                );
                added += 1;
            }
        }

        self.save_records(&records)?;
        Ok(added)
    }

    // Update last poke timestamp
    fn update_poke(&self, username: &str) -> Result<()> {
        let mut records = self.load_records()?;

        if let Some(record) = records.get_mut(username) {
            record.last_poke = Some(chrono::Utc::now().timestamp());
            self.save_records(&records)?;
        }

        Ok(())
    }

    fn update_poke_with_cooldown(&self, username: &str, cooldown_seconds: i64) -> Result<()> {
        let mut records = self.load_records()?;

        if let Some(record) = records.get_mut(username) {
            let now = chrono::Utc::now().timestamp();

            // Calculate when the poke actually happened
            // cooldown_seconds is time remaining until we can poke again (out of 24h)
            let poke_time = now - (86400 - cooldown_seconds); // 86400 = 24h in seconds

            record.last_poke = Some(poke_time);
            self.save_records(&records)?;

            println!(
                "   ℹ️  Set last_poke to {} seconds ago",
                86400 - cooldown_seconds
            );
        }

        Ok(())
    }

    // Mark friend request as sent
    fn mark_friend_request_sent(&self, username: &str) -> Result<()> {
        let mut records = self.load_records()?;

        if let Some(record) = records.get_mut(username) {
            record.friend_request_sent = true;
            record.friend_request_date = Some(chrono::Utc::now().timestamp());
            self.save_records(&records)?;
        }

        Ok(())
    }

    // Get users ready to poke (>24h since last poke)
    fn get_pokeable_users(&self) -> Result<Vec<String>> {
        let records = self.load_records()?;
        let now = chrono::Utc::now().timestamp();
        let day_in_seconds = 24 * 60 * 60;

        let pokeable: Vec<String> = records
            .values()
            .filter(|r| {
                match r.last_poke {
                    None => true,                               // Never poked
                    Some(last) => now - last >= day_in_seconds, // >24h ago
                }
            })
            .map(|r| r.username.clone())
            .collect();

        Ok(pokeable)
    }

    // Get users without friend request
    fn get_users_for_friend_request(&self) -> Result<Vec<String>> {
        let records = self.load_records()?;

        let users: Vec<String> = records
            .values()
            .filter(|r| !r.friend_request_sent)
            .map(|r| r.username.clone())
            .collect();

        Ok(users)
    }

    // Get stats
    fn get_stats(&self) -> Result<DatabaseStats> {
        let records = self.load_records()?;
        let pokeable = self.get_pokeable_users()?.len();
        let need_friend_request = self.get_users_for_friend_request()?.len();

        let mut poked_count = 0;
        for record in records.values() {
            if record.last_poke.is_some() {
                poked_count += 1;
            }
        }

        Ok(DatabaseStats {
            total_users: records.len(),
            poked_at_least_once: poked_count,
            pokeable_now: pokeable,
            friend_requests_sent: records.values().filter(|r| r.friend_request_sent).count(),
            need_friend_request,
        })
    }

    // Get specific user record
    fn get_user(&self, username: &str) -> Result<Option<UserRecord>> {
        let records = self.load_records()?;
        Ok(records.get(username).cloned())
    }

    #[allow(dead_code)]
    fn remove_users_not_in_list(&self, usernames_to_keep: &[String]) -> Result<usize> {
        let mut records = self.load_records()?;
        let initial_count = records.len();

        // Convert to HashSet for O(1) lookup
        let keep_set: std::collections::HashSet<_> = usernames_to_keep.iter().collect();

        // Keep only users that are in the list
        records.retain(|username, _| keep_set.contains(username));

        let removed = initial_count - records.len();

        self.save_records(&records)?;
        Ok(removed)
    }

    // Sync database with a fresh friend list (add new, remove unfriended)
    fn sync_with_friends(&self, current_friends: &[String]) -> Result<SyncStats> {
        let mut records = self.load_records()?;
        let initial_count = records.len();

        // Convert current friends to HashSet for efficient lookup
        let friends_set: std::collections::HashSet<_> = current_friends.iter().collect();

        // Count removals and remove unfriended users
        let mut removed = 0;
        records.retain(|username, _| {
            if friends_set.contains(username) {
                true
            } else {
                removed += 1;
                false
            }
        });

        // Add new friends
        let mut added = 0;
        for username in current_friends {
            if !records.contains_key(username) {
                records.insert(
                    username.to_string(),
                    UserRecord {
                        username: username.to_string(),
                        last_poke: None,
                        friend_request_sent: false,
                        friend_request_date: None,
                        notes: None,
                    },
                );
                added += 1;
            }
        }

        self.save_records(&records)?;

        Ok(SyncStats {
            initial_count,
            final_count: records.len(),
            added,
            removed,
        })
    }

    // Get the next user that will become pokeable
    fn get_next_pokeable_user(&self) -> Result<Option<NextPokeableUser>> {
        let records = self.load_records()?;
        let now = chrono::Utc::now().timestamp();
        let day_in_seconds = 24 * 60 * 60;

        let mut next_user: Option<NextPokeableUser> = None;
        let mut shortest_wait = i64::MAX;

        for record in records.values() {
            if let Some(last) = record.last_poke {
                let time_since_poke = now - last;
                
                if time_since_poke < day_in_seconds {
                    let time_until_pokeable = day_in_seconds - time_since_poke;
                    if time_until_pokeable < shortest_wait {
                        shortest_wait = time_until_pokeable;
                        next_user = Some(NextPokeableUser {
                            username: record.username.clone(),
                            time_until_pokeable: time_until_pokeable as u64,
                        });
                    }
                } else {
                    if 0 < shortest_wait {
                        shortest_wait = 0;
                        next_user = Some(NextPokeableUser {
                            username: record.username.clone(),
                            time_until_pokeable: 0,
                        });
                    }
                }
            }
        }

        Ok(next_user)
    }
}

#[derive(Debug, Clone)]
pub struct NextPokeableUser {
    pub username: String,
    pub time_until_pokeable: u64, // seconds
}

#[derive(Debug)]
struct SyncStats {
    initial_count: usize,
    final_count: usize,
    added: usize,
    removed: usize,
}

#[derive(Debug)]
struct DatabaseStats {
    total_users: usize,
    poked_at_least_once: usize,
    pokeable_now: usize,
    friend_requests_sent: usize,
    need_friend_request: usize,
}

// ===== HELPER FUNCTIONS =====
fn generate_socket_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    // Generate random string similar to: mbzgdfe7
    let random: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    format!("{:x}{}", now, random).chars().take(11).collect()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfilePfp {
    pub project: String,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateThemeRequest {
    pub theme: String,
    pub color: u32,
    pub cover: String,
    pub pfp: ProfilePfp,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateThemeResponse {
    pub success: bool,
}
// ===== API CLIENT =====

struct BeetleApiClient {
    client: Client,
    auth_token: String,
    profile_sid: String,
    beetle_sid: String,
}

impl BeetleApiClient {
    fn new(auth_token: &str, profile_sid: &str, beetle_sid: &str) -> Result<Self> {
        let client = Client::builder()
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36")
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            auth_token: auth_token.to_string(),
            profile_sid: profile_sid.to_string(),
            beetle_sid: beetle_sid.to_string(),
        })
    }

    fn build_headers_remilia(&self) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();

        // Build cookie string with both session IDs
        let cookie_str = format!(
            "profile.sid={}; beetle.sid={}",
            self.profile_sid, self.beetle_sid
        );

        if let Ok(cookie) = header::HeaderValue::from_str(&cookie_str) {
            headers.insert(header::COOKIE, cookie);
        }

        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(header::ACCEPT, header::HeaderValue::from_static("*/*"));
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

    fn build_headers_beetle(&self) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();

        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&self.auth_token).unwrap(),
        );
        headers.insert(
            header::ORIGIN,
            header::HeaderValue::from_static("https://remilia.com"),
        );
        headers.insert(
            header::REFERER,
            header::HeaderValue::from_static("https://remilia.com/"),
        );
        headers.insert(header::ACCEPT, header::HeaderValue::from_static("*/*"));
        headers.insert(
            "sec-fetch-site",
            header::HeaderValue::from_static("cross-site"),
        );

        headers
    }

    async fn socket_init(&self) -> Result<String> {
        let timestamp = generate_socket_timestamp();
        let url = format!(
            "https://www.remilia.com/identity/socket.io/?EIO=4&transport=polling&t={}",
            timestamp
        );

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers_remilia())
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Socket init failed with status {}: {}", status, body);
        }

        // Response format: 0{"sid":"...","upgrades":...}
        // Strip the leading "0" and parse JSON
        let json_part = body.trim_start_matches('0');
        let init_response: SocketInitResponse =
            serde_json::from_str(json_part).context("Failed to parse socket init response")?;

        println!("✅ Socket.IO session initialized: {}", init_response.sid);
        Ok(init_response.sid)
    }

    async fn get_feed_activity(&self) -> Result<Vec<FeedActivity>> {
        println!("🔌 Initializing Socket.IO connection...");
        let sid = self.socket_init().await?;
        println!("✅ Socket.IO session initialized: {}", sid);

        // Send a POST request to subscribe to the feed
        let timestamp = generate_socket_timestamp();
        let subscribe_url = format!(
            "https://www.remilia.com/identity/socket.io/?EIO=4&transport=polling&t={}&sid={}",
            timestamp, sid
        );

        // Post the subscription message: 40/feed,
        let subscribe_payload = r#"40/feed,"#;

        println!("📡 Subscribing to feed namespace...");
        let sub_response = self
            .client
            .post(&subscribe_url)
            .headers(self.build_headers_remilia())
            .header(header::CONTENT_TYPE, "text/plain;charset=UTF-8")
            .body(subscribe_payload.to_string())
            .send()
            .await?;

        if !sub_response.status().is_success() {
            anyhow::bail!("Failed to subscribe to feed: {}", sub_response.status());
        }

        println!("✅ Subscribed to feed namespace");

        // Wait a moment for the server to process
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Now poll for the actual feed data
        let timestamp = generate_socket_timestamp();
        let poll_url = format!(
            "https://www.remilia.com/identity/socket.io/?EIO=4&transport=polling&t={}&sid={}",
            timestamp, sid
        );

        println!("📡 Polling for feed data...");
        let response = self
            .client
            .get(&poll_url)
            .headers(self.build_headers_remilia())
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("Failed to get feed: {} - {}", status, body);
        }

        println!("📦 Received response ({} bytes)", body.len());

        self.parse_feed_response(&body)
    }

    fn parse_feed_response(&self, body: &str) -> Result<Vec<FeedActivity>> {
        // Socket.IO protocol: messages are prefixed with codes like "42/feed,"
        // Find the JSON array that starts with ["recent",

        if let Some(start) = body.find(r#"["recent","#) {
            let json_str = &body[start..];

            // Find the end of this JSON array (matching closing bracket)
            let mut depth = 0;
            let mut end_pos = 0;

            for (i, ch) in json_str.chars().enumerate() {
                match ch {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if end_pos == 0 {
                return Err(anyhow::anyhow!("Could not find end of JSON array"));
            }

            let json_str = &json_str[..end_pos];
            println!("🔍 Parsing JSON array ({} bytes)", json_str.len());

            // Parse as ["recent", [...activities...]]
            let parsed: serde_json::Value =
                serde_json::from_str(json_str).context("Failed to parse JSON")?;

            if let Some(array) = parsed.as_array() {
                if array.len() >= 2 {
                    if let Some(activities_json) = array.get(1) {
                        if let Some(activities_array) = activities_json.as_array() {
                            let mut activities = Vec::new();

                            for item in activities_array {
                                match serde_json::from_value::<FeedActivity>(item.clone()) {
                                    Ok(activity) => activities.push(activity),
                                    Err(e) => {
                                        println!("⚠️  Skipping unparseable activity: {}", e);
                                    }
                                }
                            }

                            return Ok(activities);
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!("Could not find feed data in response"))
    }

    async fn get_auth_status(&mut self) -> Result<AuthStatusResponse> {
        println!("📡 Fetching auth status...");

        let response = self
            .client
            .get("https://www.remilia.com/auth/status") // Note: www.remilia.com
            .headers(self.build_headers_remilia())
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
                            self.profile_sid = cookie_value.to_string();
                            println!("✅ Updated profile.sid");
                        }
                    }
                }
                if cookie_str.contains("beetle.sid=") {
                    if let Some(value) = cookie_str.split(';').next() {
                        if let Some(cookie_value) = value.strip_prefix("beetle.sid=") {
                            self.beetle_sid = cookie_value.to_string();
                            println!("✅ Updated beetle.sid");
                        }
                    }
                }
            }
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        println!("📦 Raw response: {}", text);

        let auth_status: AuthStatusResponse =
            serde_json::from_str(&text).context("Failed to parse auth status JSON")?;

        Ok(auth_status)
    }

    async fn get_profile(&self, username: &str) -> Result<ProfileResponse> {
        println!("📡 Fetching profile for {}...", username);

        let url = format!("https://www.remilia.com/api/profile/~{}", username);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers_remilia())
            .send()
            .await
            .context("Failed to send profile request")?;

        let status = response.status();
        println!("✅ Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        println!("📦 Raw response : {}", text);

        let profile: ProfileResponse =
            serde_json::from_str(&text).context("Failed to parse profile JSON")?;

        println!("✅ Fetched profile for {}", profile.user.display_name);
        Ok(profile)
    }

    async fn get_cooldowns(&self) -> Result<CooldownsResponse> {
        println!("📡 Fetching cooldowns...");

        let response = self
            .client
            .get("https://www.remilia.com/beetle/api/cooldowns")
            .headers(self.build_headers_beetle())
            .send()
            .await
            .context("Failed to send cooldowns request")?;


        let status = response.status();
        println!("✅ Response status: {}", status);



        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        println!("📦 Raw cooldowns response: {}", text);

        let cooldowns: CooldownsResponse =
            serde_json::from_str(&text).context("Failed to parse cooldowns JSON")?;

        Ok(cooldowns)
    }

    pub async fn get_beetle_user(&self) -> Result<User> {
        let response = self
            .client
            .get("https://www.remilia.com/beetle/api/user")
            .headers(self.build_headers_beetle())
            .send()
            .await
            .context("Failed to send beetle user request")?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        let result: User =
            serde_json::from_str(&text).context("Failed to parse beetle user JSON")?;

        Ok(result)
    }

    async fn beetle_hunt(&self) -> Result<BeetleHuntApiResponse> {
        println!("🎯 Starting beetle hunt...");

        // Check cooldowns first
        let cooldowns = self.get_cooldowns().await?;

        if cooldowns.hunt_info.hunts_used >= 3 {
            println!(
                "⚠️  Hunt limit reached ({}/3)",
                cooldowns.hunt_info.hunts_used
            );
            println!(
                "⏰ Resets at: {}",
                if cooldowns.hunt_info.reset_time > 0 {
                    format!("timestamp {}", cooldowns.hunt_info.reset_time)
                } else {
                    "midnight UTC".to_string()
                }
            );
            anyhow::bail!("Hunt limit reached. Try again later.");
        }

        println!(
            "📊 Hunts remaining: {}/3",
            3 - cooldowns.hunt_info.hunts_used
        );

        let response = self
            .client
            .post("https://www.remilia.com/beetle/api/action/beetleHunt")
            .headers(self.build_headers_beetle())
            .json(&BeetleHuntRequest {})
            .send()
            .await
            .context("Failed to send beetle hunt request")?;

        let status = response.status();
        println!("✅ Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        // println!("📦 beetle_hunt - Raw response: {}", text);

        let result: BeetleHuntApiResponse =
            serde_json::from_str(&text).context("Failed to parse beetle hunt JSON")?;

        Ok(result)
    }

    async fn beetle_catch(&self) -> Result<CatchBeetleApiResponse> {
        println!("🎯 Catching beetle (cooldown-based)...");

        let response = self
            .client
            .post("https://www.remilia.com/beetle/api/action/catchBeetle")
            .headers(self.build_headers_beetle())
            .json(&CatchBeetleRequest {})
            .send()
            .await
            .context("Failed to send catch beetle request")?;

        let status = response.status();
        println!("✅ Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        // println!("📦 Raw response: {}", text);

        let result: CatchBeetleApiResponse =
            serde_json::from_str(&text).context("Failed to parse catch beetle JSON")?;

        Ok(result)
    }

    async fn claim_ubc(&self) -> Result<ClaimUBCApiResponse> {
        println!("🧀 Claiming Universal Basic Cheese...");

        let response = self
            .client
            .post("https://www.remilia.com/beetle/api/action/claimUBC")
            .headers(self.build_headers_beetle())
            .json(&ClaimUBCRequest {})
            .send()
            .await
            .context("Failed to send claim UBC request")?;

        let status = response.status();
        println!("✅ Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        // println!("📦 Raw response: {}", text);

        let result: ClaimUBCApiResponse =
            serde_json::from_str(&text).context("Failed to parse claim UBC JSON")?;

        Ok(result)
    }

    async fn poke_user(&self, username: &str) -> Result<PokeResponse> {
        println!("👉 Poking user: {}", username);

        let response = self
            .client
            .post("https://www.remilia.com/api/poke")
            .headers(self.build_headers_remilia())
            .json(&PokeRequest {
                poke_username: username.to_string(),
            })
            .send()
            .await
            .context("Failed to send poke request")?;

        let status = response.status();
        println!("✅ Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();

            // If we get a 500 error, check the profile to see if we can poke
            if status == 500 {
                println!("⚠️  Got 500 error, checking profile for poke availability...");

                match self.get_profile(username).await {
                    Ok(profile) => {
                        if let Some(extra_context) = profile.extra_context {
                            if !extra_context.can_poke {
                                let cooldown = extra_context.poke_cooldown_seconds;
                                let minutes = cooldown / 60;
                                let hours = minutes / 60;
                                let remaining_minutes = minutes % 60;

                                anyhow::bail!(
                                    "❌ Cannot poke {}: Still on cooldown for {} hours and {} minutes ({} seconds)",
                                    username,
                                    hours,
                                    remaining_minutes,
                                    cooldown
                                );
                            } else {
                                anyhow::bail!(
                                    "❌ Profile says canPoke is true, but request failed with 500: {}",
                                    error_text
                                );
                            }
                        } else {
                            anyhow::bail!(
                                "❌ Request failed with 500 and no extra context in profile: {}",
                                error_text
                            );
                        }
                    }
                    Err(profile_err) => {
                        anyhow::bail!(
                            "❌ Request failed with 500 and couldn't fetch profile: {}. Original error: {}",
                            profile_err,
                            error_text
                        );
                    }
                }
            }

            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        // println!("📦 Raw response: {}", text);

        let result: PokeResponse =
            serde_json::from_str(&text).context("Failed to parse poke JSON")?;

        Ok(result)
    }
    async fn send_friend_request(&self, username: &str) -> Result<FriendsResponse> {
        println!("👉 Sending friend request to: {}", username);

        let response = self
            .client
            .post("https://www.remilia.com/api/friends/request")
            .headers(self.build_headers_remilia())
            .json(&FriendsRequest {
                friend_username: username.to_string(),
            })
            .send()
            .await
            .context("Failed to send friend request")?;

        let status = response.status();
        println!("✅ Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Request failed with status {}: {}", status, error_text);
        }

        let text = response.text().await?;
        // println!("📦 Raw response: {}", text);

        let result: FriendsResponse =
            serde_json::from_str(&text).context("Failed to parse friends JSON")?;

        Ok(result)
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
            .header("Accept", "application/json, text/plain, */*")
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
        let first_page = self.get_friends_page(username, 1, 100).await?;
        let total_friends = first_page.total;
        let total_pages = (total_friends as f32 / 100.0).ceil() as u32;

        println!("👥 Target user has {} friends", total_friends);
        println!("📄 Will scrape {} pages\n", total_pages);

        let mut all_usernames = Vec::new();
        let mut errors = 0;
        let mut filtered_self = 0;

        // Scrape all pages
        for page in 1..=total_pages {
            print!("📄 Page {}/{} ... ", page, total_pages);
            std::io::Write::flush(&mut std::io::stdout())?;

            match self.get_friends_page(username, page, 100).await {
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

    /// Auto-poke users from feed activities
    async fn auto_poke_from_feed(&self, max_pokes: usize) -> Result<()> {
        println!("🎯 Starting auto-poke from feed...\n");

        // Get feed activities
        let activities = self.get_feed_activity().await?;

        // Extract unique usernames (avoid duplicates)
        let mut usernames_to_poke: Vec<String> = Vec::new();
        let mut seen_users: std::collections::HashSet<String> = std::collections::HashSet::new();

        for activity in activities {
            match activity {
                FeedActivity::Poke { from, to, .. } => {
                    // Add both users if not seen
                    if seen_users.insert(from.username.clone()) {
                        usernames_to_poke.push(from.username);
                    }
                    if seen_users.insert(to.username.clone()) {
                        usernames_to_poke.push(to.username);
                    }
                }
                FeedActivity::Friendship { users, .. } => {
                    for user in users {
                        if seen_users.insert(user.username.clone()) {
                            usernames_to_poke.push(user.username);
                        }
                    }
                }
            }

            // Stop if we have enough
            if usernames_to_poke.len() >= max_pokes {
                break;
            }
        }

        println!("📋 Found {} unique users to poke", usernames_to_poke.len());
        let to_poke = usernames_to_poke
            .into_iter()
            .take(max_pokes)
            .collect::<Vec<_>>();

        println!("🎯 Will poke {} users:\n", to_poke.len());
        for (i, username) in to_poke.iter().enumerate() {
            println!("  {}. ~{}", i + 1, username);
        }

        println!("\n⏳ Starting pokes with 2s delay between each...\n");

        let mut success_count = 0;
        let mut fail_count = 0;

        for (i, username) in to_poke.iter().enumerate() {
            println!("👉 [{}/{}] Poking ~{}...", i + 1, to_poke.len(), username);

            match self.poke_user(username).await {
                Ok(response) => {
                    if response.success {
                        println!("   ✅ Success!");
                        success_count += 1;
                    } else {
                        println!("   ❌ Poke failed (API returned false)");
                        fail_count += 1;
                    }
                }
                Err(e) => {
                    println!("   ❌ Failed: {}", e);
                    fail_count += 1;
                }
            }

            // Delay between pokes to avoid rate limiting
            if i < to_poke.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }

        println!("\n{}", "=".repeat(80));
        println!("📊 Auto-Poke Summary:");
        println!("   ✅ Successful: {}", success_count);
        println!("   ❌ Failed: {}", fail_count);
        println!("   📊 Total: {}", to_poke.len());
        println!("{}", "=".repeat(80));

        Ok(())
    }

    /// Auto-poke with smart filtering
    async fn auto_poke_smart(
        &self,
        max_pokes: usize,
        exclude_usernames: Vec<String>,
    ) -> Result<()> {
        println!("🧠 Starting SMART auto-poke from feed...\n");

        let activities = self.get_feed_activity().await?;

        let mut user_activity_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        // Count activity per user (more activity = more likely to respond)
        for activity in &activities {
            match activity {
                FeedActivity::Poke { from, to, .. } => {
                    *user_activity_count
                        .entry(from.username.clone())
                        .or_insert(0) += 1;
                    *user_activity_count.entry(to.username.clone()).or_insert(0) += 1;
                }
                FeedActivity::Friendship { users, .. } => {
                    for user in users {
                        *user_activity_count
                            .entry(user.username.clone())
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        // Sort by activity count (most active first)
        let mut sorted_users: Vec<_> = user_activity_count.into_iter().collect();
        sorted_users.sort_by(|a, b| b.1.cmp(&a.1));

        // Filter out excluded users
        let filtered_users: Vec<String> = sorted_users
            .into_iter()
            .map(|(username, _)| username)
            .filter(|u| !exclude_usernames.contains(u))
            .take(max_pokes)
            .collect();

        println!(
            "🎯 Selected {} most active users (excluding {} filtered)\n",
            filtered_users.len(),
            exclude_usernames.len()
        );

        for (i, username) in filtered_users.iter().enumerate() {
            println!("  {}. ~{}", i + 1, username);
        }

        println!("\n⏳ Starting pokes with 2s delay between each...\n");

        let mut success_count = 0;
        let mut fail_count = 0;

        for (i, username) in filtered_users.iter().enumerate() {
            println!(
                "👉 [{}/{}] Poking ~{}...",
                i + 1,
                filtered_users.len(),
                username
            );

            match self.poke_user(username).await {
                Ok(response) => {
                    if response.success {
                        println!("   ✅ Success!");
                        success_count += 1;
                    } else {
                        println!("   ❌ Poke failed (API returned false)");
                        fail_count += 1;
                    }
                }
                Err(e) => {
                    println!("   ❌ Failed: {}", e);
                    fail_count += 1;
                }
            }

            if i < filtered_users.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }

        println!("\n{}", "=".repeat(80));
        println!("📊 Smart Auto-Poke Summary:");
        println!("   ✅ Successful: {}", success_count);
        println!("   ❌ Failed: {}", fail_count);
        println!("   📊 Total: {}", filtered_users.len());
        println!("{}", "=".repeat(80));

        Ok(())
    }

    /// Auto-friend users from feed activities
    async fn auto_friends_from_feed(&self, max_friends: usize) -> Result<()> {
        println!("🎯 Starting auto-friend from feed...\n");

        // Get feed activities
        let activities = self.get_feed_activity().await?;

        // Extract unique usernames (avoid duplicates)
        let mut usernames_to_add: Vec<String> = Vec::new();
        let mut seen_users: std::collections::HashSet<String> = std::collections::HashSet::new();

        for activity in activities {
            match activity {
                FeedActivity::Poke { from, to, .. } => {
                    // Add both users if not seen
                    if seen_users.insert(from.username.clone()) {
                        usernames_to_add.push(from.username);
                    }
                    if seen_users.insert(to.username.clone()) {
                        usernames_to_add.push(to.username);
                    }
                }
                FeedActivity::Friendship { users, .. } => {
                    for user in users {
                        if seen_users.insert(user.username.clone()) {
                            usernames_to_add.push(user.username);
                        }
                    }
                }
            }

            // Stop if we have enough
            if usernames_to_add.len() >= max_friends {
                break;
            }
        }

        println!("📋 Found {} unique users to add", usernames_to_add.len());
        let to_add = usernames_to_add
            .into_iter()
            .take(max_friends)
            .collect::<Vec<_>>();

        println!("🎯 Will add {} users:\n", to_add.len());
        for (i, username) in to_add.iter().enumerate() {
            println!("  {}. ~{}", i + 1, username);
        }

        println!("\n⏳ Starting friend requests with 2s delay between each...\n");

        let mut success_count = 0;
        let mut fail_count = 0;

        for (i, username) in to_add.iter().enumerate() {
            println!(
                "👉 [{}/{}] Sending friend request to ~{}...",
                i + 1,
                to_add.len(),
                username
            );

            match self.send_friend_request(username).await {
                Ok(response) => {
                    if response.success {
                        println!("   ✅ Success!");
                        success_count += 1;
                    } else {
                        println!("   ❌ Poke failed (API returned false)");
                        fail_count += 1;
                    }
                }
                Err(e) => {
                    println!("   ❌ Failed: {}", e);
                    fail_count += 1;
                }
            }

            // Delay between pokes to avoid rate limiting
            if i < to_add.len() - 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }

        println!("\n{}", "=".repeat(80));
        println!("📊 Auto-Poke Summary:");
        println!("   ✅ Successful: {}", success_count);
        println!("   ❌ Failed: {}", fail_count);
        println!("   📊 Total: {}", to_add.len());
        println!("{}", "=".repeat(80));

        Ok(())
    }

    async fn auto_poke_from_db(&self, db_file: &str, max_count: usize) -> Result<()> {
        let db = FriendsDatabase::new(db_file);
        let pokeable = db.get_pokeable_users()?;

        if pokeable.is_empty() {
            println!("⏰ No users ready to poke yet (all recently poked)");
            return Ok(());
        }

        let to_poke: Vec<String> = pokeable.into_iter().take(max_count).collect();

        println!("\n🎯 ===== AUTO-POKE FROM DATABASE =====");
        println!("📊 Found {} users ready to poke", to_poke.len());
        println!("🎲 Will poke {} users\n", to_poke.len());

        let mut success = 0;
        let mut failed = 0;

        for (i, username) in to_poke.iter().enumerate() {
            println!("[{}/{}] 👉 Poking @{}...", i + 1, to_poke.len(), username);

            match self.poke_user(username).await {
                Ok(response) => {
                    if response.success {
                        println!("   ✅ Poke successful!");
                        db.update_poke(username)?;
                        success += 1;
                    } else {
                        println!("   ⚠️  Poke returned false");
                        failed += 1;
                    }
                }
                Err(e) => {
                    println!("   ❌ Error: {}", e);
                    failed += 1;
                }
            }

            // Rate limiting
            if i < to_poke.len() - 1 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        println!("\n✨ ===== POKE SUMMARY =====");
        println!("✅ Successful: {}", success);
        println!("❌ Failed: {}", failed);
        println!("⏰ Next batch ready in ~24 hours");

        Ok(())
    }

    async fn auto_friend_request_from_db(&self, db_file: &str, max_count: usize) -> Result<()> {
        let db = FriendsDatabase::new(db_file);
        let users = db.get_users_for_friend_request()?;

        if users.is_empty() {
            println!("✅ All users have received friend requests!");
            return Ok(());
        }

        let to_friend: Vec<String> = users.into_iter().take(max_count).collect();

        println!("\n🤝 ===== AUTO-FRIEND FROM DATABASE =====");
        println!("📊 Found {} users without friend request", to_friend.len());
        println!("🎲 Will send {} friend requests\n", to_friend.len());

        let mut success = 0;
        let mut failed = 0;

        for (i, username) in to_friend.iter().enumerate() {
            println!("[{}/{}] 🤝 Adding @{}...", i + 1, to_friend.len(), username);

            match self.send_friend_request(username).await {
                Ok(response) => {
                    if response.success {
                        println!("   ✅ Friend request sent!");
                        db.mark_friend_request_sent(username)?;
                        success += 1;
                    } else {
                        println!("   ⚠️  Request returned false");
                        // Still mark as sent to avoid retrying
                        db.mark_friend_request_sent(username)?;
                        failed += 1;
                    }
                }
                Err(e) => {
                    println!("   ❌ Error: {}", e);
                    failed += 1;
                }
            }

            // Rate limiting
            if i < to_friend.len() - 1 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        println!("\n✨ ===== FRIEND REQUEST SUMMARY =====");
        println!("✅ Successful: {}", success);
        println!("❌ Failed: {}", failed);
        println!(
            "📊 Remaining users: {}",
            db.get_users_for_friend_request()?.len()
        );

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
                            if let Some(cooldown_secs) = Self::extract_cooldown_seconds(&err_msg) {
                                db.update_poke_with_cooldown(username, cooldown_secs)?;
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
                tokio::time::sleep(Duration::from_secs_f64(d
                    elay_secs)).await;
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

    // Helper function to extract cooldown seconds from error message
    fn extract_cooldown_seconds(err_msg: &str) -> Option<i64> {
        // Error format: "Still on cooldown for X hours and Y minutes (Z seconds)"
        if let Some(start) = err_msg.find('(') {
            if let Some(end) = err_msg[start..].find(" seconds)") {
                let seconds_str = &err_msg[start + 1..start + end];
                return seconds_str.parse::<i64>().ok();
            }
        }
        None
    }


    pub async fn update_theme_to_dark(&mut self) -> Result<UpdateThemeResponse> {
        // First, get current profile to preserve settings
        let _current_profile = self.get_auth_status().await?;

        let request = UpdateThemeRequest {
            theme: "dark".to_string(),
            color: 171,                  // Keep existing color
            cover: "Monkey".to_string(), // Keep existing cover
            pfp: ProfilePfp {
                project: "VeryInternetPerson".to_string(),
                id: "2608".to_string(),
            },
            display_name: "maO Z".to_string(),
            username: "~mao".to_string(),
        };

        let response = self
            .client
            .post("https://www.remilia.com/api/profile/update/theme")
            .headers(self.build_headers_remilia())
            .json(&request)
            .send()
            .await?;

        // let text = response.text().await?;
        // println!("📦 Raw response: {}", text);

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to update theme: {}",
                response.status()
            ));
        }

        let theme_response: UpdateThemeResponse = response.json().await?;
        Ok(theme_response)
    }
}

pub fn display_beetle_user(user: &User) {
    println!("\n╔═══════════════════════════════════════════╗");
    println!("║         🪲 BEETLE USER INFO               ║");
    println!("╠═══════════════════════════════════════════╣");
    println!("║ Level:                    {:>15} ║", user.level);
    println!("║ XP:                       {:>15} ║", user.xp);
    println!("║ Cheese:                   {:>15} 🧀║", user.cheese);
    println!("║ Total Beetles:            {:>15} ║", user.total_beetles());
    println!("╠═══════════════════════════════════════════╣");
    println!("║ 📦 INVENTORY                              ║");
    println!("╠═══════════════════════════════════════════╣");
    println!("║ 🟢 Green:                 {:>15} ║", user.inventory.green);
    println!(
        "║ 🔴 Ladybug:               {:>15} ║",
        user.inventory.ladybug
    );
    println!(
        "║ 🟠 Monarch:               {:>15} ║",
        user.inventory.monarch
    );
    println!("║ 🔵 Pond:                  {:>15} ║", user.inventory.pond);
    println!(
        "║ ⚫ Bombardier:            {:>15} ║",
        user.inventory.bombardier
    );
    println!(
        "║ 🟣 Purple:                {:>15} ║",
        user.inventory.purple
    );
    println!("║ 💀 Skull:                {:>15} ║", user.inventory.skull);
    println!("╠═══════════════════════════════════════════╣");
    println!("║ 🔥 STREAKS                                ║");
    println!("╠═══════════════════════════════════════════╣");
    println!("║ UBC Streak:               {:>15} ║", user.streaks.ubc);
    println!(
        "║ 🪳 Lousy Beetle:          {:>15} ║",
        user.streaks.lousy_beetle
    );
    println!(
        "║ 🎲 Pity Counter:          {:>15} ║",
        user.streaks.pity_counter
    );
    println!("╠═══════════════════════════════════════════╣");
    println!("║ 🎯 HUNTS                                  ║");
    println!("╠═══════════════════════════════════════════╣");
    println!(
        "║ Used Today:               {:>15} ║",
        user.beetle_hunts_used
    );
    println!(
        "║ Remaining:                {:>15} ║",
        user.hunts_remaining()
    );
    println!("╠═══════════════════════════════════════════╣");
    println!("║ ⏰ COOLDOWNS                              ║");
    println!("╠═══════════════════════════════════════════╣");

    if user.can_catch_beetle() {
        println!("║ 🪲 Catch:                 {:>15} ║", "✅ Ready!");
    } else {
        let time = user.time_until_catch_ready();
        println!(
            "║ 🪲 Catch:                 {:>15} ║",
            format_duration(time)
        );
    }

    if user.can_claim_ubc() {
        println!("║ 🧀 UBC Claim:             {:>15} ║", "✅ Ready!");
    } else {
        let time = user.time_until_ubc_ready();
        println!(
            "║ 🧀 UBC Claim:             {:>15} ║",
            format_duration(time)
        );
    }

    println!("╠═══════════════════════════════════════════╣");
    println!("║ 📊 LEVEL PROGRESS                         ║");
    println!("╠═══════════════════════════════════════════╣");
    println!(
        "║ XP for Next:              {:>15} ║",
        user.level_info.xp_needed_for_next
    );
    println!(
        "║ Progress:                 {:>14.1}% ║",
        user.level_info.progress_percent
    );
    println!("╚═══════════════════════════════════════════╝\n");
}

// ===== DISPLAY FUNCTIONS =====

fn display_auth_status(auth: &AuthStatusResponse) {
    println!("\n════════════════════════════════════");
    println!("🔐 AUTH STATUS");
    println!("════════════════════════════════════");
    println!(
        "Authenticated: {}",
        if auth.authenticated {
            "✅ Yes"
        } else {
            "❌ No"
        }
    );

    if let Some(user) = &auth.user {
        println!("\n👤 ===== USER INFO =====");
        println!("Username: ~{}", user.username);
        println!("Display Name: {}", user.display_name);
        println!("Email: {}", user.email);
        println!(
            "Onboarded: {}",
            if user.onboarded { "✅" } else { "⏳ Pending" }
        );

        println!("\n📊 ===== STATS =====");
        println!("Pokes: {}", user.pokes);
        println!("Page Views: {}", user.page_views);
        println!("Friends: {}", user.friend_count);
    } else {
        println!("\n⚠️  Not authenticated or no user data available");
        println!("💡 Try logging in or checking your session");
    }

    println!("════════════════════════════════════\n");
}

fn display_profile(profile: &ProfileResponse) {
    let user = &profile.user;

    println!("\n════════════════════════════════════");
    println!("👤 PROFILE: {}", user.display_name);
    println!("════════════════════════════════════");
    println!("Username: ~{}", user.username);
    println!(
        "Location: {}",
        user.location.as_deref().unwrap_or("Unknown")
    );

    if let Some(bio) = &user.bio {
        println!("Bio: {}", bio);
    }

    println!("\n🎨 Appearance:");
    println!("  Theme: {}", user.theme);
    println!("  Cover: {}", user.cover);
    println!("  Color: #{:06X}", user.color);

    println!("\n📊 Stats:");
    println!("  Pokes: 👆 {}", user.pokes);
    println!("  Beetles: 🪲 {}", user.beetles);
    println!("  Friends: 🤝 {}", user.friend_count);

    // Try to get achievements count from the user object
    if let Some(ach_count) = user.other.get("achievementsCount").and_then(|v| v.as_u64()) {
        println!("  Achievements: 🏆 {}", ach_count);
    }

    // Try to get page views (only on own profile)
    if let Some(views) = user.other.get("pageViews").and_then(|v| v.as_u64()) {
        println!("  Page Views: 👁️  {}", views);
    }

    // Try to display social credit
    if let Some(social_credit) = user.other.get("socialCredit") {
        if let Some(score) = social_credit.get("score").and_then(|v| v.as_u64()) {
            println!("\n💯 Social Credit: {}", score);

            if let Some(components) = social_credit.get("components") {
                if let Some(base) = components.get("base").and_then(|v| v.as_u64()) {
                    println!("  Base: {}", base);
                }
                if let Some(onboarding) = components.get("onboarding").and_then(|v| v.as_u64()) {
                    println!("  Onboarding: +{}", onboarding);
                }
                if let Some(agg_bonus) = components.get("aggregateBonus").and_then(|v| v.as_u64()) {
                    println!("  Aggregate: +{}", agg_bonus);
                }
                if let Some(friend_bonus) = components.get("friendBonus").and_then(|v| v.as_u64()) {
                    println!("  Friends: +{}", friend_bonus);
                }
                if let Some(final_score) = components.get("final").and_then(|v| v.as_u64()) {
                    println!("  Final: {}", final_score);
                }

                // Display aggregate scores
                if let Some(agg_scores) = components.get("aggregateScores") {
                    println!("\n🎯 Aggregate Scores:");
                    if let Some(mc) = agg_scores.get("miladychan").and_then(|v| v.as_u64()) {
                        println!("  Miladychan: {}", mc);
                    }
                    if let Some(tw) = agg_scores.get("twitter").and_then(|v| v.as_u64()) {
                        println!("  Twitter: {}", tw);
                    }
                    if let Some(prof) = agg_scores.get("profiles").and_then(|v| v.as_u64()) {
                        println!("  Profiles: {}", prof);
                    }
                    if let Some(bg) = agg_scores.get("beetle_game").and_then(|v| v.as_u64()) {
                        println!("  Beetle Game: {}", bg);
                    }
                    if let Some(mc) = agg_scores.get("miladycraft").and_then(|v| v.as_u64()) {
                        println!("  Miladycraft: {}", mc);
                    }
                    if let Some(eth) = agg_scores.get("ethereum").and_then(|v| v.as_u64()) {
                        println!("  Ethereum: {}", eth);
                    }
                }
            }
        }
    }

    // Try to display achievements
    if let Some(achievements) = user.other.get("allAchievements").and_then(|v| v.as_array()) {
        if !achievements.is_empty() {
            println!("\n🏆 Achievements ({}):", achievements.len());
            for ach in achievements.iter().take(5) {
                if let (Some(title), Some(desc)) = (
                    ach.get("title").and_then(|v| v.as_str()),
                    ach.get("description").and_then(|v| v.as_str()),
                ) {
                    println!("  • {} - {}", title, desc);
                }
            }
            if achievements.len() > 5 {
                println!("  ... and {} more", achievements.len() - 5);
            }
        }
    }

    // Try to display connections (only on own profile)
    if let Some(connections) = user.other.get("connections").and_then(|v| v.as_array()) {
        if !connections.is_empty() {
            println!("\n🔗 Connected Accounts:");
            for conn in connections {
                if let (Some(conn_type), Some(username)) = (
                    conn.get("type").and_then(|v| v.as_str()),
                    conn.get("username").and_then(|v| v.as_str()),
                ) {
                    let icon = match conn_type {
                        "twitter" => "🐦",
                        "discord" => "💬",
                        _ => "🔗",
                    };
                    println!("  {} {}: {}", icon, conn_type, username);
                }
            }
        }
    }

    // Display relationship info (only when viewing others' profiles)
    if let Some(extra) = &profile.extra_context {
        println!("\n🔗 Relationship:");
        println!(
            "  Friends: {}",
            if extra.are_friends {
                "Yes ✅"
            } else {
                "No ❌"
            }
        );
        println!("  Mutual Friends: {}", extra.mutual_count);
        println!(
            "  Can Poke: {}",
            if extra.can_poke { "Yes ✅" } else { "No ❌" }
        );
        if !extra.can_poke {
            let cooldown = extra.poke_cooldown_seconds;
            let hours = cooldown / 3600;
            let minutes = (cooldown % 3600) / 60;
            let seconds = cooldown % 60;
            println!("  Poke Cooldown: {}h {}m {}s", hours, minutes, seconds);
        }

        // Display pending requests
        if extra.pending_request_from {
            println!("  📨 Has sent you a friend request");
        }
        if extra.pending_request_to {
            println!("  📤 You sent them a friend request");
        }
    }

    // Authentication status
    if profile.is_authenticated {
        if let Some(username) = &profile.current_username {
            println!("\n🔐 Viewing as: ~{}", username);
        }
        if profile.is_own_profile {
            println!("✅ This is your profile");
        }
    }

    println!("════════════════════════════════════\n");
}

fn display_cooldowns(response: &CooldownsResponse) {
    println!("\n⏰ ===== COOLDOWNS =====");

    // Catch Beetle
    let catch_beetle_seconds = response.cooldowns.catch_beetle / 1000;
    if response.cooldowns.catch_beetle > 0 {
        println!("🐞 Catch Beetle: {}", format_duration(catch_beetle_seconds));
    } else {
        println!("🐞 Catch Beetle: ✅ READY!");
    }

    // Claim UBC
    let claim_ubc_seconds = response.cooldowns.claim_ubc / 1000;
    if response.cooldowns.claim_ubc > 0 {
        println!("🎁 Claim UBC: {}", format_duration(claim_ubc_seconds));
    } else {
        println!("🎁 Claim UBC: ✅ READY!");
    }

    // Hunt Info
    println!("\n🎮 ===== HUNT INFO =====");
    println!("Hunts Used Today: {}/3", response.hunt_info.hunts_used);
    println!("Hunts Remaining: {}/3", 3 - response.hunt_info.hunts_used);

    // Reset time
    let reset_time_secs = response.hunt_info.reset_time / 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if reset_time_secs > now {
        let time_until_reset = reset_time_secs - now;
        println!("🔄 Hunts Reset In: {}", format_duration(time_until_reset));
    } else {
        println!("🔄 Hunts: Ready to reset!");
    }
}

// Helper function to clean up duration formatting
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

fn format_timestamp(seconds_from_now: u64) -> String {
    use chrono::{Duration, Local};

    let now = Local::now();
    let target = now + Duration::seconds(seconds_from_now as i64);
    target.format("at %H:%M:%S").to_string()
}

fn display_hunt_result(result: &BeetleHuntApiResponse) {
    match result {
        BeetleHuntApiResponse::Success { result, user, .. } => {
            println!("\n╔═══════════════════════════════════════════╗");
            println!("║          🎯 BEETLE HUNT SUCCESS! 🎯       ║");
            println!("╚═══════════════════════════════════════════╝");
            println!("🪲 Beetle: {}", result.beetle_name);
            println!("🏷️  Species: {}", result.beetle_card.species);
            println!("✨ XP gained: +{}", result.xp);
            println!("🧀 Cheese: {} (-20)", user.cheese);
            println!("📊 Level: {} (XP: {})", user.level, user.xp);
            println!("🎯 Hunts used: {}/3", user.beetle_hunts_used);
            println!("╰─────────────────────────────────────────╯\n");
            display_beetle_user(user)
        }
        BeetleHuntApiResponse::FailedHunt { user, .. } => {
            println!("\n╔═══════════════════════════════════════════╗");
            println!("║          🎯 HUNT FAILED - NOTHING! 💨     ║");
            println!("╚═══════════════════════════════════════════╝");
            println!("❌ The beetle got away!");
            println!("🧀 Cheese: {} (-20)", user.cheese);
            println!("🎯 Hunts used: {}/3", user.beetle_hunts_used);
            println!("╰─────────────────────────────────────────╯\n");
        }
        BeetleHuntApiResponse::Error { error, user, .. } => {
            println!("\n╔═══════════════════════════════════════════╗");
            println!("║              ❌ HUNT ERROR ❌              ║");
            println!("╚═══════════════════════════════════════════╝");
            println!("Error: {}", error);
            println!("🧀 Cheese: {}", user.cheese);
            println!("🎯 Hunts used: {}/3", user.beetle_hunts_used);
            println!("╰─────────────────────────────────────────╯\n");
        }
    }
}

fn display_catch_result(response: &CatchBeetleApiResponse) {
    match response {
        CatchBeetleApiResponse::Error {
            success,
            error,
            user,
        } => {
            println!("\n❌ ===== CATCH ERROR =====");
            println!("Success: {}", success);
            println!("Error: {}", error);

            // Parse cooldown from error message if present
            if let Some(seconds_str) = error.split("for ").nth(1) {
                if let Some(seconds) = seconds_str.trim_end_matches('s').parse::<u64>().ok() {
                    println!("\n⏰ Cooldown remaining:");
                    println!(
                        "   {}h {}m {}s",
                        seconds / 3600,
                        (seconds / 60) % 60,
                        seconds % 60
                    );
                }
            }

            println!("\n💡 Check cooldowns to see when you can catch again");
            display_beetle_user(user);
        }
        CatchBeetleApiResponse::Success {
            success: _,
            result,
            user,
        } => {
            println!("\n🐞 ===== CAUGHT BEETLE =====");
            println!("🎯 {}", result.beetle_name);
            println!("🆔 Type: {}", result.beetle);
            println!("⭐ XP Gained: +{}", result.xp);
            println!(
                "⏰ Next catch in: {}h {}m",
                result.cooldown_ms / 1000 / 3600,
                (result.cooldown_ms / 1000 / 60) % 60
            );
            display_beetle_user(user);
        }
    }
}

fn display_poke_result(response: &PokeResponse, username: &str) {
    if response.success {
        println!("\n✅ Successfully poked ~{}", username);
        println!("👉 They'll get a notification!");
    } else {
        println!("\n❌ Failed to poke ~{}", username);
    }
}

fn display_feed_activity(activities: &[FeedActivity]) {
    println!("\n📰 Recent Feed Activity ({} items)\n", activities.len());
    println!("{}", "=".repeat(80));

    for activity in activities {
        match activity {
            FeedActivity::Poke {
                id,
                timestamp,
                from,
                to,
            } => {
                println!("👉 POKE");
                println!("   From: {} ({})", from.username, from.avatar);
                println!("   To:   {} ({})", to.username, to.avatar);
                println!("   Time: {}", timestamp);
                println!("   ID:   {}", id);
            }
            FeedActivity::Friendship {
                id,
                timestamp,
                users,
            } => {
                println!("🤝 FRIENDSHIP");
                for user in users {
                    println!("   User: {} ({})", user.username, user.avatar);
                }
                println!("   Time: {}", timestamp);
                println!("   ID:   {}", id);
            }
        }
        println!("{}", "-".repeat(80));
    }
}

// Display function for UBC claim results
fn display_claim_ubc_result(response: &ClaimUBCApiResponse) {
    match response {
        ClaimUBCApiResponse::Error {
            success,
            error,
            user,
        } => {
            println!("\n❌ ===== UBC CLAIM ERROR =====");
            println!("Success: {}", success);
            println!("Error: {}", error);

            // Parse cooldown from error message if present
            if let Some(seconds_str) = error.split("for ").nth(1) {
                if let Some(seconds) = seconds_str.trim_end_matches('s').parse::<u64>().ok() {
                    println!("\n⏰ Cooldown remaining:");
                    println!(
                        "   {}h {}m {}s",
                        seconds / 3600,
                        (seconds / 60) % 60,
                        seconds % 60
                    );
                }
            }

            println!("\n💡 Check cooldowns to see when you can claim again");
            display_beetle_user(user);
        }
        ClaimUBCApiResponse::Success {
            success: _,
            result,
            user,
        } => {
            println!("\n🎉 ===== UBC CLAIM SUCCESS =====");
            println!("Success: ✅");

            println!("\n🧀 ===== CHEESE CLAIMED =====");
            println!("🧀 Cheese Earned: +{}", result.cheese);
            println!("⭐ XP Gained: +{}", result.xp);
            println!("🔥 Streak: {} days", result.streak);

            display_beetle_user(user);
        }
    }
}

async fn auto_hunt_loop(client: &BeetleApiClient) -> Result<()> {
    loop {
        println!("\n{}", "=".repeat(60));
        println!("🔄 Checking cooldowns...");

        let cooldowns = client.get_cooldowns().await?;
        display_cooldowns(&cooldowns);

        if cooldowns.cooldowns.catch_beetle == 0 && cooldowns.hunt_info.hunts_used < 3 {
            println!("\n✅ Hunt available! Starting hunt...");
            tokio::time::sleep(Duration::from_secs(2)).await;

            match client.beetle_hunt().await {
                Ok(result) => display_hunt_result(&result),
                Err(e) => eprintln!("❌ Hunt failed: {}", e),
            }
        } else {
            let wait_seconds = cooldowns.cooldowns.catch_beetle / 1000;
            if wait_seconds > 0 {
                println!(
                    "\n⏳ Waiting {} seconds until next hunt...",
                    wait_seconds + 5
                );
                tokio::time::sleep(Duration::from_secs(wait_seconds + 5)).await;
            } else {
                println!("\n⚠️  No hunts remaining today");
                break;
            }
        }
    }
    Ok(())
}

async fn auto_catch_loop(client: &BeetleApiClient) -> Result<()> {
    loop {
        println!("\n============================================================");
        println!("🔄 Checking catch availability...");

        let cooldowns = client.get_cooldowns().await?;
        display_cooldowns(&cooldowns);

        let catch_ready = cooldowns.cooldowns.catch_beetle == 0;

        if catch_ready {
            println!("\n✅ Catch ready!");
            tokio::time::sleep(Duration::from_secs(2)).await;

            match client.beetle_catch().await {
                Ok(result) => {
                    display_catch_result(&result);

                    // Check if it was actually an error (cooldown)
                    if let CatchBeetleApiResponse::Error { error, .. } = &result {
                        // Parse cooldown from error message like "Beetle catch on cooldown for 1929s"
                        if let Some(cooldown_secs) = extract_cooldown_from_error(error) {
                            let wait_secs = cooldown_secs.min(3600); // Cap at 1 hour max
                            println!(
                                "\n⏳ Waiting {} seconds ({} minutes) for cooldown...",
                                wait_secs,
                                wait_secs / 60
                            );
                            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                            continue;
                        }
                    }

                    println!("\n⏳ Waiting 5 seconds before next check...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(e) => {
                    eprintln!("❌ Catch failed: {}", e);
                    println!("\n⏳ Retrying in 30 seconds...");
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            }
        } else {
            let wait_ms = cooldowns.cooldowns.catch_beetle;
            let wait_secs = (wait_ms / 1000).min(3600); // Cap at 1 hour, then recheck

            println!(
                "\n⏰ Beetle catch on cooldown for {}h {}m {}s",
                wait_ms / 1000 / 3600,
                (wait_ms / 1000 / 60) % 60,
                (wait_ms / 1000) % 60
            );
            println!("⏳ Waiting {} seconds before checking again...", wait_secs);
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }
    }
}

// Auto-loop for UBC claiming with cooldown management
async fn auto_claim_ubc_loop(client: &BeetleApiClient) -> Result<()> {
    loop {
        println!("\n============================================================");
        println!("🧀 Checking UBC claim availability...");

        let cooldowns = client.get_cooldowns().await?;
        display_cooldowns(&cooldowns);

        let claim_ready = cooldowns.cooldowns.claim_ubc == 0;
        if claim_ready {
            println!("\n✅ UBC claim ready!");
            tokio::time::sleep(Duration::from_secs(2)).await;

            match client.claim_ubc().await {
                Ok(result) => {
                    display_claim_ubc_result(&result);

                    // Check if it was actually an error (cooldown)
                    if let ClaimUBCApiResponse::Error { error, .. } = &result {
                        // Parse cooldown from error message like "UBC claim on cooldown for 86400s"
                        if let Some(cooldown_secs) = extract_cooldown_from_error(error) {
                            let wait_secs = cooldown_secs.min(3600); // Cap at 1 hour max
                            println!(
                                "\n⏳ Waiting {} seconds ({} minutes) for cooldown...",
                                wait_secs,
                                wait_secs / 60
                            );
                            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                            continue;
                        }
                    }

                    // UBC is typically daily, so wait longer
                    println!("\n⏳ Waiting 1 hour before next check...");
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                }
                Err(e) => {
                    eprintln!("❌ Claim failed: {}", e);
                    println!("\n⏳ Retrying in 5 minutes...");
                    tokio::time::sleep(Duration::from_secs(300)).await;
                }
            }
        } else {
            let wait_ms = cooldowns.cooldowns.claim_ubc;

            let wait_secs = (wait_ms / 1000).min(3600); // Cap at 1 hour, then recheck

            println!(
                "\n⏰ UBC claim on cooldown for {}h {}m {}s",
                wait_ms / 1000 / 3600,
                (wait_ms / 1000 / 60) % 60,
                (wait_ms / 1000) % 60
            );
            println!("⏳ Waiting {} seconds before checking again...", wait_secs);
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }
    }
}

// Helper function to extract cooldown seconds from error message
fn extract_cooldown_from_error(error: &str) -> Option<u64> {
    // Parse "Beetle catch on cooldown for 1929s" -> 1929
    if let Some(start) = error.find("for ") {
        if let Some(end) = error[start..].find('s') {
            let num_str = &error[start + 4..start + end];
            return num_str.parse::<u64>().ok();
        }
    }
    None
}

// ===== MAIN =====

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 Beetle Hunt Bot Starting...\n");

    let token = load_auth_token()?;
    let (profile_sid, beetle_sid) = load_remilia_cookies()?;
    let mut client = BeetleApiClient::new(&token, &profile_sid, &beetle_sid)?;

    println!("\nChoose an option:");
    println!("1. Check cooldowns only");
    println!("2. Catch beetle (cooldown-based)");
    println!("3. Hunt beetles (daily limited)");
    println!("4. Auto-catch when ready (loop)");
    println!("5. Auto-hunt when ready (loop)");
    println!("6. View auth status");
    println!("7. View profile (your own)");
    println!("8. View another user's profile");
    println!("9. Poke a user");
    println!("10. View feed activity");
    println!("11. Auto-poke from feed (random)");
    println!("12. Auto-poke from feed (smart - most active)");
    println!("13. Auto-friend from feed");
    println!("14. Auto-claim UBC (cheese) - loop mode");
    println!("15. Scrape Remilia Jackson's friends to DB");
    println!("16. View DB stats");
    println!("17. View enhanced DB stats");
    println!("18. Auto-poke from DB (24h cooldown)");
    println!("19. Auto-friend request from DB");
    println!("20. Auto-engage (poke + friend)");
    println!("21. 🤖 Run ALL workers (auto-everything mode)");
    println!("22. Get cheese count");
    println!("23. 🌙 Update theme to dark mode\n");

    print!("Enter option (1-23): ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let choice: u32 = input.trim().parse().unwrap_or(1);

    match choice {
        1 => {
            let cooldowns = client.get_cooldowns().await?;
            display_cooldowns(&cooldowns);
        }
        2 => {
            let result = client.beetle_catch().await?;
            display_catch_result(&result);
        }
        3 => {
            let result = client.beetle_hunt().await?;
            display_hunt_result(&result);
        }
        4 => {
            auto_catch_loop(&client).await?;
        }
        5 => {
            auto_hunt_loop(&client).await?;
        }
        6 => {
            let auth_status = client.get_auth_status().await?;
            display_auth_status(&auth_status);
        }
        7 => {
            let _cooldowns = client.get_cooldowns().await?;

            print!("Enter your username (without ~): ");
            use std::io::Write;
            std::io::stdout().flush()?;

            let mut username = String::new();
            std::io::stdin().read_line(&mut username)?;
            let username = format!("~{}", username.trim());

            let profile = client.get_profile(&username).await?;
            display_profile(&profile);
        }
        8 => {
            print!("Enter username (with or without ~): ");
            std::io::Write::flush(&mut std::io::stdout())?;

            let mut username = String::new();
            std::io::stdin().read_line(&mut username)?;
            let username = username.trim();

            let profile = client.get_profile(&username).await?;
            display_profile(&profile);
        }
        9 => {
            print!("Enter username to poke (without ~): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut username = String::new();
            std::io::stdin().read_line(&mut username)?;
            let username = username.trim();

            let result = client.poke_user(username).await?;
            display_poke_result(&result, username);
        }
        10 => {
            let activities = client.get_feed_activity().await?;
            display_feed_activity(&activities);
        }
        11 => {
            print!("How many users to poke? (max 50): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            input.clear();
            std::io::stdin().read_line(&mut input)?;
            let max_pokes = input.trim().parse::<usize>().unwrap_or(10).min(50);

            client.auto_poke_from_feed(max_pokes).await?;
        }
        12 => {
            print!("How many users to poke? (max 50): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            input.clear();
            std::io::stdin().read_line(&mut input)?;
            let max_pokes = input.trim().parse::<usize>().unwrap_or(10).min(50);

            print!("Exclude any usernames? (comma-separated, or press Enter to skip): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            input.clear();
            std::io::stdin().read_line(&mut input)?;

            let exclude: Vec<String> = input
                .trim()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            client.auto_poke_smart(max_pokes, exclude).await?;
        }
        13 => {
            print!("How many users to add? (max 50): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            input.clear();
            std::io::stdin().read_line(&mut input)?;
            let max_friends = input.trim().parse::<usize>().unwrap_or(10).min(50);

            client.auto_friends_from_feed(max_friends).await?;
        }
        14 => {
            auto_claim_ubc_loop(&client).await?;
        }
        15 => {
            std::io::Write::flush(&mut std::io::stdout())?;
            client.scrape_all_friends("friends_db.json").await?;
        }
        16 => {
            let db = FriendsDatabase::new("friends_db.json");
            let count = db.count()?;
            let existing = db.load_records()?;

            println!("\n📊 ===== DATABASE STATS =====");
            println!("Total unique usernames: {}", count);
            println!("File: friends_db.json");

            if count > 0 {
                println!("\n📋 Sample (first 10):");
                for (i, username) in existing.iter().take(10).enumerate() {
                    println!("  {}. {:?}", i + 1, username);
                }
            }
        }
        17 => {
            let db = FriendsDatabase::new("friends_db.json");
            let stats = db.get_stats()?;

            println!("\n📊 ===== DATABASE STATISTICS =====");
            println!("Total users: {}", stats.total_users);
            println!("Poked at least once: {}", stats.poked_at_least_once);
            println!("✅ Ready to poke now: {}", stats.pokeable_now);
            println!("Friend requests sent: {}", stats.friend_requests_sent);
            println!("Need friend request: {}", stats.need_friend_request);
        }
        18 => {
            print!("How many to poke (max 50): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let max = input.trim().parse::<usize>().unwrap_or(10).min(50);

            client.auto_poke_from_db("friends_db.json", max).await?;
        }
        19 => {
            print!("How many friend requests (max 50): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let max = input.trim().parse::<usize>().unwrap_or(10).min(50);

            client
                .auto_friend_request_from_db("friends_db.json", max)
                .await?;
        }
        20 => {}
        21 => {
            run_all_workers(client).await?;
        }
        23 => {
            println!("🌙 Updating theme to dark mode...");
            match client.update_theme_to_dark().await {
                Ok(response) => {
                    if response.success {
                        println!("✅ Theme successfully updated to dark mode! 🌙");
                    } else {
                        eprintln!("❌ Theme update returned false");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to update theme: {}", e);
                }
            }
        }
        _ => println!("❌ Invalid option. Please choose 1-23."),
    }

    Ok(())
}

async fn run_all_workers(client: BeetleApiClient) -> Result<()> {
    println!("🤖 ===== STARTING ALL WORKERS =====\n");

    let client = Arc::new(client);
    let stats = WorkerStats::new();

    // Spawn all workers with stats
    let beetle_worker = tokio::spawn(beetle_auto_claim_worker(Arc::clone(&client), stats.clone()));
    let cheese_worker = tokio::spawn(cheese_auto_claim_worker(Arc::clone(&client), stats.clone()));
    let poke_worker = tokio::spawn(daily_poke_worker(Arc::clone(&client), stats.clone()));
    let dashboard_worker =
        tokio::spawn(status_dashboard_worker(stats.clone(), Arc::clone(&client)));
    // let scrape_worker = tokio::spawn(daily_scrape_worker(Arc::clone(&client)));

    println!("✅ All workers started!");
    println!("   🪲 Beetle auto-claim");
    println!("   🧀 Cheese auto-claim");
    println!("   👉 Daily poke/friend");
    println!("   📊 Status dashboard");
    println!("   📥 Daily friend scraper\n");
    println!("Press Ctrl+C to stop all workers\n");

    // Wait for Ctrl+C
    signal::ctrl_c().await?;

    println!("\n🛑 Shutting down workers...");

    // Abort all tasks
    beetle_worker.abort();
    cheese_worker.abort();
    poke_worker.abort();
    dashboard_worker.abort();
    // scrape_worker.abort();

    println!("✅ All workers stopped");

    Ok(())
}

// ===== WORKER 0: Status Dashboard =====
async fn status_dashboard_worker(stats: WorkerStats, client: Arc<BeetleApiClient>) -> Result<()> {
    println!("📊 [DASHBOARD] Starting status worker...\n");

    let mut interval = interval(Duration::from_secs(5 * 60));

    loop {
        interval.tick().await;

        match client.get_beetle_user().await {
            Ok(user) => {
                stats.update_inventory(user.inventory.clone());
            }
            Err(e) => {
                eprintln!("⚠️  Failed to fetch beetle user: {}", e);
            }
        }

        let inventory = stats.get_inventory();
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

        // 🆕 Display missed hunts (beetle escaped)
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
            "║ 🧀 Last Cheese:                   {:>24} ║",
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
        println!("╚═══════════════════════════════════════════════════════════╝\n");
    }
}

fn format_time_ago(timestamp_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
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

async fn beetle_auto_claim_worker(client: Arc<BeetleApiClient>, stats: WorkerStats) -> Result<()> {
    println!("🪲 [BEETLE WORKER] Starting...\n");

    match client.get_beetle_user().await {
        Ok(user) => {
            stats.init_inventory(user.inventory.clone());
            println!(
                "✅ Session initialized with {} total beetles",
                stats.get_inventory().total_beetles()
            );
        }
        Err(e) => {
            eprintln!("⚠️  Could not initialize inventory: {}", e);
        }
    }

    loop {
        match client.get_beetle_user().await {
            Ok(user) => {
                user.display_status();
                stats.update_inventory(user.inventory.clone());

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
                                stats.update_inventory(user.inventory);
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
                                            stats.update_inventory(user.inventory);
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
                                            stats.update_inventory(user.inventory);
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
// ===== WORKER 2: Auto-claim cheese (updated with stats) =====
async fn cheese_auto_claim_worker(client: Arc<BeetleApiClient>, stats: WorkerStats) -> Result<()> {
    println!("🧀 [CHEESE WORKER] Starting...\n");

    loop {
        match client.get_cooldowns().await {
            Ok(cooldowns) => {
                let claim_ubc_seconds = cooldowns.time_until_ubc_ready();

                if cooldowns.can_claim_ubc() {
                    println!("🎯 UBC claim is ready!");

                    match client.claim_ubc().await {
                        Ok(result) => {
                            display_claim_ubc_result(&result);
                            stats.increment_cheese(); // ✅ Track cheese claimed

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

// ===== WORKER 3: Daily poke + friend (updated with stats) =====
async fn daily_poke_worker(client: Arc<BeetleApiClient>, stats: WorkerStats) -> Result<()> {
    println!("👉 [POKE WORKER] Starting...\n");

    let mut interval = interval(Duration::from_secs(30 * 60)); // Check every 30 minutes

    loop {
        interval.tick().await;

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

        // time in 30 minutes
        let next_check = chrono::Local::now() + chrono::Duration::minutes(30);
        println!(
            "👉 [POKE] ⏰ Next check in 30 minutes, at {}",
            next_check.format("%Y-%m-%d %H:%M:%S")
        );
    }
}

// ===== WORKER 4: Daily scraper =====
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
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    fn increment_cheese(&self) {
        self.cheese_claimed.fetch_add(1, Ordering::Relaxed);
        self.last_cheese_time.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
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
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    fn increment_friends(&self, count: u64) {
        self.users_friended.fetch_add(count, Ordering::Relaxed);
    }

    fn init_inventory(&self, inventory: BeetleInventory) {
        if let Ok(mut inv) = self.beetle_inventory.lock() {
            *inv = SessionBeetleInventory::new(inventory);
        }
    }

    fn update_inventory(&self, inventory: BeetleInventory) {
        if let Ok(mut inv) = self.beetle_inventory.lock() {
            inv.update(inventory);
        }
    }

    fn get_inventory(&self) -> SessionBeetleInventory {
        self.beetle_inventory
            .lock()
            .map(|inv| inv.clone())
            .unwrap_or_default()
    }
}

impl User {
    pub fn total_beetles(&self) -> u32 {
        self.inventory.green
            + self.inventory.ladybug
            + self.inventory.monarch
            + self.inventory.pond
            + self.inventory.bombardier
            + self.inventory.skull
            + self.inventory.purple
    }

    pub fn can_catch_beetle(&self) -> bool {
        self.cooldowns.catch_beetle == 0
    }

    pub fn can_claim_ubc(&self) -> bool {
        self.cooldowns.claim_ubc == 0
    }

    pub fn time_until_catch_ready(&self) -> u64 {
        if self.cooldowns.catch_beetle == 0 {
            0
        } else {
            // Convert milliseconds to seconds, round up
            (self.cooldowns.catch_beetle + 999) / 1000
        }
    }

    pub fn time_until_ubc_ready(&self) -> u64 {
        if self.cooldowns.claim_ubc == 0 {
            0
        } else {
            // Convert milliseconds to seconds, round up
            (self.cooldowns.claim_ubc + 999) / 1000
        }
    }

    pub fn has_hunts_remaining(&self) -> bool {
        self.beetle_hunts_used < 3
    }

    pub fn hunts_remaining(&self) -> u32 {
        3_u32.saturating_sub(self.beetle_hunts_used)
    }

    pub fn is_new_hunt_day(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Hunts reset 1.5 hours (5400 seconds = 5,400,000 ms) after last hunt
        let reset_time = self.last_beetle_hunt_date + 5_400_000;

        now >= reset_time
    }
    /// Calculate time until hunts reset (midnight UTC)
    pub fn time_until_hunt_reset(&self) -> u64 {
        if self.is_new_hunt_day() {
            return 0; // Already reset
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Reset time is 1.5 hours (5,400,000 ms) after last hunt
        let reset_time = self.last_beetle_hunt_date + 5_400_000;

        if reset_time > now {
            (reset_time - now) / 1000 // Convert ms to seconds
        } else {
            0
        }
    }

    /// Display formatted status
    pub fn display_status(&self) {
        println!("┌─────────────────────────────────────────────┐");
        println!("│ 📊 BEETLE STATUS                            │");
        println!("├─────────────────────────────────────────────┤");
        println!(
            "│ Level: {}  |  XP: {}  |  Cheese: {} 🧀   │",
            self.level, self.xp, self.cheese
        );
        println!(
            "│ Total Beetles: {}                          │",
            self.total_beetles()
        );
        println!("├─────────────────────────────────────────────┤");
        println!("│ COOLDOWNS:                                  │");

        if self.can_catch_beetle() {
            println!("│ 🪲 Catch: ✅ READY                          │");
        } else {
            println!(
                "│ 🪲 Catch: {} remaining                    │",
                format_duration(self.time_until_catch_ready())
            );
        }

        if self.can_claim_ubc() {
            println!("│ 🧀 UBC: ✅ READY                            │");
        } else {
            println!(
                "│ 🧀 UBC: {} remaining                      │",
                format_duration(self.time_until_ubc_ready())
            );
        }

        if self.has_hunts_remaining() {
            println!(
                "│ 🎯 Hunts: {} remaining                    │",
                self.hunts_remaining()
            );
        } else {
            println!(
                "│ 🎯 Hunts: Reset in {}                    │",
                format_duration(self.time_until_hunt_reset())
            );
        }

        println!("└─────────────────────────────────────────────┘\n");
    }
}
