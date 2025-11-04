use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// User record in the friends database
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserRecord {
    pub username: String,
    #[serde(default)]
    pub last_poke: Option<i64>,
    #[serde(default)]
    pub friend_request_sent: bool,
    #[serde(default)]
    pub friend_request_date: Option<i64>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Database statistics
#[derive(Debug)]
pub struct DatabaseStats {
    pub total_users: usize,
    pub poked_at_least_once: usize,
    pub pokeable_now: usize,
    pub friend_requests_sent: usize,
    pub need_friend_request: usize,
}

/// Sync statistics
#[derive(Debug)]
pub struct SyncStats {
    pub initial_count: usize,
    pub final_count: usize,
    pub added: usize,
    pub removed: usize,
}

/// Next pokeable user information
#[derive(Debug, Clone)]
pub struct NextPokeableUser {
    pub username: String,
    pub time_until_pokeable: u64,
}

/// Friends database manager
pub struct FriendsDatabase {
    file_path: String,
}

impl FriendsDatabase {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }

    pub fn load_records(&self) -> Result<HashMap<String, UserRecord>> {
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

    pub fn count(&self) -> Result<usize> {
        Ok(self.load_records()?.len())
    }

    pub fn save_records(&self, records: &HashMap<String, UserRecord>) -> Result<()> {
        let json = serde_json::to_string_pretty(records)?;
        std::fs::write(&self.file_path, json)?;
        Ok(())
    }

    pub fn add_username(&self, username: &str) -> Result<bool> {
        let mut records = self.load_records()?;

        if records.contains_key(username) {
            return Ok(false);
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

    pub fn add_usernames(&self, usernames: &[String]) -> Result<usize> {
        let mut records = self.load_records()?;
        let mut added = 0;

        for username in usernames {
            if !records.contains_key(username) {
                records.insert(
                    username.clone(),
                    UserRecord {
                        username: username.clone(),
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

    pub fn update_poke(&self, username: &str) -> Result<()> {
        let mut records = self.load_records()?;

        if let Some(record) = records.get_mut(username) {
            record.last_poke = Some(chrono::Utc::now().timestamp());
            self.save_records(&records)?;
        }

        Ok(())
    }

    pub fn update_poke_with_cooldown(&self, username: &str, cooldown_seconds: i64) -> Result<()> {
        let mut records = self.load_records()?;

        if let Some(record) = records.get_mut(username) {
            let now = chrono::Utc::now().timestamp();
            let next_pokeable_time = now + cooldown_seconds;
            record.last_poke = Some(next_pokeable_time - (24 * 60 * 60));
            self.save_records(&records)?;
        }

        Ok(())
    }

    pub fn mark_friend_request_sent(&self, username: &str) -> Result<()> {
        let mut records = self.load_records()?;

        if let Some(record) = records.get_mut(username) {
            record.friend_request_sent = true;
            record.friend_request_date = Some(chrono::Utc::now().timestamp());
            self.save_records(&records)?;
        }

        Ok(())
    }

    pub fn get_pokeable_users(&self) -> Result<Vec<String>> {
        let records = self.load_records()?;
        let now = chrono::Utc::now().timestamp();
        let day_in_seconds = 24 * 60 * 60;

        let pokeable: Vec<String> = records
            .values()
            .filter(|record| {
                record.last_poke.map_or(true, |last| now - last >= day_in_seconds)
            })
            .map(|record| record.username.clone())
            .collect();

        Ok(pokeable)
    }

    pub fn get_users_for_friend_request(&self) -> Result<Vec<String>> {
        let records = self.load_records()?;

        let users: Vec<String> = records
            .values()
            .filter(|record| !record.friend_request_sent)
            .map(|record| record.username.clone())
            .collect();

        Ok(users)
    }

    pub fn get_stats(&self) -> Result<DatabaseStats> {
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
            friend_requests_sent: records.len() - need_friend_request,
            need_friend_request,
        })
    }

    pub fn get_user(&self, username: &str) -> Result<Option<UserRecord>> {
        let records = self.load_records()?;
        Ok(records.get(username).cloned())
    }

    pub fn remove_users_not_in_list(&self, usernames_to_keep: &[String]) -> Result<usize> {
        let mut records = self.load_records()?;
        let initial_count = records.len();

        let keep_set: std::collections::HashSet<_> = usernames_to_keep.iter().collect();

        records.retain(|username, _| keep_set.contains(username));

        let removed = initial_count - records.len();

        self.save_records(&records)?;
        Ok(removed)
    }

    pub fn sync_with_friends(&self, current_friends: &[String]) -> Result<SyncStats> {
        let mut records = self.load_records()?;
        let initial_count = records.len();

        let friends_set: std::collections::HashSet<_> = current_friends.iter().collect();

        let mut removed = 0;
        records.retain(|username, _| {
            let keep = friends_set.contains(username);
            if !keep {
                removed += 1;
            }
            keep
        });

        let mut added = 0;
        for username in current_friends {
            if !records.contains_key(username) {
                records.insert(
                    username.clone(),
                    UserRecord {
                        username: username.clone(),
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

    pub fn get_next_pokeable_user(&self) -> Result<Option<NextPokeableUser>> {
        let records = self.load_records()?;
        let now = chrono::Utc::now().timestamp();
        let day_in_seconds = 24 * 60 * 60;

        let mut next_user: Option<NextPokeableUser> = None;
        let mut shortest_wait = i64::MAX;

        for record in records.values() {
            if let Some(last_poke) = record.last_poke {
                let time_since_poke = now - last_poke;
                let time_until_pokeable = day_in_seconds - time_since_poke;

                if time_until_pokeable > 0 && time_until_pokeable < shortest_wait {
                    shortest_wait = time_until_pokeable;
                    next_user = Some(NextPokeableUser {
                        username: record.username.clone(),
                        time_until_pokeable: time_until_pokeable as u64,
                    });
                }
            }
        }

        Ok(next_user)
    }
}
