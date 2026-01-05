use std::time::{SystemTime, UNIX_EPOCH};

/// Format duration in seconds to human-readable string
pub fn format_duration(seconds: u64) -> String {
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

/// Format timestamp as local time
pub fn format_timestamp(seconds_from_now: u64) -> String {
    use chrono::{Duration, Local};

    let now = Local::now();
    let target = now + Duration::seconds(seconds_from_now as i64);
    target.format("at %H:%M:%S").to_string()
}

/// Format time elapsed since timestamp
pub fn format_time_ago(timestamp_secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
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

/// Extract cooldown seconds from detailed error message
pub fn extract_cooldown_seconds(err_msg: &str) -> Option<i64> {
    // Error format: "Still on cooldown for X hours and Y minutes (Z seconds)"
    if let Some(start) = err_msg.find('(') {
        if let Some(end) = err_msg.find(" seconds)") {
            return err_msg[start + 1..end].parse().ok();
        }
    }
    None
}
