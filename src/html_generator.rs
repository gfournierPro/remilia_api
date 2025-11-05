use crate::models::{LeaderboardStats, SortBy};

pub struct HtmlGenerator;

impl HtmlGenerator {
    pub fn generate_html(stats: &LeaderboardStats, sort_by: SortBy) -> String {
        let mut html = String::new();

        // HTML header
        html.push_str(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Remilia Leaderboard</title>
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css">
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            background: linear-gradient(135deg, #0f0f1e 0%, #1a1a2e 100%);
            color: #ffffff;
            padding: 2rem;
            min-height: 100vh;
        }
        
        .container {
            max-width: 1400px;
            margin: 0 auto;
        }
        
        h1 {
            text-align: center;
            font-size: 3rem;
            margin-bottom: 1rem;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
        }
        
        .stats-summary {
            text-align: center;
            color: #6e7787;
            margin-bottom: 2rem;
            font-size: 1.1rem;
        }
        
        .overflow-x-auto {
            overflow-x: auto;
            border: 1px solid #243147;
            border-radius: 0.5rem;
        }
        
        table {
            min-width: 100%;
            overflow: hidden;
            table-layout: fixed;
        }
        
        thead tr {
            border-bottom: 1px solid #20242b;
            background: linear-gradient(to bottom, #22252c 0%, #16171b 100%);
        }
        
        th {
            padding: 0.75rem 1.5rem;
            text-align: left;
            font-size: 0.75rem;
            font-weight: 500;
            color: #6e7787;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }
        
        th button {
            display: flex;
            align-items: center;
            gap: 0.5rem;
            color: #6e7787;
            background: none;
            border: none;
            cursor: pointer;
            transition: color 0.3s;
            font-family: inherit;
            font-size: inherit;
            font-weight: inherit;
            text-transform: inherit;
            letter-spacing: inherit;
        }
        
        th button:hover {
            color: white;
        }
        
        th button i {
            font-size: 0.75rem;
            vertical-align: middle;
        }
        
        th button i.active {
            color: white;
        }
        
        tbody {
            background: #16171b;
        }
        
        tbody tr {
            border-bottom: 1px solid #20242b;
            transition: background-color 0.2s;
        }
        
        tbody tr:hover {
            background-color: #1e1f24;
        }
        
        td {
            padding: 1rem 1.5rem;
            color: #e6edf3;
        }
        
        .rank {
            font-weight: 700;
            font-size: 1.1rem;
        }
        
        .rank.top-1 {
            color: #ffd700;
        }
        
        .rank.top-2 {
            color: #c0c0c0;
        }
        
        .rank.top-3 {
            color: #cd7f32;
        }
        
        .user-cell {
            display: flex;
            align-items: center;
            gap: 0.75rem;
        }
        
        .user-avatar {
            width: 40px;
            height: 40px;
            border-radius: 50%;
            object-fit: cover;
            border: 2px solid #243147;
        }
        
        .user-info {
            display: flex;
            flex-direction: column;
        }
        
        .user-name {
            font-weight: 600;
            color: #e6edf3;
        }
        
        .user-username {
            font-size: 0.875rem;
            color: #6e7787;
        }
        
        .stat-value {
            font-weight: 600;
            font-size: 1.05rem;
        }
        
        .empty-state {
            padding: 3rem;
            text-align: center;
            color: #6e7787;
        }
        
        .w-1-6 { width: 16.666%; }
        .w-1-3 { width: 33.333%; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🏆 Remilia Leaderboard</h1>
        <div class="stats-summary">
            Total Users: {total_users} | Sorted by: {sort_criteria}
        </div>
        <div class="overflow-x-auto">
            <table>
                <thead>
                    <tr>
                        <th class="w-1-6">
                            <button>
                                RANK
                                <i class="fa-regular fa-angle-down"></i>
                            </button>
                        </th>
                        <th class="w-1-3">
                            <button>
                                USER
                                <i class="fa-regular fa-angle-down"></i>
                            </button>
                        </th>
                        <th class="w-1-6">
                            <button onclick="sortTable('beetles')">
                                BEETLES
                                <i id="beetles-icon" class="fa-regular fa-angle-down{beetles_active}"></i>
                            </button>
                        </th>
                        <th class="w-1-6">
                            <button onclick="sortTable('pokes')">
                                POKES
                                <i id="pokes-icon" class="fa-regular fa-angle-down{pokes_active}"></i>
                            </button>
                        </th>
                        <th class="w-1-6">
                            <button onclick="sortTable('social')">
                                SOCIAL CREDIT
                                <i id="social-icon" class="fa-regular fa-angle-down{social_active}"></i>
                            </button>
                        </th>
                    </tr>
                </thead>
                <tbody id="leaderboard-body">
"#);

        // Determine active column
        let sort_criteria = match sort_by {
            SortBy::Beetles => "Beetles",
            SortBy::Pokes => "Pokes",
            SortBy::SocialCredit => "Social Credit",
        };

        let beetles_active = if matches!(sort_by, SortBy::Beetles) { " active" } else { "" };
        let pokes_active = if matches!(sort_by, SortBy::Pokes) { " active" } else { "" };
        let social_active = if matches!(sort_by, SortBy::SocialCredit) { " active" } else { "" };

        html = html.replace("{total_users}", &stats.total_users.to_string());
        html = html.replace("{sort_criteria}", sort_criteria);
        html = html.replace("{beetles_active}", beetles_active);
        html = html.replace("{pokes_active}", pokes_active);
        html = html.replace("{social_active}", social_active);

        // Table rows
        if stats.entries.is_empty() {
            html.push_str(r#"                    <tr>
                        <td colspan="5" class="empty-state">No users found</td>
                    </tr>
"#);
        } else {
            for (idx, entry) in stats.entries.iter().enumerate() {
                let rank = idx + 1;
                let rank_class = match rank {
                    1 => "rank top-1",
                    2 => "rank top-2",
                    3 => "rank top-3",
                    _ => "rank",
                };

                let avatar_url = entry.pfp_url.as_deref().unwrap_or("https://via.placeholder.com/40");

                html.push_str(&format!(
                    r#"                    <tr data-beetles="{beetles}" data-pokes="{pokes}" data-social="{social_credit}">
                        <td class="{rank_class}">{rank}</td>
                        <td>
                            <div class="user-cell">
                                <img src="{avatar}" alt="{username}" class="user-avatar">
                                <div class="user-info">
                                    <span class="user-name">{display_name}</span>
                                    <span class="user-username">@{username}</span>
                                </div>
                            </div>
                        </td>
                        <td class="stat-value">{beetles}</td>
                        <td class="stat-value">{pokes}</td>
                        <td class="stat-value">{social_credit}</td>
                    </tr>
"#,
                    rank_class = rank_class,
                    rank = rank,
                    avatar = avatar_url,
                    username = entry.username,
                    display_name = entry.display_name,
                    beetles = entry.beetles,
                    pokes = entry.pokes,
                    social_credit = entry.social_credit,
                ));
            }
        }

        // HTML footer
        html.push_str(r#"                </tbody>
            </table>
        </div>
    </div>
    
    <script>
        let currentSort = '';
        let ascending = false;
        
        function sortTable(column) {
            const tbody = document.getElementById('leaderboard-body');
            const rows = Array.from(tbody.querySelectorAll('tr'));
            
            // Toggle sort direction if clicking same column
            if (currentSort === column) {
                ascending = !ascending;
            } else {
                ascending = false;
                currentSort = column;
            }
            
            // Sort rows
            rows.sort((a, b) => {
                const aVal = parseInt(a.getAttribute('data-' + column));
                const bVal = parseInt(b.getAttribute('data-' + column));
                return ascending ? aVal - bVal : bVal - aVal;
            });
            
            // Update rank classes and numbers
            rows.forEach((row, idx) => {
                const rank = idx + 1;
                const rankCell = row.querySelector('td:first-child');
                
                // Update rank class
                if (rank === 1) {
                    rankCell.className = 'rank top-1';
                } else if (rank === 2) {
                    rankCell.className = 'rank top-2';
                } else if (rank === 3) {
                    rankCell.className = 'rank top-3';
                } else {
                    rankCell.className = 'rank';
                }
                
                // Update rank number
                rankCell.textContent = rank;
                
                // Reinsert row
                tbody.appendChild(row);
            });
            
            // Update icons
            ['beetles', 'pokes', 'social'].forEach(col => {
                const icon = document.getElementById(col + '-icon');
                if (col === column) {
                    icon.className = 'fa-regular fa-angle-' + (ascending ? 'up' : 'down') + ' active';
                } else {
                    icon.className = 'fa-regular fa-angle-down';
                }
            });
        }
    </script>
</body>
</html>
"#);

        html
    }

    pub fn save_to_file(html: &str, filename: &str) -> std::io::Result<()> {
        std::fs::write(filename, html)
    }
}
