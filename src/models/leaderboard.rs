use serde::{Deserialize, Serialize};

/// Leaderboard entry for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub username: String,
    pub display_name: String,
    pub beetles: u32,
    pub pokes: u32,
    pub social_credit: i64,
    pub pfp_url: Option<String>,
}

/// Leaderboard statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct LeaderboardStats {
    pub entries: Vec<LeaderboardEntry>,
    pub total_users: usize,
}

/// Sort criteria for leaderboard
#[derive(Debug, Clone, Copy)]
pub enum SortBy {
    Beetles,
    Pokes,
    SocialCredit,
}

impl LeaderboardStats {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            total_users: 0,
        }
    }

    pub fn add_entry(&mut self, entry: LeaderboardEntry) {
        self.entries.push(entry);
        self.total_users = self.entries.len();
    }

    pub fn sort_by(&mut self, sort_by: SortBy) {
        match sort_by {
            SortBy::Beetles => {
                self.entries.sort_by(|a, b| b.beetles.cmp(&a.beetles));
            }
            SortBy::Pokes => {
                self.entries.sort_by(|a, b| b.pokes.cmp(&a.pokes));
            }
            SortBy::SocialCredit => {
                self.entries
                    .sort_by(|a, b| b.social_credit.cmp(&a.social_credit));
            }
        }
    }
}

impl Default for LeaderboardStats {
    fn default() -> Self {
        Self::new()
    }
}
