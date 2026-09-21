//! App-agnostic content classification engine (Tier 2: Work-DNA & Signal Density Gate).
//!
//! Differentiates between productive work content (code, technical documentation,
//! developer discussions, terminal output) and unwanted leisure/social noise
//! (social media feeds, entertainment streams, casual commerce) based on structural
//! and lexical text properties rather than hardcoded app lists.

use super::types::ContentType;
use std::collections::BTreeSet;

/// Code syntax keywords and language constructs.
const CODE_KEYWORDS: &[&str] = &[
    "fn ", "def ", "function ", "func ", "class ", "struct ", "enum ", "interface ",
    "impl ", "trait ", "type ", "let ", "const ", "var ", "mut ", "pub ", "public ",
    "private ", "protected ", "return ", "import ", "export ", "from ", "require(",
    "async ", "await ", "yield ", "try ", "catch ", "finally ", "throw ", "throws ",
    "package ", "namespace ", "using ", "include ", "#include", "extern ", "unsafe ",
    "select ", "insert into", "update ", "delete from", "where ", "group by",
];

/// Known file extensions indicative of source code, configuration, or documentation.
const CODE_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".c", ".cpp", ".h", ".hpp",
    ".java", ".kt", ".swift", ".cs", ".rb", ".php", ".sh", ".bash", ".zsh", ".sql",
    ".json", ".toml", ".yaml", ".yml", ".md", ".html", ".css", ".scss", ".wasm",
    ".proto", ".graphql", ".dockerfile",
];

/// Common code directory and path fragments.
const CODE_PATH_FRAGMENTS: &[&str] = &[
    "src/", "lib/", "tests/", "test/", "components/", "utils/", "models/", "services/",
    "controllers/", "routes/", "api/", "config/", "migrations/", "dist/", "target/",
    "node_modules/", "vendor/", "pkg/", "internal/",
];

/// CLI tools and shell command prompts.
const SHELL_COMMANDS: &[&str] = &[
    "cargo ", "npm ", "pnpm ", "yarn ", "npx ", "pip ", "python ", "git ", "docker ",
    "kubectl ", "curl ", "ssh ", "grep ", "systemctl ", "psql ", "sqlite3 ", "brew ",
    "apt ", "go run", "go test", "pytest", "make ", "cmake ",
];

/// Technical and computer science vocabulary.
const TECHNICAL_TERMS: &[&str] = &[
    "algorithm", "architecture", "argument", "authenticate", "authorization",
    "backend", "bandwidth", "benchmark", "binary", "buffer", "build", "cache",
    "callback", "cli", "client", "cluster", "commit", "compiler", "concurrency",
    "configuration", "database", "deadlock", "debug", "debugging", "dependency",
    "deployment", "deserialization", "diff", "distributed", "docker", "endpoint",
    "entity", "error", "exception", "framework", "frontend", "gateway", "handler",
    "hash", "header", "http", "https", "index", "infrastructure", "ingress",
    "iterator", "jwt", "kubernetes", "latency", "library", "lifecycle", "lint",
    "logging", "memory", "metadata", "middleware", "migration", "mutex", "mutation",
    "network", "oauth", "optimize", "parameter", "parser", "patch", "payload",
    "performance", "pipeline", "pointer", "polymorphism", "postgres", "pr",
    "protocol", "pull request", "query", "queue", "refactor", "regression",
    "repository", "request", "resolver", "response", "rfc", "router", "schema",
    "sdk", "serialization", "server", "socket", "sqlite", "stack trace", "stacktrace",
    "syntax", "tcp", "terminal", "test", "thread", "timeout", "token", "traceback",
    "transaction", "typed", "udp", "variable", "webhook", "websocket",
];

/// Social media interaction, engagement verbs, and feed UI artifacts.
const SOCIAL_AND_LEISURE_PATTERNS: &[&str] = &[
    // Social feed controls
    "like", "likes", "liked", "share", "shares", "shared", "comment", "comments",
    "commented", "follow", "following", "followers", "subscribe", "subscribed",
    "subscribers", "retweet", "retweets", "repost", "upvote", "upvotes", "downvote",
    "downvotes", "feed", "foryou", "fyp", "reels", "shorts", "explore", "trending",
    "direct message", "direct messages", "messages", "dm", "dms", "suggested for you",
    "add a comment", "see all", "view all comments", "posted by", "photo by",
    "video by", "stories", "story", "status update", "notification", "notifications",
    // Media & video playback controls
    "play", "pause", "mute", "unmute", "volume", "fullscreen", "autoplay",
    "playback speed", "subtitles", "captions", "skip ad", "advertisement",
    // Casual shopping / commerce
    "add to cart", "buy now", "proceed to checkout", "item(s)", "in stock",
    "free shipping", "free delivery", "order summary", "promo code", "wishlist",
    "customer reviews", "ratings & reviews", "price:", "price in ", "buy online",
    "shopping cart", "best price", "best offers", "deals of the day", "flipkart",
    "amazon", "ebay", "myntra", "delivery by", "bank offers", "special price",
    // Casual gaming
    "gameplay", "walkthrough", "speedrun", "high score", "leaderboard", "play game",
];

/// Structured metrics for content signal evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalScore {
    pub code_syntax_count: usize,
    pub technical_term_count: usize,
    pub path_or_extension_count: usize,
    pub identifier_count: usize,
    pub leisure_noise_count: usize,
    pub work_weight: f32,
    pub noise_weight: f32,
}

impl SignalScore {
    /// Returns true if the work signal convincingly outweighs noise.
    pub fn is_work_dominant(&self) -> bool {
        if self.noise_weight >= 6.0 && self.noise_weight > self.work_weight * 1.5 {
            return false;
        }
        self.work_weight >= 3.0 && self.work_weight >= self.noise_weight
    }
}

/// Evaluates raw text and returns a breakdown of work signals versus leisure noise.
pub fn compute_signal_score(text: &str) -> SignalScore {
    let lower = text.to_lowercase();
    let tokens: BTreeSet<String> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.')
        .filter(|t| !t.is_empty())
        .map(ToString::to_string)
        .collect();

    // 1. Code syntax & operators
    let mut code_syntax_count = 0;
    for kw in CODE_KEYWORDS {
        if lower.contains(kw) {
            code_syntax_count += 1;
        }
    }
    for shell in SHELL_COMMANDS {
        if lower.contains(shell) {
            code_syntax_count += 1;
        }
    }
    // Check syntax operators: braces, arrow, double colon, strict equals
    for op in &["{", "}", "=>", "->", "::", "!=", "==", "===", "&&", "||", "/>", "</"] {
        if text.contains(op) {
            code_syntax_count += 1;
        }
    }

    // 2. Paths and file extensions
    let mut path_or_extension_count = 0;
    for ext in CODE_EXTENSIONS {
        if lower.contains(ext) {
            path_or_extension_count += 1;
        }
    }
    for frag in CODE_PATH_FRAGMENTS {
        if lower.contains(frag) {
            path_or_extension_count += 1;
        }
    }

    // 3. Technical vocabulary
    let mut technical_term_count = 0;
    for term in TECHNICAL_TERMS {
        if term.contains(' ') {
            if lower.contains(term) {
                technical_term_count += 1;
            }
        } else if tokens.contains(*term) {
            technical_term_count += 1;
        }
    }

    // 4. Identifiers: camelCase, PascalCase, snake_case, or SCREAMING_SNAKE
    let mut identifier_count = 0;
    for word in text.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if is_code_identifier(trimmed) {
            identifier_count += 1;
        }
    }
    // Cap identifier count to avoid runaway on raw symbol dumps
    identifier_count = identifier_count.min(25);

    // 5. Leisure / Social noise
    let mut leisure_noise_count = 0;
    for pattern in SOCIAL_AND_LEISURE_PATTERNS {
        if pattern.contains(' ') {
            if lower.contains(pattern) {
                leisure_noise_count += 1;
            }
        } else if tokens.contains(*pattern) {
            leisure_noise_count += 1;
        }
    }

    let work_weight = (code_syntax_count as f32 * 2.0)
        + (path_or_extension_count as f32 * 2.0)
        + (technical_term_count as f32 * 1.5)
        + (identifier_count as f32 * 0.5);

    let noise_weight = leisure_noise_count as f32 * 2.0;

    SignalScore {
        code_syntax_count,
        technical_term_count,
        path_or_extension_count,
        identifier_count,
        leisure_noise_count,
        work_weight,
        noise_weight,
    }
}

/// Checks whether a token represents a genuine programming identifier
/// (camelCase, PascalCase, snake_case, SCREAMING_SNAKE) rather than
/// an ordinary capitalized English word (e.g. "Highlights", "Shopping").
fn is_code_identifier(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 3 || chars.len() > 60 {
        return false;
    }
    if !chars.iter().all(|c| c.is_alphanumeric() || *c == '_') {
        return false;
    }

    // snake_case / SCREAMING_SNAKE: contains underscore in middle with alphabetic characters
    if token.contains('_') && !token.starts_with('_') && !token.ends_with('_') {
        return chars.iter().any(|c| c.is_alphabetic());
    }

    // camelCase: starts with lowercase, contains at least one uppercase letter (e.g. onClick, useState)
    if chars[0].is_lowercase() && chars.iter().skip(1).any(|c| c.is_uppercase()) {
        return true;
    }

    // PascalCase: starts with uppercase, has lowercase, and has another uppercase letter later (e.g. SqlitePool, AppError)
    if chars[0].is_uppercase() {
        let mut transitions = 0;
        let mut prev_is_lower = false;
        for c in &chars[1..] {
            if c.is_uppercase() && prev_is_lower {
                transitions += 1;
            }
            prev_is_lower = c.is_lowercase();
        }
        if transitions >= 1 {
            return true;
        }
    }

    false
}

/// Determines if captured text contains sufficient work signals to warrant retention.
///
/// - High-trust applications (Code editors, Terminals) pass with standard content checks.
/// - Browsers and generic applications must pass the work-DNA signal density test,
///   preventing social media feeds, casual streaming, and shopping catalogs from
///   polluting SQLite or slowing down the LLM summarization pipeline.
pub fn has_work_signal(
    text: &str,
    app_name: &str,
    window_title: &str,
    content_type: &ContentType,
    url: Option<&str>,
) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < 25 {
        return false;
    }

    // Editors and Terminals are inherently work contexts
    match content_type {
        ContentType::CodeContent | ContentType::TerminalContent => {
            return true;
        }
        _ => {}
    }

    let app_lower = app_name.to_lowercase();
    if app_lower.contains("code")
        || app_lower.contains("cursor")
        || app_lower.contains("neovim")
        || app_lower.contains("vim")
        || app_lower.contains("terminal")
        || app_lower.contains("alacritty")
        || app_lower.contains("ghostty")
        || app_lower.contains("iterm")
    {
        return true;
    }

    let title_lower = window_title.to_lowercase();

    // Instant leisure / shopping disqualifiers in window title
    for kw in &[
        "flipkart", "amazon.", "ebay.", "myntra", "shopping", "cart",
        "buy now", "buy online", "price in ", "checkout", "wishlist",
        "instagram", "tiktok", "netflix", "hotstar", "prime video",
    ] {
        if title_lower.contains(kw) {
            return false;
        }
    }

    // Check URL priors if available
    if let Some(u) = url {
        let u_lower = u.to_lowercase();
        if u_lower.contains("flipkart.com")
            || u_lower.contains("amazon.")
            || u_lower.contains("ebay.")
            || u_lower.contains("myntra.com")
            || u_lower.contains("aliexpress.com")
            || u_lower.contains("instagram.com")
            || u_lower.contains("tiktok.com")
            || u_lower.contains("facebook.com")
            || u_lower.contains("netflix.com")
            || u_lower.contains("hulu.com")
            || u_lower.contains("disneyplus.com")
            || u_lower.contains("primevideo.com")
        {
            return false;
        }

        // Clear developer documentation / repo domains pass easily
        if u_lower.contains("github.com")
            || u_lower.contains("gitlab.com")
            || u_lower.contains("stackoverflow.com")
            || u_lower.contains("developer.mozilla.org")
            || u_lower.contains("docs.rs")
            || u_lower.contains("crates.io")
            || u_lower.contains("localhost")
            || u_lower.contains("127.0.0.1")
        {
            return true;
        }
    }

    // Compute signal score on the extracted text
    let score = compute_signal_score(trimmed);

    // If noise heavily outnumbers work signals, drop it immediately
    if score.noise_weight >= 6.0 && score.noise_weight > score.work_weight {
        return false;
    }

    // Retain if work signals dominate or meet minimum threshold
    score.is_work_dominant()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_rust_code_snippet() {
        let code = r#"
            pub fn run_server(port: u16) -> Result<(), AppError> {
                let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;
                println!("Listening on port {}", port);
                Ok(())
            }
        "#;
        assert!(has_work_signal(code, "Chrome", "Server.rs - Code", &ContentType::BrowserContent, None));
    }

    #[test]
    fn allows_github_pr_discussion() {
        let text = r#"
            Fix connection pool leak in SQLite migration #142
            Merged into main from fix/pool-leak
            Files changed: 3 (+45, -12)
            src/database/events.rs
            Reviewed by alice: Looks good to me, passes CI and cargo test.
        "#;
        assert!(has_work_signal(text, "Google Chrome", "Pull Request #142 · Kaushik4141/TaskFlow", &ContentType::BrowserContent, None));
    }

    #[test]
    fn allows_fastapi_documentation() {
        let text = r#"
            How to configure CORS in FastAPI?
            from fastapi.middleware.cors import CORSMiddleware
            app.add_middleware(
                CORSMiddleware,
                allow_origins=["*"],
                allow_methods=["*"],
            )
            Response status code 200 OK with endpoint schema definition.
        "#;
        assert!(has_work_signal(text, "Brave", "CORS (Cross-Origin Resource Sharing) - FastAPI", &ContentType::BrowserContent, None));
    }

    #[test]
    fn drops_instagram_feed_and_reels() {
        let instagram_text = r#"
            Instagram
            Search
            Explore
            Reels
            Messages
            Notifications
            Create
            Profile
            Suggested for you
            See all
            johndoe Follow
            Liked by user1 and 42 others
            Add a comment... Post
            Share to Direct
        "#;
        assert!(!has_work_signal(instagram_text, "Google Chrome", "Instagram", &ContentType::BrowserContent, None));
    }

    #[test]
    fn drops_shopping_cart_checkout() {
        let shopping_text = r#"
            Shopping Cart
            1 item selected
            Price: $49.99
            In Stock
            Eligible for FREE Shipping
            Proceed to checkout
            Qty: 1
            Delete | Save for later
            Customers who bought this also bought...
        "#;
        assert!(!has_work_signal(shopping_text, "Microsoft Edge", "Shopping Cart", &ContentType::BrowserContent, None));
    }

    #[test]
    fn drops_flipkart_earbuds_page() {
        let title = "Vaku Luxos SONAIR TWS Bluetooth 5.4 Earbuds 60H Playtime Deep Bass Touch Controls Bluetooth Price in India - Buy Vaku Luxos SONAIR TWS Bluetooth 5.4 Earbuds 60H Playtime Deep Bass Touch Controls Bluetooth Online - Vaku Luxos : Flipkart.com - Brave";
        let content = "60H Playtime Deep Bass\nKey Highlights\nKey Highlights\nSONAIR TWS Bluetooth 5.4 Earbuds 60H Playtime Deep Bass Touch Controls Bluetooth Headset";
        let url = Some("https://www.flipkart.com/vaku-luxos-sonair-tws-bluetooth-5-4-earbuds-60h-playtime-deep-bass-touch-controls/p/itm5ece30e8c53ab");
        assert!(!has_work_signal(content, "brave.exe", title, &ContentType::BrowserContent, url));
    }

    #[test]
    fn drops_youtube_entertainment_comments() {
        let yt_comments = r#"
            Official Music Video - 4K Remastered
            Play
            Pause
            Mute
            Volume
            Subscribe to my channel for more music videos!
            1.2M views • 2 days ago
            Top comments (4,512)
            Like this comment if you love this song!
            Share
            Add a comment...
        "#;
        assert!(!has_work_signal(yt_comments, "Firefox", "Music Video - YouTube", &ContentType::BrowserContent, None));
    }

    #[test]
    fn always_allows_editors_and_terminals() {
        let simple_text = "workspace setup notes\njust starting the day";
        assert!(has_work_signal(simple_text, "Code", "notes.md", &ContentType::CodeContent, None));
        assert!(has_work_signal(simple_text, "Alacritty", "terminal", &ContentType::TerminalContent, None));
    }
}
