//! Deterministic feature extraction for TaskFlow-TinyNet v1.
//!
//! Every constant, normalization rule, FNV-1a 64-bit hash, and structural signal
//! mirrors `training/classifier/schema.py` byte-for-byte so PyTorch training and
//! pure-Rust inference produce identical feature vectors.

use std::collections::HashSet;

use url::Url;

pub const EMBED_DIM: usize = 96;
pub const APP_DIM: usize = 64;
pub const CTX_HADAMARD_DIM: usize = 40;
pub const CTX_SCALAR_DIM: usize = 8;
pub const CTX_FEAT_DIM: usize = CTX_SCALAR_DIM + CTX_HADAMARD_DIM; // 48
pub const STRUCT_FEAT_DIM: usize = 24;
pub const INPUT_DIM: usize = APP_DIM + EMBED_DIM + EMBED_DIM + CTX_FEAT_DIM + STRUCT_FEAT_DIM; // 328
pub const HIDDEN_DIM: usize = 128;
pub const NUM_CATEGORIES: usize = 16;
pub const HASH_BUCKETS: usize = 12_288;

const FNV_OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01B3;

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "into", "over",
    "your", "what", "when", "where", "how", "who", "all", "any", "are",
    "was", "were", "have", "has", "had", "not", "but", "you", "our",
    "out", "new", "tab", "window", "page", "home", "app", "com", "www",
    "http", "https", "org", "net",
];

const BROWSER_APPS: &[&str] = &[
    "chrome", "chromium", "firefox", "edge", "msedge", "brave", "arc",
    "safari", "opera", "vivaldi", "zen", "helium", "librewolf", "waterfox",
];

const IDE_APPS: &[&str] = &[
    "cursor", "code", "vscode", "vscodium", "intellij", "idea", "pycharm",
    "webstorm", "goland", "clion", "rider", "rustrover", "datagrip",
    "neovim", "nvim", "vim", "helix", "zed", "sublime", "xcode",
    "android studio", "eclipse", "emacs", "windsurf",
];

const TERMINAL_APPS: &[&str] = &[
    "alacritty", "kitty", "wezterm", "ghostty", "foot", "warp", "iterm",
    "terminal", "windowsterminal", "wt", "powershell", "pwsh", "cmd",
    "konsole", "gnome-terminal", "hyper", "tmux",
];

const DUAL_USE_HOSTS: &[&str] = &[
    "youtube.com", "youtu.be", "reddit.com", "twitter.com", "x.com",
    "twitch.tv", "discord.com", "discord", "instagram.com", "tiktok.com",
    "facebook.com", "threads.net", "pinterest.com", "amazon.com",
    "amazon.in", "flipkart.com", "ebay.com", "myntra.com", "medium.com",
    "substack.com", "netflix.com", "primevideo.com", "hotstar.com",
    "disneyplus.com", "hulu.com", "spotify.com", "youtube", "reddit",
    " / x", "twitch",
];

const CODE_SYNTAX_MARKERS: &[&str] = &[
    "fn ", "def ", "function ", "func ", "class ", "struct ", "enum ",
    "interface ", "impl ", "trait ", "const ", "let ", "mut ", "pub ",
    "return ", "import ", "export ", "async ", "await ", "select ",
    "insert into", "create table", "=>", "->", "::", "!=", "===", "&&", "||",
    "/>", "</", "{", "}", "#include",
];

const CODE_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".c", ".cpp",
    ".h", ".hpp", ".java", ".kt", ".swift", ".cs", ".rb", ".php", ".sh",
    ".sql", ".json", ".toml", ".yaml", ".yml", ".html", ".css", ".scss",
    ".proto", ".graphql", ".tf", ".dockerfile", "src/", "lib/", "tests/",
    "components/", "migrations/", "pkg/",
];

const SHELL_CMD_MARKERS: &[&str] = &[
    "cargo ", "npm ", "pnpm ", "yarn ", "npx ", "pip ", "python ",
    "python3 ", "git ", "docker ", "kubectl ", "curl ", "ssh ", "grep ",
    "rg ", "systemctl ", "psql ", "sqlite3 ", "go test", "go run",
    "pytest", "make ", "cmake ", "terraform ", "helm ", "uv ", "bun ",
];

const LEARNING_MARKERS: &[&str] = &[
    "tutorial", "course", "full course", "crash course", "lecture",
    "lesson", "module", "syllabus", "masterclass", "bootcamp", "workshop",
    "explained", "deep dive", "walkthrough", "study", "flashcard", "quiz",
    "exam", "homework", "assignment", "coursera", "udemy", "edx",
    "khan academy", "leetcode", "hackerrank", "kaggle", "anki", "duolingo",
    "mit ", "stanford ", "cs50", "3blue1brown", "freecodecamp",
];

const RESEARCH_DOC_MARKERS: &[&str] = &[
    "documentation", "docs", "api reference", "reference", "manual",
    "specification", "rfc ", "arxiv", "pubmed", "scholar", "doi:",
    "abstract", "methodology", "citation", "paper", "whitepaper",
    "stackoverflow", "stackexchange", "developer.mozilla", "mdn",
    "docs.rs", "wikipedia", "perplexity", "claude", "chatgpt", "gemini",
    "benchmark", "case study",
];

const DESIGN_MEDIA_MARKERS: &[&str] = &[
    "figma", "sketch", "blender", "photoshop", "illustrator", "indesign",
    "after effects", "premiere", "davinci", "resolve", "final cut",
    "audacity", "ableton", "logic pro", "obs studio", "descript",
    "canva", "miro", "excalidraw", "autocad", "solidworks", "kicad",
    "wireframe", "prototype", "artboard", "keyframe", "color grade",
    "timeline", "render", "layer", "vector", "typography", "youtube studio",
];

const PLANNING_MARKERS: &[&str] = &[
    "sprint", "backlog", "roadmap", "milestone", "kanban", "epic",
    "ticket", "issue #", "issues", "pull request", "jira", "linear",
    "asana", "trello", "clickup", "monday.com", "calendar", "schedule",
    "standup", "retrospective", "okr", "quarterly", "project plan",
    "todo", "board",
];

const MEETING_MARKERS: &[&str] = &[
    "zoom meeting", "zoom workplace", "meet.google", "google meet",
    "teams meeting", "microsoft teams", "huddle", "webex", "whereby",
    "loom", "participants", "screen share", "unmute", "leave call",
    "join meeting", "1:1", "sync call", "all-hands", "interview",
    "demo call", "webinar",
];

const COMM_MARKERS: &[&str] = &[
    "gmail", "outlook", "thunderbird", "proton mail", "inbox", "unread",
    "compose", "reply", "subject:", "sent mail", "slack", "zendesk",
    "intercom", "freshdesk", "helpscout", "hubspot", "salesforce",
    "mail.google", "#engineering", "#product", "#design", "#general",
    "#support", "thread",
];

const DATA_FINANCE_MARKERS: &[&str] = &[
    "excel", "sheets", "spreadsheet", "workbook", "pivot", "vlookup",
    "formula", "dashboard", "metabase", "tableau", "power bi", "looker",
    "superset", "jupyter", "notebook", "colab", "pandas", "dataframe",
    "sql", "postgres", "sqlite", "supabase", "bigquery", "snowflake",
    "stripe", "quickbooks", "xero", "invoice", "revenue", "ledger",
    "balance sheet", "forecast", "budget", "accounting", "tax return",
    "bloomberg", "portfolio", ".xlsx", ".csv",
];

const OPS_INFRA_MARKERS: &[&str] = &[
    "aws console", "amazon web services", "cloud.google", "portal.azure",
    "vercel", "render.com", "cloudflare", "datadog", "grafana", "sentry",
    "pagerduty", "prometheus", "kibana", "splunk", "github actions",
    "gitlab ci", "circleci", "jenkins", "terraform", "kubernetes",
    "deployment", "deployments", "cluster", "iam", "dns", "ssl",
    "monitoring", "incident", "status page", "admin panel", "cpanel",
    "file explorer", "nautilus", "dolphin", "finder", "system settings",
];

const SHORT_FORM_MARKERS: &[&str] = &[
    "/shorts/", "#shorts", "/reels/", "reels", "tiktok", "fyp",
    "for you page", "try not to laugh", "prank", "memes compilation",
    "funny moments", "clickbait", "stories",
];

const ENTERTAINMENT_MARKERS: &[&str] = &[
    "netflix", "prime video", "disney+", "hotstar", "hulu", "crunchyroll",
    "spotify", "apple music", "soundcloud", "official video",
    "official audio", "official trailer", "teaser trailer", "music video",
    "full album", "lyrics", "lofi hip hop", "chillhop", "vlog",
    "reaction", "standup", "comedy", "podcast", "episode", "season ",
    "highlights", "espn", "nba", "nfl", "fifa", "cricket", "ipl",
    "ufc", "wwe", "f1 race", "movie", "anime",
];

const GAMING_MARKERS: &[&str] = &[
    "steam", "epic games", "battle.net", "riot client", "xbox",
    "playstation", "gameplay", "let's play", "playthrough", "speedrun",
    "boss fight", "ranked match", "minecraft", "valorant", "fortnite",
    "league of legends", "dota 2", "counter-strike", "cs2", "apex legends",
    "overwatch", "gta ", "grand theft auto", "roblox", "elden ring",
    "baldur's gate", "cyberpunk 2077", "stardew valley", "chess.com",
    "lichess", "twitch.tv", "esports",
];

const SOCIAL_FEED_MARKERS: &[&str] = &[
    "instagram", "facebook", "threads.net", "snapchat", "pinterest",
    "r/memes", "r/funny", "r/aww", "r/gifs", "r/videos", "r/gaming",
    "r/askreddit", "r/pics", "r/dankmemes", "r/mildlyinteresting",
    "suggested for you", "view all comments", "add a comment",
    "retweet", "retweets", "followers", "following", "trending now",
    "home / x", "explore / x", "for you", "news feed", "celebrity",
    "gossip", "drama",
];

const SHOPPING_MARKERS: &[&str] = &[
    "add to cart", "buy now", "proceed to checkout", "shopping cart",
    "wishlist", "in stock", "free shipping", "free delivery",
    "order summary", "promo code", "coupon", "deals of the day",
    "customer reviews", "ratings & reviews", "track package", "my orders",
    "flipkart", "myntra", "ebay", "aliexpress", "etsy", "walmart",
    "target.com", "bestbuy", "zomato", "swiggy", "doordash", "ubereats",
    "airbnb", "booking.com", "expedia", "flights to", "price:",
    "amazon.com/dp/", "amazon.in/dp/", "/cart", "/checkout",
];

#[derive(Debug, Default, Clone)]
pub struct FieldIds {
    pub vocab_ids: Vec<usize>,
    pub hash_ids: Vec<usize>,
}

#[derive(Debug, Default, Clone)]
pub struct ContextFeatureIds {
    pub vocab_ids: Vec<usize>,
    pub hash_ids: Vec<usize>,
    pub sig_tokens: Vec<String>,
    pub known_projects_lower: Vec<String>,
}

/// Deterministic 64-bit FNV-1a hash over UTF-8 bytes.
#[inline]
pub fn fnv1a_64(text: &str) -> u64 {
    let mut h = FNV_OFFSET_BASIS;
    for &b in text.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[inline]
pub fn hash_bucket(feature: &str, num_buckets: usize) -> usize {
    (fnv1a_64(feature) % (num_buckets as u64)) as usize
}

pub fn normalize_app_stem(app_name: &str) -> String {
    let mut s = app_name.trim().to_lowercase();
    for ext in [".exe", ".app", ".bin"] {
        if s.len() > ext.len() && s.ends_with(ext) {
            s.truncate(s.len() - ext.len());
            break;
        }
    }
    s.trim().to_string()
}

/// Split text into lowercase alphanumeric tokens, splitting camelCase boundaries and paths.
pub fn split_tokens(text: &str, max_tokens: usize) -> Vec<String> {
    let mut tokens = Vec::with_capacity(max_tokens.min(32));
    let mut buf = String::new();
    let mut prev_lower = false;

    let flush = |buf: &mut String, tokens: &mut Vec<String>| {
        if !buf.is_empty() {
            let tok = buf.to_lowercase();
            buf.clear();
            let char_len = tok.chars().count();
            if char_len >= 2 || matches!(tok.as_str(), "c" | "r" | "x") {
                tokens.push(tok);
            }
        }
    };

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '+' || ch == '#' {
            if prev_lower && ch.is_uppercase() && !buf.is_empty() {
                flush(&mut buf, &mut tokens);
                if tokens.len() >= max_tokens {
                    return tokens;
                }
            }
            buf.push(ch);
            prev_lower = ch.is_lowercase();
        } else {
            flush(&mut buf, &mut tokens);
            prev_lower = false;
            if tokens.len() >= max_tokens {
                return tokens;
            }
        }
    }
    flush(&mut buf, &mut tokens);
    tokens.truncate(max_tokens);
    tokens
}

/// Parse URL into (normalized host without leading `www.`, path/query tokens).
pub fn parse_url_parts(url: Option<&str>) -> (String, Vec<String>) {
    let Some(raw) = url else {
        return (String::new(), Vec::new());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), Vec::new());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let Ok(parsed) = Url::parse(&candidate) else {
        return (String::new(), Vec::new());
    };
    let host = parsed
        .host_str()
        .map(|h| {
            let lower = h.to_lowercase();
            lower.strip_prefix("www.").unwrap_or(&lower).to_string()
        })
        .unwrap_or_default();
    let path_and_query = match parsed.query() {
        Some(q) => format!("{} {}", parsed.path(), q),
        None => parsed.path().to_string(),
    };
    let path_tokens = split_tokens(&path_and_query, 24);
    (host, path_tokens)
}

#[inline]
pub fn take_chars(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

/// Extract (app, title+url, content) vocab and FNV-1a hash bucket IDs.
pub fn extract_field_features<F>(
    app_name: &str,
    window_title: &str,
    url: Option<&str>,
    content: Option<&str>,
    num_buckets: usize,
    lookup_vocab: F,
) -> (FieldIds, FieldIds, FieldIds)
where
    F: Fn(&str) -> Option<usize>,
{
    // 1. App field
    let app_stem = normalize_app_stem(app_name);
    let app_tokens = split_tokens(&app_stem, 16);
    let mut app_ids = FieldIds::default();
    if !app_stem.is_empty() {
        app_ids
            .hash_ids
            .push(hash_bucket(&format!("app_full:{app_stem}"), num_buckets));
    }
    for tok in &app_tokens {
        if let Some(vid) = lookup_vocab(tok) {
            app_ids.vocab_ids.push(vid);
        }
        app_ids
            .hash_ids
            .push(hash_bucket(&format!("app:{tok}"), num_buckets));
        let chars: Vec<char> = tok.chars().collect();
        if chars.len() >= 4 {
            let mut padded = Vec::with_capacity(chars.len() + 2);
            padded.push('<');
            padded.extend_from_slice(&chars);
            padded.push('>');
            for win in padded.windows(3) {
                let tri: String = win.iter().collect();
                app_ids
                    .hash_ids
                    .push(hash_bucket(&format!("a3:{tri}"), num_buckets));
            }
        }
    }

    // 2. Title + URL field
    let (host, path_tokens) = parse_url_parts(url);
    let title_tokens = split_tokens(window_title, 48);
    let mut tu_ids = FieldIds::default();
    if !host.is_empty() {
        tu_ids
            .hash_ids
            .push(hash_bucket(&format!("dom:{host}"), num_buckets));
        for part in host.split('.') {
            if part.len() >= 2 && !matches!(part, "com" | "org" | "net" | "io" | "co" | "www") {
                if let Some(vid) = lookup_vocab(part) {
                    tu_ids.vocab_ids.push(vid);
                }
                tu_ids
                    .hash_ids
                    .push(hash_bucket(&format!("domp:{part}"), num_buckets));
            }
        }
    }
    for pt in path_tokens.iter().take(16) {
        if let Some(vid) = lookup_vocab(pt) {
            tu_ids.vocab_ids.push(vid);
        }
        tu_ids
            .hash_ids
            .push(hash_bucket(&format!("path:{pt}"), num_buckets));
    }
    for (idx, tok) in title_tokens.iter().enumerate() {
        if let Some(vid) = lookup_vocab(tok) {
            tu_ids.vocab_ids.push(vid);
        }
        tu_ids
            .hash_ids
            .push(hash_bucket(&format!("ttl:{tok}"), num_buckets));
        if idx + 1 < title_tokens.len() {
            tu_ids.hash_ids.push(hash_bucket(
                &format!("t2:{}_{}", tok, title_tokens[idx + 1]),
                num_buckets,
            ));
        }
        let chars: Vec<char> = tok.chars().collect();
        if (4..=20).contains(&chars.len()) {
            let mut padded = Vec::with_capacity(chars.len() + 2);
            padded.push('<');
            padded.extend_from_slice(&chars);
            padded.push('>');
            for win in padded.windows(3) {
                let tri: String = win.iter().collect();
                tu_ids
                    .hash_ids
                    .push(hash_bucket(&format!("c3:{tri}"), num_buckets));
            }
        }
    }

    // 3. Content field (Stage 2 deep text)
    let mut cnt_ids = FieldIds::default();
    if let Some(raw_content) = content {
        if !raw_content.trim().is_empty() {
            let snippet = take_chars(raw_content, 800);
            let cnt_tokens = split_tokens(snippet, 96);
            for (idx, tok) in cnt_tokens.iter().enumerate() {
                if let Some(vid) = lookup_vocab(tok) {
                    cnt_ids.vocab_ids.push(vid);
                }
                cnt_ids
                    .hash_ids
                    .push(hash_bucket(&format!("txt:{tok}"), num_buckets));
                if idx + 1 < cnt_tokens.len() {
                    cnt_ids.hash_ids.push(hash_bucket(
                        &format!("c2:{}_{}", tok, cnt_tokens[idx + 1]),
                        num_buckets,
                    ));
                }
            }
        }
    }

    (app_ids, tu_ids, cnt_ids)
}

/// Extract vocab/hash IDs and lexical token sets for active task + known projects.
pub fn extract_context_feature_ids<F>(
    task_title: Option<&str>,
    task_description: Option<&str>,
    known_projects: &[String],
    num_buckets: usize,
    lookup_vocab: F,
) -> ContextFeatureIds
where
    F: Fn(&str) -> Option<usize>,
{
    let mut parts = Vec::new();
    if let Some(title) = task_title {
        let t = title.trim();
        if !t.is_empty() {
            parts.push(t.to_string());
        }
    }
    if let Some(desc) = task_description {
        let d = desc.trim();
        if !d.is_empty() {
            parts.push(take_chars(d, 300).to_string());
        }
    }
    let mut known_projects_lower = Vec::new();
    for p in known_projects {
        let clean = p.trim().to_lowercase();
        if clean.chars().count() >= 2 {
            known_projects_lower.push(clean.clone());
            parts.push(clean);
        }
    }

    let joined = parts.join(" ");
    if joined.trim().is_empty() {
        return ContextFeatureIds {
            known_projects_lower,
            ..Default::default()
        };
    }

    let tokens = split_tokens(&joined, 64);
    let mut vocab_ids = Vec::new();
    let mut hash_ids = Vec::new();
    let mut sig_tokens = Vec::new();
    let mut seen = HashSet::new();

    for tok in tokens {
        if tok.chars().count() >= 3
            && !STOPWORDS.contains(&tok.as_str())
            && seen.insert(tok.clone())
        {
            sig_tokens.push(tok.clone());
        }
        if let Some(vid) = lookup_vocab(&tok) {
            vocab_ids.push(vid);
        }
        hash_ids.push(hash_bucket(&format!("ttl:{tok}"), num_buckets));
    }

    ContextFeatureIds {
        vocab_ids,
        hash_ids,
        sig_tokens,
        known_projects_lower,
    }
}

pub fn compute_lexical_context_overlap(
    window_title: &str,
    url: Option<&str>,
    content: Option<&str>,
    ctx_sig_tokens: &[String],
    known_projects_lower: &[String],
) -> (f32, f32) {
    let content_snip = content.map(|c| take_chars(c, 400)).unwrap_or("");
    let combined_lower = format!("{window_title} {} {content_snip}", url.unwrap_or("")).to_lowercase();
    let exact_proj = if known_projects_lower
        .iter()
        .any(|p| p.chars().count() >= 2 && combined_lower.contains(p.as_str()))
    {
        1.0
    } else {
        0.0
    };
    if ctx_sig_tokens.is_empty() {
        return (exact_proj, 0.0);
    }
    let ev_tokens: HashSet<String> = split_tokens(&combined_lower, 96).into_iter().collect();
    let hits = ctx_sig_tokens
        .iter()
        .filter(|t| ev_tokens.contains(t.as_str()))
        .count();
    let denom = ctx_sig_tokens.len().min(6).max(1) as f32;
    (exact_proj, (hits as f32 / denom).min(1.0))
}

#[inline]
fn count_hits(haystack_lower: &str, markers: &[&str], cap: f32) -> f32 {
    let mut hits = 0usize;
    for marker in markers {
        if haystack_lower.contains(marker) {
            hits += 1;
        }
    }
    ((hits as f32) / cap).min(1.0)
}

/// Compute the 24 continuous structural features matching `schema.py` identically.
pub fn extract_structural_features(
    app_name: &str,
    window_title: &str,
    url: Option<&str>,
    content: Option<&str>,
) -> [f32; STRUCT_FEAT_DIM] {
    let app_lower = app_name.trim().to_lowercase();
    let title_lower = window_title.trim().to_lowercase();
    let url_lower = url.unwrap_or("").trim().to_lowercase();
    let content_snip = content.map(|c| take_chars(c, 1200)).unwrap_or("");
    let content_lower = content_snip.to_lowercase();
    let combined = format!("{app_lower} {title_lower} {url_lower} {content_lower}");

    let has_deep = if !content_snip.trim().is_empty() { 1.0 } else { 0.0 };
    let has_url = if !url_lower.is_empty() { 1.0 } else { 0.0 };
    let is_browser = if BROWSER_APPS.iter().any(|b| app_lower.contains(b)) { 1.0 } else { 0.0 };
    let is_ide = if IDE_APPS.iter().any(|e| app_lower.contains(e)) { 1.0 } else { 0.0 };
    let is_terminal = if TERMINAL_APPS.iter().any(|t| app_lower.contains(t)) { 1.0 } else { 0.0 };
    let is_dual_use = if DUAL_USE_HOSTS
        .iter()
        .any(|d| app_lower.contains(d) || title_lower.contains(d) || url_lower.contains(d))
    {
        1.0
    } else {
        0.0
    };

    let code_syntax = count_hits(&combined, CODE_SYNTAX_MARKERS, 5.0);
    let code_ext = if CODE_EXTENSIONS.iter().any(|e| combined.contains(e)) { 1.0 } else { 0.0 };
    let shell_cmd = count_hits(&combined, SHELL_CMD_MARKERS, 3.0);
    let learning = count_hits(&combined, LEARNING_MARKERS, 3.0);
    let research = count_hits(&combined, RESEARCH_DOC_MARKERS, 3.0);
    let design_media = count_hits(&combined, DESIGN_MEDIA_MARKERS, 3.0);
    let planning = count_hits(&combined, PLANNING_MARKERS, 3.0);
    let meeting = count_hits(&combined, MEETING_MARKERS, 2.0);
    let comm = count_hits(&combined, COMM_MARKERS, 3.0);
    let data_fin = count_hits(&combined, DATA_FINANCE_MARKERS, 3.0);
    let ops_infra = count_hits(&combined, OPS_INFRA_MARKERS, 3.0);
    let short_form = if SHORT_FORM_MARKERS.iter().any(|m| combined.contains(m)) { 1.0 } else { 0.0 };
    let entertainment = count_hits(&combined, ENTERTAINMENT_MARKERS, 3.0);
    let gaming = count_hits(&combined, GAMING_MARKERS, 2.0);
    let social_feed = count_hits(&combined, SOCIAL_FEED_MARKERS, 3.0);
    let shopping = count_hits(&combined, SHOPPING_MARKERS, 3.0);

    let title_len_norm = ((window_title.chars().count() as f32) / 120.0).min(1.0);

    let raw_combo = format!("{window_title} {}", take_chars(content_snip, 400));
    let mut ident_hits = 0usize;
    let mut word_count = 0usize;
    for w in raw_combo.split_whitespace().take(60) {
        word_count += 1;
        let has_snake = w.contains('_') && w.chars().any(|c| c.is_alphabetic());
        let chars: Vec<char> = w.chars().collect();
        let has_camel = chars
            .windows(2)
            .any(|pair| pair[0].is_lowercase() && pair[1].is_uppercase());
        let has_scope = w.contains("::")
            || w.contains("->")
            || (w.contains('/') && !w.starts_with("http"));
        if has_snake || has_camel || has_scope {
            ident_hits += 1;
        }
    }
    let ident_density = if word_count > 0 {
        ((ident_hits as f32) / 8.0).min(1.0)
    } else {
        0.0
    };

    [
        has_deep,
        has_url,
        is_browser,
        is_ide,
        is_terminal,
        is_dual_use,
        code_syntax,
        code_ext,
        shell_cmd,
        learning,
        research,
        design_media,
        planning,
        meeting,
        comm,
        data_fin,
        ops_infra,
        short_form,
        entertainment,
        gaming,
        social_feed,
        shopping,
        title_len_norm,
        ident_density,
    ]
}
