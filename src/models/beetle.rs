use serde::{Deserialize, Serialize};
use crate::utils::format_duration;

/// Beetle inventory
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
            ("🟢", "Green", self.green, "common"),
            ("🔴", "Ladybug", self.ladybug, "common"),
            ("🟠", "Monarch", self.monarch, "uncommon"),
            ("🔵", "Pond", self.pond, "uncommon"),
            ("⚫", "Bombardier", self.bombardier, "rare"),
            ("🟣", "Purple", self.purple, "epic"),
            ("💀", "Skull", self.skull, "legendary"),
        ]
    }
}

/// Session beetle inventory tracking
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
            "purple" => inv.purple,
            "skull" => inv.skull,
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
                "⚫",
                "Bombardier",
                self.current.bombardier,
                self.get_session_delta("bombardier"),
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
                "💀",
                "Skull",
                self.current.skull,
                self.get_session_delta("skull"),
                "legendary",
            ),
        ]
    }
}

/// Cooldowns information
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cooldowns {
    #[serde(rename = "catchBeetle", default)]
    pub catch_beetle: u64,
    #[serde(rename = "claimUBC", default)]
    pub claim_ubc: u64,
}

/// Hunt information
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HuntInfo {
    pub hunts_used: u32,
    pub reset_time: u64,
}

/// Cooldowns response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CooldownsResponse {
    pub cooldowns: Cooldowns,
    pub hunt_info: HuntInfo,
}

impl CooldownsResponse {
    pub fn can_beetle_catch(&self) -> bool {
        self.cooldowns.catch_beetle == 0
    }

    pub fn can_claim_ubc(&self) -> bool {
        self.cooldowns.claim_ubc == 0
    }

    pub fn has_hunts_remaining(&self) -> bool {
        self.hunt_info.hunts_used < 3
    }

    pub fn time_until_beetle_ready(&self) -> u64 {
        self.cooldowns.catch_beetle / 1000
    }

    pub fn time_until_ubc_ready(&self) -> u64 {
        self.cooldowns.claim_ubc / 1000
    }

    pub fn time_until_hunt_reset(&self) -> u64 {
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
            0
        }
    }
}

/// Streaks information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Streaks {
    pub ubc: u32,
    pub last_claim: u64,
    pub lousy_beetle: u32,
    #[serde(default)]
    pub pity_counter: u32,
}

/// Level information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LevelInfo {
    pub xp_in_current_level: i32,
    pub xp_for_next_level: u32,
    pub xp_needed_for_next: u32,
    pub progress_percent: f64,
}

/// Beetle user information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub cheese: u32,
    pub xp: u32,
    pub level: u32,
    pub inventory: BeetleInventory,
    pub discovered: Vec<String>,
    pub streaks: Streaks,
    #[serde(rename = "beetleHuntsUsed")]
    pub beetle_hunts_used: u32,
    #[serde(rename = "lastBeetleHuntDate")]
    pub last_beetle_hunt_date: u64,
    #[serde(rename = "levelInfo")]
    pub level_info: LevelInfo,
    pub cooldowns: Cooldowns,
}

impl User {
    pub fn total_beetles(&self) -> u32 {
        self.inventory.total_beetles()
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
            self.cooldowns.catch_beetle / 1000
        }
    }

    pub fn time_until_ubc_ready(&self) -> u64 {
        if self.can_claim_ubc() {
            0
        } else {
            self.cooldowns.claim_ubc / 1000
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
            .as_secs();

        let last_hunt_day = self.last_beetle_hunt_date / (24 * 60 * 60);
        let today = now / (24 * 60 * 60);

        today > last_hunt_day
    }

    pub fn time_until_hunt_reset(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let seconds_since_epoch = now;
        let seconds_today = seconds_since_epoch % (24 * 60 * 60);
        let seconds_until_midnight = (24 * 60 * 60) - seconds_today;

        seconds_until_midnight
    }

    pub fn display_status(&self) {
        println!("\n╔═══════════════════════════════════════════╗");
        println!("║         🪲 BEETLE USER STATUS             ║");
        println!("╠═══════════════════════════════════════════╣");
        println!("║ Level:                    {:>15} ║", self.level);
        println!("║ XP:                       {:>15} ║", self.xp);
        println!("║ Cheese:                   {:>15} 🧀║", self.cheese);
        println!("║ Total Beetles:            {:>15} ║", self.total_beetles());
        println!("╠═══════════════════════════════════════════╣");
        println!("║ ⏰ COOLDOWNS                              ║");
        println!("╠═══════════════════════════════════════════╣");

        if self.can_catch_beetle() {
            println!("║ 🪲 Catch:                 {:>15} ║", "✅ Ready!");
        } else {
            let time = self.time_until_catch_ready();
            println!("║ 🪲 Catch:                 {:>15} ║", format_duration(time));
        }

        if self.can_claim_ubc() {
            println!("║ 🧀 UBC Claim:             {:>15} ║", "✅ Ready!");
        } else {
            let time = self.time_until_ubc_ready();
            println!("║ 🧀 UBC Claim:             {:>15} ║", format_duration(time));
        }

        println!("╠═══════════════════════════════════════════╣");
        println!("║ 🎯 HUNTS                                  ║");
        println!("╠═══════════════════════════════════════════╣");
        println!("║ Used Today:               {:>15} ║", self.beetle_hunts_used);
        println!("║ Remaining:                {:>15} ║", self.hunts_remaining());
        println!("╚═══════════════════════════════════════════╝\n");
    }
}

/// Video information for beetle catch
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub src: String,
    pub poster: String,
    #[serde(rename = "loop")]
    pub loop_video: bool,
}

/// Beetle card information
#[derive(Debug, Deserialize, Clone)]
pub struct BeetleCard {
    pub beetle_name: String,
    pub girl_name: String,
    pub species: String,
    pub species_latin: String,
    pub icon: String,
    pub background: String,
    pub character: String,
}

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
pub struct FailedHuntResult {
    pub success: bool,
}

/// Beetle hunt result
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BeetleHuntResult {
    pub success: bool,
    pub beetle: String,
    #[serde(rename = "beetle_name")]
    pub beetle_name: String,
    pub xp: u32,
    pub catch_video_info: VideoInfo,
    pub beetle_card: BeetleCard,
}

/// Beetle hunt API response
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
