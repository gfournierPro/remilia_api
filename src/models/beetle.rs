use crate::utils::format_duration;
use serde::{Deserialize, Serialize};

// ============================================================================
// CORE USER & INVENTORY MODELS
// ============================================================================

/// Beetle user information
#[derive(Debug, Deserialize, Clone, Default, PartialEq, Serialize)]
pub struct User {
    pub id: i64,
    #[serde(rename = "keycloak_id")]
    pub keycloak_id: String,
    pub xp: i64,
    pub level: i64,
    pub inventory: BeetleInventory,
    #[serde(rename = "UBCStreak")]
    pub ubc_streak: i64,
    #[serde(rename = "LastCheeseClaimAt")]
    pub last_cheese_claim_at: i64,
    #[serde(rename = "LastBeetleClaimAt")]
    pub last_beetle_claim_at: i64,
    #[serde(rename = "lastBeetleHuntDate")]
    pub last_beetle_hunt_date: i64,
    #[serde(rename = "beetleHuntsUsed")]
    pub beetle_hunts_used: i64,
    #[serde(rename = "LousyBeetleCount")]
    pub lousy_beetle_count: i64,
    #[serde(rename = "created_at")]
    pub created_at: i64,
    #[serde(rename = "updated_at")]
    pub updated_at: i64,
    #[serde(rename = "santafy_credits_available", default)]
    pub santafy_credits_available: i64,
    #[serde(default)]
    pub cooldowns: Cooldowns,
    #[serde(default)]
    pub streaks: Streaks,
    #[serde(rename = "levelInfo", default)]
    pub level_info: LevelInfo,
    #[serde(default)]
    pub discovered: serde_json::Value,
}

/// Beetle inventory
#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct BeetleInventory {
    #[serde(default)]
    pub cheese: i64,
    #[serde(default)]
    pub green: i64,
    #[serde(default)]
    pub ladybug: i64,
    #[serde(default)]
    pub monarch: i64,
    #[serde(default)]
    pub pond: i64,
    #[serde(default)]
    pub bombardier: i64,
    #[serde(default)]
    pub purple: i64,
    #[serde(default)]
    pub skull: i64,
    #[serde(default)]
    pub stag: i64,
    #[serde(default)]
    pub golden: i64,
    #[serde(default)]
    pub goliath: i64,
    #[serde(default)]
    pub black_widow: i64,
}

/// Session beetle inventory tracking
#[derive(Debug, Clone, Default)]
pub struct SessionBeetleInventory {
    pub current: BeetleInventory,
    pub session_start: BeetleInventory,
}

/// Level information
#[derive(Debug, Deserialize, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelInfo {
    #[serde(deserialize_with = "deserialize_number_from_any")]
    pub xp_in_current_level: f64, // Will be computed from User.xp
    #[serde(deserialize_with = "deserialize_number_from_any")]
    pub xp_for_next_level: f64,
    #[serde(deserialize_with = "deserialize_number_from_any")]
    pub xp_needed_for_next: f64,
    #[serde(deserialize_with = "deserialize_number_from_any")]
    pub progress_percent: f64,
}

/// Custom deserializer to handle both regular numbers and overflow/underflow values
fn deserialize_number_from_any<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct NumberVisitor;

    impl<'de> Visitor<'de> for NumberVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a number or an overflow value")
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Handle overflow - if it's a huge u64, it's likely an overflow
            // These large values are actually negative numbers in disguise
            if value > i64::MAX as u64 {
                // Convert via i64 to get the actual intended negative value
                Ok((value as i64) as f64)
            } else if value > 1_000_000_000_000 {
                // Values over 1 trillion are likely overflow artifacts (e.g., progressPercent)
                // Treat as 0 or calculate the actual percentage
                Ok(0.0)
            } else {
                Ok(value as f64)
            }
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Handle extremely large f64 values that might be overflow artifacts
            // Scientific notation values like 3.57e17 are likely corrupted percentages
            if value.abs() > 1e15 {
                // Likely an overflow artifact, treat as invalid/zero
                Ok(0.0)
            } else {
                Ok(value)
            }
        }
    }

    deserializer.deserialize_any(NumberVisitor)
}

/// Streaks information
#[derive(Debug, Deserialize, Clone, Default, PartialEq, Serialize)]
pub struct Streaks {
    pub ubc: i64,
    #[serde(rename = "lastClaim")]
    pub last_claim: i64,
}

/// Cooldowns information
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cooldowns {
    #[serde(rename = "catchBeetle", default)]
    pub catch_beetle: i64,
    #[serde(rename = "claimUBC", default)]
    pub claim_ubc: i64,
}


// ============================================================================
// BEETLE CARD & VIDEO INFO
// ============================================================================

/// Video information for beetle catch
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub src: String,
    pub poster: String,
    #[serde(rename = "loop", default)]
    pub loop_video: bool,
}

/// Beetle card information
#[derive(Debug, Deserialize, Clone)]
pub struct BeetleCard {
    pub beetle_name: String,
    pub girl_name: String,
    pub species: String,
    #[serde(default)]
    pub species_latin: String,
    pub icon: String,
    #[serde(default)]
    pub img: String,
    pub background: String,
    pub character: String,
}

// ============================================================================
// API REQUEST/RESPONSE MODELS
// ============================================================================

/// Claim UBC request
#[derive(Debug, Serialize)]
pub struct ClaimUBCRequest {}

/// Claim UBC result
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimUBCResult {
    pub cheese: u32,
    pub xp: u32,
    pub streak: u32,
}

/// Claim UBC API response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum ClaimUBCApiResponse {
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

/// Catch beetle request
#[derive(Debug, Serialize)]
pub struct CatchBeetleRequest {}

/// Catch beetle result
#[derive(Debug, Deserialize, Clone)]
pub struct CatchBeetleResult {
    pub beetle: String,
    pub beetle_name: String,
    pub xp: u32,
    #[serde(rename = "cooldownMs")]
    pub cooldown_ms: u64,
    #[serde(rename = "catchVideoInfo")]
    pub catch_video_info: VideoInfo,
    #[serde(rename = "beetleCard")]
    pub beetle_card: BeetleCard,
}

/// Catch beetle API response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum CatchBeetleApiResponse {
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

/// Beetle hunt request
#[derive(Debug, Serialize)]
pub struct BeetleHuntRequest {}

/// Failed hunt result
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)] // Fail to match if extra fields present
pub struct FailedHuntResult {
    pub success: bool,
}

/// Beetle hunt result
#[derive(Debug, Deserialize, Clone)]
pub struct BeetleHuntResult {
    pub success: bool,
    pub beetle: String,
    pub beetle_name: String,
    pub xp: u32,
    #[serde(rename = "catchVideoInfo")]
    pub catch_video_info: VideoInfo,
    #[serde(rename = "beetleCard")]
    pub beetle_card: BeetleCard,
}

/// Beetle hunt API response
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum BeetleHuntApiResponse {
    // Try Success FIRST (most specific - requires beetle, beetle_name, xp, etc.)
    Success {
        success: bool,
        result: BeetleHuntResult,
        user: User,
    },
    // Then Error (has unique 'error' field)
    Error {
        success: bool,
        error: String,
        user: User,
    },
    // Finally FailedHunt (least specific - only requires success field)
    FailedHunt {
        success: bool,
        result: FailedHuntResult,
        user: User,
    },
}

// ============================================================================
// IMPLEMENTATION: User
// ============================================================================

impl User {
    /// Get consolidated beetle inventory from User fields
    pub fn get_inventory(&self) -> BeetleInventory {
        self.inventory.clone()
    }

    pub fn cheese(&self) -> i64 {
        self.inventory.cheese
    }

    pub fn total_beetles(&self) -> i64 {
        self.inventory.green
            + self.inventory.ladybug
            + self.inventory.monarch
            + self.inventory.pond
            + self.inventory.bombardier
            + self.inventory.purple
            + self.inventory.skull
            + self.inventory.stag
            + self.inventory.golden
            + self.inventory.goliath
            + self.inventory.black_widow
    }

    pub fn can_catch_beetle(&self) -> bool {
        self.cooldowns.catch_beetle == 0
    }

    pub fn can_claim_ubc(&self) -> bool {
        self.cooldowns.claim_ubc == 0
    }

    pub fn time_until_catch_ready(&self) -> u64 {
        if self.can_catch_beetle() {
            0
        } else {
            (self.cooldowns.catch_beetle / 1000) as u64
        }
    }

    pub fn time_until_ubc_ready(&self) -> u64 {
        if self.can_claim_ubc() {
            0
        } else {
            (self.cooldowns.claim_ubc / 1000) as u64
        }
    }

    pub fn has_hunts_remaining(&self) -> bool {
        if self.beetle_hunts_used >= 3 {
            // Check if 1.5 hours (5,400 seconds) have passed since last hunt
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // last_beetle_hunt_date is in milliseconds
            let reset_time = self.last_beetle_hunt_date as u64 + 5_400_000; // 1.5 hours in ms

            // If current time is past reset time, we can hunt again (API bug workaround)
            now >= reset_time
        } else {
            true
        }
    }

    pub fn hunts_remaining(&self) -> i64 {
        if self.beetle_hunts_used >= 3 {
            // Check if 1.5 hours (5,400 seconds) have passed since last hunt
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // last_beetle_hunt_date is in milliseconds
            let reset_time = self.last_beetle_hunt_date as u64 + 5_400_000; // 1.5 hours in ms

            // If current time is past reset time, we can hunt again (API bug workaround)
            if now >= reset_time {
                3 // Reset available, treat as if we have all hunts
            } else {
                0 // Still in cooldown, no hunts available
            }
        } else {
            3_i64.saturating_sub(self.beetle_hunts_used)
        }
    }

    pub fn is_new_hunt_day(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let last_hunt_day = (self.last_beetle_hunt_date / (24 * 60 * 60)) as u64;
        let today = now / (24 * 60 * 60);

        today > last_hunt_day
    }

    pub fn time_until_hunt_reset(&self) -> u64 {
        if self.is_new_hunt_day() {
            return 0; // Already reset
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Reset time is 1.5 hours (5,400,000 ms) after last hunt
        let reset_time = self.last_beetle_hunt_date as u64 + 5_400_000;

        if reset_time > now {
            (reset_time - now) / 1000 // Convert ms to seconds
        } else {
            0
        }
    }

    pub fn display_status(&self) {
        println!("\n╔═══════════════════════════════════════════╗");
        println!("║         🪲 BEETLE USER STATUS             ║");
        println!("╠═══════════════════════════════════════════╣");
        println!("║ Level:                    {:>15} ║", self.level);
        println!("║ XP:                       {:>15} ║", self.xp);
        println!("║ Cheese:                   {:>15} 🧀║", self.cheese());
        println!("║ Total Beetles:            {:>15} ║", self.total_beetles());
        println!("╠═══════════════════════════════════════════╣");
        println!("║ ⏰ COOLDOWNS                              ║");
        println!("╠═══════════════════════════════════════════╣");

        if self.can_catch_beetle() {
            println!("║ 🪲 Catch:                 {:>15} ║", "✅ Ready!");
        } else {
            let time = self.time_until_catch_ready();
            println!(
                "║ 🪲 Catch:                 {:>15} ║",
                format_duration(time)
            );
        }

        if self.can_claim_ubc() {
            println!("║ 🧀 UBC Claim:             {:>15} ║", "✅ Ready!");
        } else {
            let time = self.time_until_ubc_ready();
            println!(
                "║ 🧀 UBC Claim:             {:>15} ║",
                format_duration(time)
            );
        }

        println!("╠═══════════════════════════════════════════╣");
        println!("║ 🎯 HUNTS                                  ║");
        println!("╠═══════════════════════════════════════════╣");
        println!(
            "║ Used Today:               {:>15} ║",
            self.beetle_hunts_used
        );
        println!(
            "║ Remaining:                {:>15} ║",
            self.hunts_remaining()
        );
        println!("╚═══════════════════════════════════════════╝\n");
    }
}

// ============================================================================
// IMPLEMENTATION: BeetleInventory
// ============================================================================

impl BeetleInventory {
    pub fn total_beetles(&self) -> i64 {
        self.green
            + self.ladybug
            + self.monarch
            + self.pond
            + self.bombardier
            + self.purple
            + self.skull
            + self.stag
            + self.golden
            + self.goliath
            + self.black_widow
    }

    pub fn get_sorted_beetles(&self) -> Vec<(&str, &str, i64, &str)> {
        vec![
            ("🟢", "Green", self.green, "common"),
            ("🔴", "Ladybug", self.ladybug, "common"),
            ("🟠", "Monarch", self.monarch, "uncommon"),
            ("🔵", "Pond", self.pond, "uncommon"),
            ("🪲", "Stag", self.stag, "uncommon"),
            ("⚫", "Bombardier", self.bombardier, "rare"),
            ("🟡", "Golden", self.golden, "rare"),
            ("🟣", "Purple", self.purple, "epic"),
            ("🦏", "Goliath", self.goliath, "epic"),
            ("💀", "Skull", self.skull, "legendary"),
            ("🕷️", "Black Widow", self.black_widow, "legendary"),
        ]
    }
}

// ============================================================================
// IMPLEMENTATION: SessionBeetleInventory
// ============================================================================

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

    fn get_count(&self, inv: &BeetleInventory, beetle_type: &str) -> i64 {
        match beetle_type {
            "green" => inv.green,
            "ladybug" => inv.ladybug,
            "monarch" => inv.monarch,
            "pond" => inv.pond,
            "stag" => inv.stag,
            "bombardier" => inv.bombardier,
            "golden" => inv.golden,
            "purple" => inv.purple,
            "goliath" => inv.goliath,
            "skull" => inv.skull,
            "black_widow" => inv.black_widow,
            _ => 0,
        }
    }

    pub fn total_beetles(&self) -> i64 {
        self.current.green
            + self.current.ladybug
            + self.current.monarch
            + self.current.pond
            + self.current.stag
            + self.current.bombardier
            + self.current.golden
            + self.current.purple
            + self.current.goliath
            + self.current.skull
            + self.current.black_widow
    }

    pub fn total_session_gain(&self) -> i32 {
        let current_total = self.total_beetles();
        let start_total = self.session_start.green
            + self.session_start.ladybug
            + self.session_start.monarch
            + self.session_start.pond
            + self.session_start.stag
            + self.session_start.bombardier
            + self.session_start.golden
            + self.session_start.purple
            + self.session_start.goliath
            + self.session_start.skull
            + self.session_start.black_widow;

        current_total as i32 - start_total as i32
    }

    pub fn get_sorted_beetles(&self) -> Vec<(&str, &str, i64, i32, &str)> {
        vec![
            (
                "🟢",
                "Green",
                self.current.green,
                self.get_session_delta("green"),
                "common",
            ),
            (
                "🔴",
                "Ladybug",
                self.current.ladybug,
                self.get_session_delta("ladybug"),
                "common",
            ),
            (
                "🟠",
                "Monarch",
                self.current.monarch,
                self.get_session_delta("monarch"),
                "uncommon",
            ),
            (
                "🔵",
                "Pond",
                self.current.pond,
                self.get_session_delta("pond"),
                "uncommon",
            ),
            (
                "🪲",
                "Stag",
                self.current.stag,
                self.get_session_delta("stag"),
                "uncommon",
            ),
            (
                "⚫",
                "Bombardier",
                self.current.bombardier,
                self.get_session_delta("bombardier"),
                "rare",
            ),
            (
                "🟡",
                "Golden",
                self.current.golden,
                self.get_session_delta("golden"),
                "rare",
            ),
            (
                "🟣",
                "Purple",
                self.current.purple,
                self.get_session_delta("purple"),
                "epic",
            ),
            (
                "🦏",
                "Goliath",
                self.current.goliath,
                self.get_session_delta("goliath"),
                "epic",
            ),
            (
                "💀",
                "Skull",
                self.current.skull,
                self.get_session_delta("skull"),
                "legendary",
            ),
            (
                "🕷️",
                "Black Widow",
                self.current.black_widow,
                self.get_session_delta("black_widow"),
                "legendary",
            ),
        ]
    }
}
