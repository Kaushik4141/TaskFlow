"""Single source of truth for TaskFlow-TinyNet v1 feature extraction, taxonomy, and binary export.

EVERY constant, normalization step, FNV-1a hash, structural feature, and binary layout
defined here MUST match `src-tauri/src/classifier/features.rs` and `model.rs` byte-for-byte.
"""
from __future__ import annotations

import hashlib
import math
import struct
from dataclasses import dataclass
from typing import Dict, List, Optional, Sequence, Tuple
from urllib.parse import urlparse

# ---------------------------------------------------------------------------
# Model Hyperparameters & Binary Format Constants
# ---------------------------------------------------------------------------
MAGIC = b"TFCL"
FORMAT_VERSION = 1

EMBED_DIM = 96
APP_DIM = 64          # First 64 PCA-ordered dims of EMBED_DIM
CTX_HADAMARD_DIM = 40 # First 40 PCA-ordered dims for element-wise event*ctx interaction
CTX_SCALAR_DIM = 8
CTX_FEAT_DIM = CTX_SCALAR_DIM + CTX_HADAMARD_DIM  # 48
STRUCT_FEAT_DIM = 24
INPUT_DIM = APP_DIM + EMBED_DIM + EMBED_DIM + CTX_FEAT_DIM + STRUCT_FEAT_DIM  # 64+96+96+48+24 = 328
HIDDEN_DIM = 128
NUM_CATEGORIES = 16
HASH_BUCKETS = 12288

# FNV-1a 64-bit constants (exact unsigned 64-bit arithmetic)
FNV_OFFSET_BASIS = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
MASK64 = 0xFFFFFFFFFFFFFFFF

# ---------------------------------------------------------------------------
# Universal 16-Category Desktop Taxonomy (12 Productive + 4 Non-Productive)
# ---------------------------------------------------------------------------
CATEGORIES: Tuple[str, ...] = (
    # Productive (0..11)
    "coding",
    "terminal",
    "research",
    "learning",
    "writing",
    "design",
    "media_production",
    "planning",
    "meetings",
    "communication",
    "data_finance",
    "operations",
    # Non-Productive (12..15)
    "entertainment",
    "gaming",
    "social_feed",
    "shopping_personal",
)

CATEGORY_TO_ID: Dict[str, int] = {name: idx for idx, name in enumerate(CATEGORIES)}
PRODUCTIVE_CATEGORY_IDS = frozenset(range(12))
NON_PRODUCTIVE_CATEGORY_IDS = frozenset(range(12, 16))

# ---------------------------------------------------------------------------
# Lexicons for the 24 Continuous Structural Signals (mirrored in features.rs)
# ---------------------------------------------------------------------------
STOPWORDS = frozenset({
    "the", "and", "for", "with", "this", "that", "from", "into", "over",
    "your", "what", "when", "where", "how", "who", "all", "any", "are",
    "was", "were", "have", "has", "had", "not", "but", "you", "our",
    "out", "new", "tab", "window", "page", "home", "app", "com", "www",
    "http", "https", "org", "net",
})

BROWSER_APPS = (
    "chrome", "chromium", "firefox", "edge", "msedge", "brave", "arc",
    "safari", "opera", "vivaldi", "zen", "helium", "librewolf", "waterfox",
)

IDE_APPS = (
    "cursor", "code", "vscode", "vscodium", "intellij", "idea", "pycharm",
    "webstorm", "goland", "clion", "rider", "rustrover", "datagrip",
    "neovim", "nvim", "vim", "helix", "zed", "sublime", "xcode",
    "android studio", "eclipse", "emacs", "windsurf",
)

TERMINAL_APPS = (
    "alacritty", "kitty", "wezterm", "ghostty", "foot", "warp", "iterm",
    "terminal", "windowsterminal", "wt", "powershell", "pwsh", "cmd",
    "konsole", "gnome-terminal", "hyper", "tmux",
)

DUAL_USE_HOSTS = (
    "youtube.com", "youtu.be", "reddit.com", "twitter.com", "x.com",
    "twitch.tv", "discord.com", "discord", "instagram.com", "tiktok.com",
    "facebook.com", "threads.net", "pinterest.com", "amazon.com",
    "amazon.in", "flipkart.com", "ebay.com", "myntra.com", "medium.com",
    "substack.com", "netflix.com", "primevideo.com", "hotstar.com",
    "disneyplus.com", "hulu.com", "spotify.com", "youtube", "reddit",
    " / x", "twitch",
)

CODE_SYNTAX_MARKERS = (
    "fn ", "def ", "function ", "func ", "class ", "struct ", "enum ",
    "interface ", "impl ", "trait ", "const ", "let ", "mut ", "pub ",
    "return ", "import ", "export ", "async ", "await ", "select ",
    "insert into", "create table", "=>", "->", "::", "!=", "===", "&&", "||",
    "/>", "</", "{", "}", "#include",
)

CODE_EXTENSIONS = (
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".c", ".cpp",
    ".h", ".hpp", ".java", ".kt", ".swift", ".cs", ".rb", ".php", ".sh",
    ".sql", ".json", ".toml", ".yaml", ".yml", ".html", ".css", ".scss",
    ".proto", ".graphql", ".tf", ".dockerfile", "src/", "lib/", "tests/",
    "components/", "migrations/", "pkg/",
)

SHELL_CMD_MARKERS = (
    "cargo ", "npm ", "pnpm ", "yarn ", "npx ", "pip ", "python ",
    "python3 ", "git ", "docker ", "kubectl ", "curl ", "ssh ", "grep ",
    "rg ", "systemctl ", "psql ", "sqlite3 ", "go test", "go run",
    "pytest", "make ", "cmake ", "terraform ", "helm ", "uv ", "bun ",
)

LEARNING_MARKERS = (
    "tutorial", "course", "full course", "crash course", "lecture",
    "lesson", "module", "syllabus", "masterclass", "bootcamp", "workshop",
    "explained", "deep dive", "walkthrough", "study", "flashcard", "quiz",
    "exam", "homework", "assignment", "coursera", "udemy", "edx",
    "khan academy", "leetcode", "hackerrank", "kaggle", "anki", "duolingo",
    "mit ", "stanford ", "cs50", "3blue1brown", "freecodecamp",
)

RESEARCH_DOC_MARKERS = (
    "documentation", "docs", "api reference", "reference", "manual",
    "specification", "rfc ", "arxiv", "pubmed", "scholar", "doi:",
    "abstract", "methodology", "citation", "paper", "whitepaper",
    "stackoverflow", "stackexchange", "developer.mozilla", "mdn",
    "docs.rs", "wikipedia", "perplexity", "claude", "chatgpt", "gemini",
    "benchmark", "case study",
)

DESIGN_MEDIA_MARKERS = (
    "figma", "sketch", "blender", "photoshop", "illustrator", "indesign",
    "after effects", "premiere", "davinci", "resolve", "final cut",
    "audacity", "ableton", "logic pro", "obs studio", "descript",
    "canva", "miro", "excalidraw", "autocad", "solidworks", "kicad",
    "wireframe", "prototype", "artboard", "keyframe", "color grade",
    "timeline", "render", "layer", "vector", "typography", "youtube studio",
)

PLANNING_MARKERS = (
    "sprint", "backlog", "roadmap", "milestone", "kanban", "epic",
    "ticket", "issue #", "issues", "pull request", "jira", "linear",
    "asana", "trello", "clickup", "monday.com", "calendar", "schedule",
    "standup", "retrospective", "okr", "quarterly", "project plan",
    "todo", "board",
)

MEETING_MARKERS = (
    "zoom meeting", "zoom workplace", "meet.google", "google meet",
    "teams meeting", "microsoft teams", "huddle", "webex", "whereby",
    "loom", "participants", "screen share", "unmute", "leave call",
    "join meeting", "1:1", "sync call", "all-hands", "interview",
    "demo call", "webinar",
)

COMM_MARKERS = (
    "gmail", "outlook", "thunderbird", "proton mail", "inbox", "unread",
    "compose", "reply", "subject:", "sent mail", "slack", "zendesk",
    "intercom", "freshdesk", "helpscout", "hubspot", "salesforce",
    "mail.google", "#engineering", "#product", "#design", "#general",
    "#support", "thread",
)

DATA_FINANCE_MARKERS = (
    "excel", "sheets", "spreadsheet", "workbook", "pivot", "vlookup",
    "formula", "dashboard", "metabase", "tableau", "power bi", "looker",
    "superset", "jupyter", "notebook", "colab", "pandas", "dataframe",
    "sql", "postgres", "sqlite", "supabase", "bigquery", "snowflake",
    "stripe", "quickbooks", "xero", "invoice", "revenue", "ledger",
    "balance sheet", "forecast", "budget", "accounting", "tax return",
    "bloomberg", "portfolio", ".xlsx", ".csv",
)

OPS_INFRA_MARKERS = (
    "aws console", "amazon web services", "cloud.google", "portal.azure",
    "vercel", "render.com", "cloudflare", "datadog", "grafana", "sentry",
    "pagerduty", "prometheus", "kibana", "splunk", "github actions",
    "gitlab ci", "circleci", "jenkins", "terraform", "kubernetes",
    "deployment", "deployments", "cluster", "iam", "dns", "ssl",
    "monitoring", "incident", "status page", "admin panel", "cpanel",
    "file explorer", "nautilus", "dolphin", "finder", "system settings",
)

SHORT_FORM_MARKERS = (
    "/shorts/", "#shorts", "/reels/", "reels", "tiktok", "fyp",
    "for you page", "try not to laugh", "prank", "memes compilation",
    "funny moments", "clickbait", "stories",
)

ENTERTAINMENT_MARKERS = (
    "netflix", "prime video", "disney+", "hotstar", "hulu", "crunchyroll",
    "spotify", "apple music", "soundcloud", "official video",
    "official audio", "official trailer", "teaser trailer", "music video",
    "full album", "lyrics", "lofi hip hop", "chillhop", "vlog",
    "reaction", "standup", "comedy", "podcast", "episode", "season ",
    "highlights", "espn", "nba", "nfl", "fifa", "cricket", "ipl",
    "ufc", "wwe", "f1 race", "movie", "anime",
)

GAMING_MARKERS = (
    "steam", "epic games", "battle.net", "riot client", "xbox",
    "playstation", "gameplay", "let's play", "playthrough", "speedrun",
    "boss fight", "ranked match", "minecraft", "valorant", "fortnite",
    "league of legends", "dota 2", "counter-strike", "cs2", "apex legends",
    "overwatch", "gta ", "grand theft auto", "roblox", "elden ring",
    "baldur's gate", "cyberpunk 2077", "stardew valley", "chess.com",
    "lichess", "twitch.tv", "esports",
)

SOCIAL_FEED_MARKERS = (
    "instagram", "facebook", "threads.net", "snapchat", "pinterest",
    "r/memes", "r/funny", "r/aww", "r/gifs", "r/videos", "r/gaming",
    "r/askreddit", "r/pics", "r/dankmemes", "r/mildlyinteresting",
    "suggested for you", "view all comments", "add a comment",
    "retweet", "retweets", "followers", "following", "trending now",
    "home / x", "explore / x", "for you", "news feed", "celebrity",
    "gossip", "drama",
)

SHOPPING_MARKERS = (
    "add to cart", "buy now", "proceed to checkout", "shopping cart",
    "wishlist", "in stock", "free shipping", "free delivery",
    "order summary", "promo code", "coupon", "deals of the day",
    "customer reviews", "ratings & reviews", "track package", "my orders",
    "flipkart", "myntra", "ebay", "aliexpress", "etsy", "walmart",
    "target.com", "bestbuy", "zomato", "swiggy", "doordash", "ubereats",
    "airbnb", "booking.com", "expedia", "flights to", "price:",
    "amazon.com/dp/", "amazon.in/dp/", "/cart", "/checkout",
)


# ---------------------------------------------------------------------------
# Byte-Exact Hashing & Tokenization (identical to Rust features.rs)
# ---------------------------------------------------------------------------
def fnv1a_64(text: str) -> int:
    """Deterministic 64-bit FNV-1a hash over UTF-8 bytes."""
    h = FNV_OFFSET_BASIS
    for b in text.encode("utf-8"):
        h ^= b
        h = (h * FNV_PRIME) & MASK64
    return h


def hash_bucket(feature: str, num_buckets: int = HASH_BUCKETS) -> int:
    return fnv1a_64(feature) % num_buckets


def normalize_app_stem(app_name: str) -> str:
    s = app_name.strip().lower()
    for ext in (".exe", ".app", ".bin"):
        if len(s) > len(ext) and s.endswith(ext):
            s = s[: -len(ext)]
    return s.strip()


def split_tokens(text: str, max_tokens: int = 96) -> List[str]:
    """Split text into lowercase alphanumeric tokens, splitting camelCase and paths.

    Matches `features::split_tokens` in Rust exactly.
    """
    tokens: List[str] = []
    buf: List[str] = []
    prev_lower = False

    def flush() -> None:
        if buf:
            tok = "".join(buf).lower()
            buf.clear()
            if len(tok) >= 2 or tok in ("c", "r", "x"):
                tokens.append(tok)

    for ch in text:
        if ch.isalnum() or ch in ("+", "#"):
            if prev_lower and ch.isupper() and buf:
                flush()
                if len(tokens) >= max_tokens:
                    return tokens
            buf.append(ch)
            prev_lower = ch.islower()
        else:
            flush()
            prev_lower = False
            if len(tokens) >= max_tokens:
                return tokens

    flush()
    return tokens[:max_tokens]


def parse_url_parts(url: Optional[str]) -> Tuple[str, List[str]]:
    """Extract normalized host (without www.) and path/query tokens."""
    if not url:
        return "", []
    u = url.strip()
    if not u:
        return "", []
    if "://" not in u:
        u = "https://" + u
    try:
        parsed = urlparse(u)
    except Exception:
        return "", []
    host = (parsed.hostname or "").lower()
    if host.startswith("www."):
        host = host[4:]
    path_and_query = (parsed.path or "") + " " + (parsed.query or "")
    path_tokens = split_tokens(path_and_query, max_tokens=24)
    return host, path_tokens


def extract_field_features(
    app_name: str,
    window_title: str,
    url: Optional[str],
    content: Optional[str],
    vocab_to_id: Dict[str, int],
) -> Tuple[
    Tuple[List[int], List[int]],  # app (vocab_ids, hash_ids)
    Tuple[List[int], List[int]],  # title_url (vocab_ids, hash_ids)
    Tuple[List[int], List[int]],  # content (vocab_ids, hash_ids)
]:
    """Extract (vocab_ids, hash_ids) for each of the three input fields."""
    # 1. App field
    app_stem = normalize_app_stem(app_name)
    app_tokens = split_tokens(app_stem, max_tokens=16)
    app_vocab: List[int] = []
    app_hashes: List[int] = []
    if app_stem:
        app_hashes.append(hash_bucket(f"app_full:{app_stem}"))
    for tok in app_tokens:
        vid = vocab_to_id.get(tok)
        if vid is not None:
            app_vocab.append(vid)
        app_hashes.append(hash_bucket(f"app:{tok}"))
        if len(tok) >= 4:
            padded = f"<{tok}>"
            for i in range(len(padded) - 2):
                app_hashes.append(hash_bucket(f"a3:{padded[i:i+3]}"))

    # 2. Title + URL field
    host, path_tokens = parse_url_parts(url)
    title_tokens = split_tokens(window_title, max_tokens=48)
    tu_vocab: List[int] = []
    tu_hashes: List[int] = []
    if host:
        tu_hashes.append(hash_bucket(f"dom:{host}"))
        for part in host.split("."):
            if len(part) >= 2 and part not in ("com", "org", "net", "io", "co", "www"):
                vid = vocab_to_id.get(part)
                if vid is not None:
                    tu_vocab.append(vid)
                tu_hashes.append(hash_bucket(f"domp:{part}"))
    for pt in path_tokens[:16]:
        vid = vocab_to_id.get(pt)
        if vid is not None:
            tu_vocab.append(vid)
        tu_hashes.append(hash_bucket(f"path:{pt}"))

    for idx, tok in enumerate(title_tokens):
        vid = vocab_to_id.get(tok)
        if vid is not None:
            tu_vocab.append(vid)
        tu_hashes.append(hash_bucket(f"ttl:{tok}"))
        if idx + 1 < len(title_tokens):
            tu_hashes.append(hash_bucket(f"t2:{tok}_{title_tokens[idx+1]}"))
        if len(tok) >= 4 and len(tok) <= 20:
            padded = f"<{tok}>"
            for i in range(len(padded) - 2):
                tu_hashes.append(hash_bucket(f"c3:{padded[i:i+3]}"))

    # 3. Content field (Stage 2 deep text)
    cnt_vocab: List[int] = []
    cnt_hashes: List[int] = []
    if content and content.strip():
        snippet = content[:800]
        cnt_tokens = split_tokens(snippet, max_tokens=96)
        for idx, tok in enumerate(cnt_tokens):
            vid = vocab_to_id.get(tok)
            if vid is not None:
                cnt_vocab.append(vid)
            cnt_hashes.append(hash_bucket(f"txt:{tok}"))
            if idx + 1 < len(cnt_tokens):
                cnt_hashes.append(hash_bucket(f"c2:{tok}_{cnt_tokens[idx+1]}"))

    return (app_vocab, app_hashes), (tu_vocab, tu_hashes), (cnt_vocab, cnt_hashes)


def extract_context_features_ids(
    task_title: Optional[str],
    task_description: Optional[str],
    known_projects: Sequence[str],
    vocab_to_id: Dict[str, int],
) -> Tuple[List[int], List[int], List[str], List[str]]:
    """Extract vocab/hash IDs and normalized keyword sets for active task + known projects."""
    parts: List[str] = []
    if task_title and task_title.strip():
        parts.append(task_title.strip())
    if task_description and task_description.strip():
        parts.append(task_description.strip()[:300])
    proj_lowers: List[str] = []
    for p in known_projects:
        p_clean = p.strip().lower()
        if len(p_clean) >= 2:
            proj_lowers.append(p_clean)
            parts.append(p_clean)

    joined = " ".join(parts)
    if not joined.strip():
        return [], [], [], proj_lowers

    tokens = split_tokens(joined, max_tokens=64)
    vocab_ids: List[int] = []
    hash_ids: List[int] = []
    sig_tokens: List[str] = []
    seen = set()
    for tok in tokens:
        if len(tok) >= 3 and tok not in STOPWORDS and tok not in seen:
            seen.add(tok)
            sig_tokens.append(tok)
        vid = vocab_to_id.get(tok)
        if vid is not None:
            vocab_ids.append(vid)
        hash_ids.append(hash_bucket(f"ttl:{tok}"))
    return vocab_ids, hash_ids, sig_tokens, proj_lowers


def _count_hits(haystack_lower: str, markers: Sequence[str], cap: float) -> float:
    hits = 0
    for m in markers:
        if m in haystack_lower:
            hits += 1
    return min(float(hits) / cap, 1.0)


def extract_structural_features(
    app_name: str,
    window_title: str,
    url: Optional[str],
    content: Optional[str],
) -> List[float]:
    """Compute the 24 normalized continuous structural features in [0.0, 1.0].

    Must match `features::extract_structural_features` in Rust identically.
    """
    app_lower = app_name.strip().lower()
    title_lower = window_title.strip().lower()
    url_lower = (url or "").strip().lower()
    content_snip = (content or "")[:1200]
    content_lower = content_snip.lower()
    combined = f"{app_lower} {title_lower} {url_lower} {content_lower}"

    has_deep = 1.0 if bool(content_snip.strip()) else 0.0
    has_url = 1.0 if bool(url_lower) else 0.0
    is_browser = 1.0 if any(b in app_lower for b in BROWSER_APPS) else 0.0
    is_ide = 1.0 if any(e in app_lower for e in IDE_APPS) else 0.0
    is_terminal = 1.0 if any(t in app_lower for t in TERMINAL_APPS) else 0.0
    is_dual_use = 1.0 if any(
        d in app_lower or d in title_lower or d in url_lower for d in DUAL_USE_HOSTS
    ) else 0.0

    code_syntax = _count_hits(combined, CODE_SYNTAX_MARKERS, 5.0)
    code_ext = 1.0 if any(ext in combined for ext in CODE_EXTENSIONS) else 0.0
    shell_cmd = _count_hits(combined, SHELL_CMD_MARKERS, 3.0)
    learning = _count_hits(combined, LEARNING_MARKERS, 3.0)
    research = _count_hits(combined, RESEARCH_DOC_MARKERS, 3.0)
    design_media = _count_hits(combined, DESIGN_MEDIA_MARKERS, 3.0)
    planning = _count_hits(combined, PLANNING_MARKERS, 3.0)
    meeting = _count_hits(combined, MEETING_MARKERS, 2.0)
    comm = _count_hits(combined, COMM_MARKERS, 3.0)
    data_fin = _count_hits(combined, DATA_FINANCE_MARKERS, 3.0)
    ops_infra = _count_hits(combined, OPS_INFRA_MARKERS, 3.0)
    short_form = 1.0 if any(m in combined for m in SHORT_FORM_MARKERS) else 0.0
    entertainment = _count_hits(combined, ENTERTAINMENT_MARKERS, 3.0)
    gaming = _count_hits(combined, GAMING_MARKERS, 2.0)
    social_feed = _count_hits(combined, SOCIAL_FEED_MARKERS, 3.0)
    shopping = _count_hits(combined, SHOPPING_MARKERS, 3.0)

    title_len_norm = min(len(window_title) / 120.0, 1.0)

    # Identifier density: snake_case, camelCase, or slash/dot code paths in raw title+content
    raw_words = (window_title + " " + content_snip[:400]).split()
    if raw_words:
        ident_hits = 0
        for w in raw_words[:60]:
            has_snake = "_" in w and any(c.isalpha() for c in w)
            has_camel = any(a.islower() and b.isupper() for a, b in zip(w, w[1:]))
            has_scope = "::" in w or "->" in w or (w.count("/") >= 1 and not w.startswith("http"))
            if has_snake or has_camel or has_scope:
                ident_hits += 1
        ident_density = min(ident_hits / 8.0, 1.0)
    else:
        ident_density = 0.0

    feats = [
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
    assert len(feats) == STRUCT_FEAT_DIM
    return feats


def compute_lexical_context_overlap(
    window_title: str,
    url: Optional[str],
    content: Optional[str],
    ctx_sig_tokens: Sequence[str],
    known_projects_lower: Sequence[str],
) -> Tuple[float, float]:
    """Compute (exact_project_hit, task_token_overlap) in [0.0, 1.0]."""
    combined_lower = f"{window_title} {url or ''} {(content or '')[:400]}".lower()
    exact_proj = 1.0 if any(p in combined_lower for p in known_projects_lower if len(p) >= 2) else 0.0
    if not ctx_sig_tokens:
        return exact_proj, 0.0
    ev_tokens = set(split_tokens(combined_lower, max_tokens=96))
    hits = sum(1 for t in ctx_sig_tokens if t in ev_tokens)
    denom = max(min(len(ctx_sig_tokens), 6), 1)
    return exact_proj, min(float(hits) / float(denom), 1.0)


# ---------------------------------------------------------------------------
# Binary Packing & Integrity Checksum (`TFCL` v1)
# ---------------------------------------------------------------------------
def pack_tfcl_v1(
    vocab: List[str],
    vocab_q: bytes,        # int8 [V * EMBED_DIM]
    vocab_scales: bytes,   # float32 [V]
    hash_q: bytes,         # int8 [HASH_BUCKETS * EMBED_DIM]
    hash_scales: bytes,    # float32 [HASH_BUCKETS]
    mlp_weights: Dict[str, bytes],
    temp_prod: float,
    temp_cat: float,
    prod_drop_threshold: float = 0.28,
    prod_pass_threshold: float = 0.50,
) -> bytes:
    """Pack quantized weights and MLP parameters into a single SHA-256-verified TFCL v1 binary."""
    # Payload after the 36-byte prefix (4B magic + 32B sha256)
    parts: List[bytes] = []
    # Header metadata: version, vocab_size, hash_buckets, embed_dim, hidden_dim, num_cats, struct_dim, ctx_dim
    parts.append(
        struct.pack(
            "<IIIIIIIIffff",
            FORMAT_VERSION,
            len(vocab),
            HASH_BUCKETS,
            EMBED_DIM,
            HIDDEN_DIM,
            NUM_CATEGORIES,
            STRUCT_FEAT_DIM,
            CTX_FEAT_DIM,
            float(temp_prod),
            float(temp_cat),
            float(prod_drop_threshold),
            float(prod_pass_threshold),
        )
    )
    # Vocab table: uint16 byte_len + utf8 bytes (lexicographically sorted!)
    for tok in vocab:
        b = tok.encode("utf-8")
        parts.append(struct.pack("<H", len(b)))
        parts.append(b)

    # Quantized embedding matrices
    parts.append(vocab_q)
    parts.append(vocab_scales)
    parts.append(hash_q)
    parts.append(hash_scales)

    # MLP FP32 tensors in fixed deterministic order
    tensor_order = (
        "fc1.weight", "fc1.bias",
        "ln1.weight", "ln1.bias",
        "res1.weight", "res1.bias",
        "ln2.weight", "ln2.bias",
        "res2.weight", "res2.bias",
        "ln3.weight", "ln3.bias",
        "head_prod.weight", "head_prod.bias",
        "head_cat.weight", "head_cat.bias",
        "head_rel.weight", "head_rel.bias",
    )
    for key in tensor_order:
        parts.append(mlp_weights[key])

    payload = b"".join(parts)
    digest = hashlib.sha256(payload).digest()
    return MAGIC + digest + payload
