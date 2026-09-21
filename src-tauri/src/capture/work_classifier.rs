//! Work relevance classifier for dual-use platforms (YouTube, Reddit, Twitter/X, Twitch).
//!
//! Differentiates between productive learning/research and non-work leisure/entertainment
//! at the ingestion gate—BEFORE anything is saved to SQLite or emitted to the application.

use std::collections::BTreeSet;

/// Known dual-use domains and application names that mix professional learning
/// and casual entertainment.
const DUAL_USE_DOMAINS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "reddit.com",
    "twitter.com",
    "x.com",
    "twitch.tv",
    // Social media & short-form
    "instagram.com",
    "tiktok.com",
    "facebook.com",
    "threads.net",
    "pinterest.com",
    // Shopping & e-commerce
    "flipkart.com",
    "amazon.com",
    "amazon.in",
    "ebay.com",
    "myntra.com",
    "aliexpress.com",
    // Streaming entertainment
    "netflix.com",
    "hulu.com",
    "disneyplus.com",
    "hotstar.com",
    "primevideo.com",
];

const DUAL_USE_TITLE_INDICATORS: &[&str] = &[
    "youtube",
    "reddit",
    "twitter",
    " / x",
    "twitch",
    "instagram",
    "tiktok",
    "facebook",
    "pinterest",
    "flipkart",
    "amazon",
    "ebay",
    "myntra",
    "shopping",
    "netflix",
    "hulu",
    "disney+",
    "prime video",
];

/// Clear entertainment/leisure disqualifiers. If present on a dual-use platform,
/// the activity is immediately dropped from the storage layer.
const LEISURE_KEYWORDS: &[&str] = &[
    // Shorts & short-form video feeds
    "/shorts/",
    "#shorts",
    "/reels/",
    "reels",
    "/stories/",
    "stories",
    "explore",
    // E-commerce & Shopping
    "buy online",
    "buy now",
    "add to cart",
    "proceed to checkout",
    "shopping cart",
    "price in ",
    "best price",
    "best offers",
    "order summary",
    "in stock",
    "free delivery",
    "free shipping",
    "wishlist",
    "deals of the day",
    // Gaming & Let's plays
    "gameplay",
    "walkthrough",
    "let's play",
    "playthrough",
    "speedrun",
    "minecraft",
    "fortnite",
    "roblox",
    "gta ",
    "grand theft auto",
    "valorant",
    "league of legends",
    // Music & chill tracks
    "music video",
    "official audio",
    "official video",
    "official trailer",
    "teaser trailer",
    "lofi hip hop",
    "chillhop",
    "soundtrack",
    "full album",
    "remix",
    "lyrics",
    // Casual entertainment, vlogs & comedy
    "vlog",
    "prank",
    "reaction",
    "comedy",
    "standup",
    "meme",
    "tiktok compilation",
    "try not to laugh",
    // Sports & streaming highlights
    "highlights",
    "espn",
    "nba",
    "fifa",
    "cricket",
    "ufc",
    "wwe",
    // Entertainment subreddits
    "/r/funny",
    "/r/memes",
    "/r/gaming",
    "/r/aww",
    "/r/gifs",
    "/r/videos",
];

/// High-signal technical, engineering, and educational keywords that qualify
/// activity on dual-use platforms as productive work.
const TECHNICAL_AND_LEARNING_KEYWORDS: &[&str] = &[
    // Educational formats
    "tutorial",
    "crash course",
    "masterclass",
    "full course",
    "lecture",
    "deep dive",
    "system design",
    "architecture",
    "debugging",
    "rfc",
    "documentation",
    "explained",
    "case study",
    "code review",
    "conference",
    "keynote",
    // Programming languages
    "rust",
    "python",
    "typescript",
    "javascript",
    "golang",
    "c++",
    "c#",
    "java",
    "swift",
    "kotlin",
    "sql",
    "bash",
    "zig",
    // Frameworks & dev tools
    "tauri",
    "react",
    "vue",
    "angular",
    "nextjs",
    "docker",
    "kubernetes",
    "aws",
    "gcp",
    "azure",
    "postgres",
    "sqlite",
    "redis",
    "mongodb",
    "git",
    "github",
    "linux",
    "kernel",
    "neovim",
    "vscode",
    "cursor",
    "vite",
    "cargo",
    "pnpm",
    "npm",
    // Core Computer Science, Engineering & AI
    "algorithm",
    "data structure",
    "distributed system",
    "machine learning",
    "deep learning",
    "neural network",
    "llm",
    "transformer",
    "compiler",
    "concurrency",
    "async",
    "multithreading",
    "networking",
    "security",
    "cryptography",
    "backend",
    "frontend",
    "fullstack",
    "devops",
];

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "into", "over",
    "your", "what", "when", "where", "how", "who", "all", "any", "app",
];

/// Checks if an active window belongs to a dual-use platform.
pub fn is_dual_use_platform(app_name: &str, window_title: &str, url: Option<&str>) -> bool {
    let app_lower = app_name.to_lowercase();
    let title_lower = window_title.to_lowercase();

    // Direct app name match
    if DUAL_USE_TITLE_INDICATORS.iter().any(|ind| app_lower.contains(ind)) {
        return true;
    }

    // Window title match (e.g. browser tabs "Something - YouTube - Chrome")
    if DUAL_USE_TITLE_INDICATORS.iter().any(|ind| title_lower.contains(ind)) {
        return true;
    }

    // URL host match
    if let Some(u) = url {
        let u_lower = u.to_lowercase();
        if DUAL_USE_DOMAINS.iter().any(|domain| u_lower.contains(domain)) {
            return true;
        }
    }

    false
}

/// Checks if the window title or URL contains explicit leisure or entertainment signatures.
pub fn is_leisure_signature(window_title: &str, url: Option<&str>) -> bool {
    let title_lower = window_title.to_lowercase();
    let url_lower = url.map(|u| u.to_lowercase()).unwrap_or_default();

    for keyword in LEISURE_KEYWORDS {
        if title_lower.contains(keyword) || url_lower.contains(keyword) {
            return true;
        }
    }
    false
}

/// Checks if the window title contains technical, engineering, or educational learning signals.
pub fn is_educational_or_technical(window_title: &str) -> bool {
    let title_lower = window_title.to_lowercase();
    let tokens: BTreeSet<&str> = title_lower
        .split(|c: char| !c.is_alphanumeric() && c != '#' && c != '+')
        .filter(|t| !t.is_empty())
        .collect();

    for keyword in TECHNICAL_AND_LEARNING_KEYWORDS {
        if keyword.contains(' ') {
            if title_lower.contains(keyword) {
                return true;
            }
        } else if tokens.contains(keyword) {
            return true;
        }
    }
    false
}

/// Checks if the window title has meaningful semantic keyword overlap with
/// the user's active task or known project registry.
pub fn matches_task_context(
    window_title: &str,
    task_title: &str,
    task_description: Option<&str>,
    known_projects: &[String],
) -> bool {
    let title_lower = window_title.to_lowercase();
    let window_tokens: BTreeSet<String> = title_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(t))
        .map(ToString::to_string)
        .collect();

    if window_tokens.is_empty() {
        return false;
    }

    // 1. Check known project names
    for project in known_projects {
        let p_lower = project.to_lowercase();
        if !p_lower.is_empty() && title_lower.contains(&p_lower) {
            return true;
        }
    }

    // 2. Check active task title tokens
    let task_title_lower = task_title.to_lowercase();
    for token in task_title_lower.split(|c: char| !c.is_alphanumeric()) {
        if token.len() >= 3 && !STOPWORDS.contains(&token) && window_tokens.contains(token) {
            return true;
        }
    }

    // 3. Check active task description tokens
    if let Some(desc) = task_description {
        let desc_lower = desc.to_lowercase();
        for token in desc_lower.split(|c: char| !c.is_alphanumeric()) {
            if token.len() >= 4 && !STOPWORDS.contains(&token) && window_tokens.contains(token) {
                return true;
            }
        }
    }

    false
}

/// Master ingestion gate decision. Returns `true` if the activity should be captured
/// and stored in SQLite; returns `false` if it is non-productive leisure that must
/// be excluded at the storage layer.
pub fn should_capture_activity(
    app_name: &str,
    window_title: &str,
    url: Option<&str>,
    task_title: Option<&str>,
    task_description: Option<&str>,
    known_projects: &[String],
) -> bool {
    // Regular developer tools and non-dual-use native apps are handled by standard privacy rules
    if !is_dual_use_platform(app_name, window_title, url) {
        return true;
    }

    // Tier 1: Instant leisure disqualifier
    if is_leisure_signature(window_title, url) {
        return false;
    }

    // Tier 2: Check alignment with active task and known projects
    if let Some(title) = task_title {
        if matches_task_context(window_title, title, task_description, known_projects) {
            return true;
        }
    }

    // Tier 3: Check for general educational / technical learning indicators
    if is_educational_or_technical(window_title) {
        return true;
    }

    // Dual-use activity with no work alignment or learning indicators is dropped
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_regular_dev_tools_immediately() {
        assert!(should_capture_activity(
            "Cursor",
            "main.rs — TaskFlow — Cursor",
            None,
            None,
            None,
            &[]
        ));
        assert!(should_capture_activity(
            "Alacritty",
            "cargo test --lib",
            None,
            None,
            None,
            &[]
        ));
        assert!(should_capture_activity(
            "Google Chrome",
            "docs.rs/sqlx/latest/sqlx - Google Chrome",
            Some("https://docs.rs/sqlx/latest/sqlx/"),
            None,
            None,
            &[]
        ));
    }

    #[test]
    fn drops_instant_leisure_on_youtube() {
        // YouTube Shorts
        assert!(!should_capture_activity(
            "Google Chrome",
            "Funny Dog Tricks #shorts - YouTube - Google Chrome",
            Some("https://www.youtube.com/shorts/abc123xyz"),
            None,
            None,
            &[]
        ));

        // Gaming gameplay
        assert!(!should_capture_activity(
            "Google Chrome",
            "GTA 6 Gameplay Walkthrough Part 1 - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=123"),
            None,
            None,
            &[]
        ));

        // Music & Lofi
        assert!(!should_capture_activity(
            "Google Chrome",
            "Lofi Hip Hop Radio - Beats to relax/study to - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=lofi"),
            None,
            None,
            &[]
        ));

        // Entertainment movie trailer
        assert!(!should_capture_activity(
            "Google Chrome",
            "Inception Official Trailer - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=movie"),
            None,
            None,
            &[]
        ));
    }

    #[test]
    fn allows_technical_learning_on_youtube() {
        // Rust tutorial
        assert!(should_capture_activity(
            "Google Chrome",
            "Async Rust Deep Dive & Tokio Runtime - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=rust101"),
            None,
            None,
            &[]
        ));

        // React full course
        assert!(should_capture_activity(
            "Google Chrome",
            "React 19 Tutorial: Full Course for Beginners - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=react19"),
            None,
            None,
            &[]
        ));

        // MIT Systems Lecture
        assert!(should_capture_activity(
            "Google Chrome",
            "MIT 6.824 Distributed Systems Lecture 1 - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=mit6824"),
            None,
            None,
            &[]
        ));
    }

    #[test]
    fn allows_task_aligned_activity_on_dual_use_platforms() {
        let task_title = "Building TaskFlow UI Animation System";
        let task_desc = Some("Integrating Framer Motion in Vite desktop frontend");
        let known_projects = vec!["TaskFlow".to_string()];

        // YouTube video matching specific task keywords
        assert!(should_capture_activity(
            "Google Chrome",
            "Framer Motion layout animations masterclass - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=framer"),
            Some(task_title),
            task_desc,
            &known_projects
        ));

        // Reddit thread matching project name
        assert!(should_capture_activity(
            "Google Chrome",
            "r/tauri - Multi-window architecture best practices - Reddit - Google Chrome",
            Some("https://www.reddit.com/r/tauri/comments/123/"),
            Some(task_title),
            task_desc,
            &known_projects
        ));
    }

    #[test]
    fn drops_unrelated_casual_youtube_without_task_or_tech_signals() {
        assert!(!should_capture_activity(
            "Google Chrome",
            "MrBeast $1,000,000 Laser Tag Challenge - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=mrbeast"),
            None,
            None,
            &[]
        ));

        assert!(!should_capture_activity(
            "Google Chrome",
            "Top 10 Most Expensive Houses In The World - YouTube - Google Chrome",
            Some("https://www.youtube.com/watch?v=houses"),
            None,
            None,
            &[]
        ));
    }
}
