use serde::{Deserialize, Serialize};

// Helper module for deserializing string or integer as u32
mod string_or_int {
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrInt {
            String(String),
            Int(i32), // Changed to i32 to handle negative numbers
            Array(Vec<serde_json::Value>), // Handle empty arrays
        }

        match StringOrInt::deserialize(deserializer) {
            Ok(StringOrInt::String(s)) => {
                s.parse::<u32>()
                    .map(Some)
                    .map_err(serde::de::Error::custom)
            }
            Ok(StringOrInt::Int(i)) => {
                if i >= 0 {
                    Ok(Some(i as u32))
                } else {
                    // Treat negative numbers as None or default to 0
                    Ok(Some(0))
                }
            }
            Ok(StringOrInt::Array(_)) => {
                // Empty arrays or any array treated as None/default
                Ok(Some(0))
            }
            Err(_) => Ok(None), // If deserialization fails, return None
        }
    }
}

/// Profile pfp information
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Pfp {
    pub project: String,
    #[serde(default)]
    pub id: Option<String>,
}

/// Profile pfp for updates (requires id)
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfilePfp {
    pub project: String,
    pub id: String,
}

/// Profile response from the API
#[derive(Debug, Deserialize, Serialize)]
pub struct ProfileResponse {
    pub user: UserProfile,
    #[serde(rename = "isAuthenticated")]
    pub is_authenticated: bool,
    #[serde(rename = "isOwnProfile")]
    pub is_own_profile: bool,
    #[serde(rename = "currentUsername", default)]
    pub current_username: Option<String>,
    #[serde(rename = "extraContext", default)]
    pub extra_context: Option<ExtraContext>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialCredit {
    pub score: i64,
    pub last_calculated: Option<String>,
}

/// User profile information
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
    #[serde(default, deserialize_with = "string_or_int::deserialize")]
    pub color: Option<u32>,
    pub location: Option<String>,
    #[serde(rename = "socialCredit")]
    pub social_credit: SocialCredit,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

/// Extra context when viewing other profiles
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
    #[serde(flatten)]
    pub other: serde_json::Value,
}

/// Friend information
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Friend {
    pub display_username: String,
    pub display_name: String,
    #[serde(default)]
    pub pfp: Option<Pfp>,
    #[serde(default)]
    pub pfp_url: Option<String>,
}

/// Friends list response
#[derive(Debug, Deserialize)]
pub struct FriendsListResponse {
    pub page: u32,
    pub limit: u32,
    pub friends: Vec<Friend>,
    pub total: u32,
}

/// Achievement information
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    pub id: u32,
    pub granted_at: String,
    pub _id: String,
    pub title: String,
    pub description: String,
    pub grant_message: String,
    #[serde(rename = "type")]
    pub achievement_type: String,
    pub trigger: String,
    pub season: u32,
}

/// Achievement context
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementContext {
    pub viewer_owns: bool,
    pub ownership_percentage: u32,
    pub season: u32,
}

/// Twitter mutuals information
#[derive(Debug, Deserialize)]
pub struct TwitterMutuals {
    pub display: Vec<String>,
    pub count: u32,
}

/// Request to update theme
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

/// Response from theme update
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateThemeResponse {
    pub success: bool,
}

/// Poke request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PokeRequest {
    pub poke_username: String,
}

/// Poke response
#[derive(Debug, Deserialize)]
pub struct PokeResponse {
    pub success: bool,
}

/// Friend request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendsRequest {
    pub friend_username: String,
}

/// Friend request response
#[derive(Debug, Deserialize)]
pub struct FriendsResponse {
    pub success: bool,
}
