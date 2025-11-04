use serde::{Deserialize, Serialize};

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
    #[serde(rename = "currentUsername")]
    pub current_username: Option<String>,
    #[serde(rename = "extraContext")]
    pub extra_context: Option<ExtraContext>,
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
    pub color: u32,
    pub location: Option<String>,
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

/// Social credit information
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialCredit {
    pub score: u32,
    pub last_calculated: String,
    pub components: SocialCreditComponents,
}

/// Social credit components
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialCreditComponents {
    pub base: u32,
    pub onboarding: u32,
    pub aggregate_bonus: u32,
    pub aggregate_scores: AggregateScores,
    pub friend_bonus: u32,
    #[serde(rename = "final")]
    pub final_score: u32,
}

/// Aggregate scores
#[derive(Debug, Deserialize)]
pub struct AggregateScores {
    pub miladychan: u32,
    pub twitter: u32,
    pub profiles: u32,
    pub beetle_game: u32,
    pub miladycraft: u32,
    pub ethereum: u32,
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
