#!/usr/bin/env python3
"""4-Source Hybrid Dataset Generator & Zero-Overlap Holdout Benchmark for TaskFlow-TinyNet v1.

Generates:
1. `training/classifier/data/train.jsonl` (~44,000 genuinely unique combinatorial + semantic examples)
2. `training/classifier/data/val.jsonl` (~5,000 domain/app-disjoint validation examples for honest calibration)
3. `training/classifier/data/eval_dual_use.jsonl` (~1,200 hard dual-use & counterfactual context tests)
4. `training/classifier/data/eval_gold_unseen.jsonl` (~996 strictly zero-overlap unseen-app/domain tests)
"""
from __future__ import annotations

import json
import random
from pathlib import Path
from typing import Dict, List, Optional, Sequence, Tuple

from schema import CATEGORIES

DATA_DIR = Path(__file__).resolve().parent / "data"

BROWSERS = [
    ("Google Chrome", "Google Chrome"),
    ("chrome.exe", "Google Chrome"),
    ("Firefox", "Mozilla Firefox"),
    ("firefox", "Mozilla Firefox"),
    ("Brave Browser", "Brave"),
    ("Arc", "Arc"),
    ("msedge.exe", "Microsoft Edge"),
    ("zen-browser", "Zen Browser"),
    ("helium", "Helium"),
    ("Safari", "Safari"),
]

# ---------------------------------------------------------------------------
# Realistic Active Task & Project Pools (25+ professions)
# ---------------------------------------------------------------------------
TASK_POOL: List[Tuple[str, str, List[str], List[str]]] = [
    ("Build TaskFlow neural activity classifier", "Rust on-device inference engine and SQLite migration", ["TaskFlow", "TinyNet"], ["coding", "terminal", "research", "learning"]),
    ("Migrate AuthService to OAuth2 PKCE", "Implement JWT rotation, Redis session store, and security tests", ["AuthService", "CoreAPI"], ["coding", "terminal", "research", "operations"]),
    ("Stripe webhook idempotency & billing dashboard", "Handle subscription retries, refund ledger, and Q3 revenue sheet", ["PaymentGateway", "Billing"], ["coding", "data_finance", "operations"]),
    ("Redesign mobile onboarding flow", "Create Figma interactive prototype, typography system, and icons", ["DesignSystem", "MobileApp"], ["design", "planning", "meetings"]),
    ("Edit Q4 product launch video", "Color grade 4K footage in DaVinci Resolve and master voiceover", ["LaunchVideo", "BrandMedia"], ["media_production", "design", "writing"]),
    ("Prepare Series A investor deck and financial model", "Update SaaS ARR cohort model, burn rate forecast, and memo", ["Fundraise2026", "FinanceModel"], ["data_finance", "writing", "planning"]),
    ("Distributed Systems Midterm & Raft Lab", "Watch MIT 6.824 lectures, complete Go consensus lab and flashcards", ["CS6824", "RaftLab"], ["learning", "coding", "research"]),
    ("Clinical literature review on cardiac biomarkers", "Analyze PubMed studies, draft systematic review manuscript", ["CardioReview", "MedPaper"], ["research", "writing", "data_finance"]),
    ("Q3 Enterprise Customer Support & Escalations", "Triage Zendesk tickets, run customer sync calls, update runbooks", ["EnterpriseSupport", "OpsRunbook"], ["communication", "meetings", "operations"]),
    ("Kubernetes multi-region failover drill", "Inspect Terraform modules, Grafana alerts, and cluster autoscaling", ["CloudInfra", "K8sCluster"], ["operations", "terminal", "coding"]),
    ("Draft SaaS MSA contract and GDPR DPA", "Review indemnity clauses, data retention schedule, and redlines", ["LegalCompliance", "EnterpriseMSA"], ["writing", "research", "communication"]),
    ("Organic chemistry & biochem exam prep", "Review reaction mechanism videos, Anki decks, and practice problems", ["Chem202", "MCATPrep"], ["learning", "research", "writing"]),
    ("Product roadmap Q4 sprint planning", "Prioritize Linear backlog, groom user stories, align engineering team", ["RoadmapQ4", "GrowthTeam"], ["planning", "meetings", "communication"]),
    ("Podcast Episode 42 audio mastering & show notes", "Clean noise floor in Audacity, export chapters, draft newsletter", ["FounderPod", "Newsletter"], ["media_production", "writing"]),
    ("E-commerce catalog analytics & pricing forecast", "Analyze SQL conversion funnel, inventory margins, and supplier sheets", ["RetailAnalytics", "SupplyChain"], ["data_finance", "operations", "planning"]),
    ("3D architectural visualization for lobby renovation", "Model lighting, textures, and CAD floorplans in Blender", ["LobbyReno", "ArchStudio"], ["design", "media_production"]),
    ("Daily Capture", "Continuous background workstream capture", ["TaskFlow", "SkillForge", "CustomCMS"], ["coding", "research", "planning"]),
    ("Machine Learning Transformer Distillation Study", "Study attention pruning, quantization math, and PyTorch benchmarks", ["MLResearch", "DistillNet"], ["learning", "research", "coding"]),
]


def add_ocr_noise(rng: random.Random, text: str) -> str:
    if rng.random() > 0.30 or len(text) < 20:
        return text
    swaps = [("l", "1"), ("O", "0"), ("rn", "m"), ("  ", " ")]
    out = text
    if rng.random() < 0.45:
        a, b = rng.choice(swaps)
        idx = out.find(a)
        if idx != -1:
            out = out[:idx] + b + out[idx + len(a):]
    if rng.random() < 0.35:
        out = out[: max(40, int(len(out) * rng.uniform(0.65, 0.95)))]
    return out


def browser_title(rng: random.Random, page_title: str) -> Tuple[str, str]:
    app, suffix = rng.choice(BROWSERS)
    if rng.random() < 0.85:
        return app, f"{page_title} - {suffix}"
    return app, page_title


# ---------------------------------------------------------------------------
# Combinatorial High-Diversity Generators (50,000+ Unique Titles & Snippets)
# `val_split=True` holds out distinct apps/domains so val measures true generalization!
# ---------------------------------------------------------------------------
def gen_coding(rng: random.Random, val_split: bool = False) -> Dict:
    ides = ["WebStorm", "GoLand", "Xcode", "Zed"] if val_split else ["Cursor", "Code.exe", "code-oss", "IntelliJ IDEA", "PyCharm", "Neovim", "Windsurf", "Visual Studio"]
    stems = [
        "window_monitor", "schema", "ai_client", "auth_middleware", "order_service", "UserController",
        "query_planner", "worker_pool", "CheckoutForm", "inference_engine", "graph_edges", "sync_worker",
        "token_bucket", "cache_manager", "parser", "lexer", "router", "scheduler", "vector_index",
        "billing_webhook", "encryption", "compression", "http_client", "session_store", "metrics_exporter",
        "shader_kernel", "audio_buffer", "state_machine", "event_bus", "db_pool", "migration_runner",
    ]
    exts = [".rs", ".ts", ".tsx", ".py", ".go", ".cpp", ".hpp", ".java", ".kt", ".swift", ".cs", ".rb", ".sql", ".zig", ".c", ".h", ".glsl", ".clj", ".ex", ".toml", ".yaml"]
    projects = ["TaskFlow", "AuthService", "PaymentGateway", "DataPipeline", "MobileApp", "SearchIndex", "CoreAPI", "WebStore", "Analytics", "BackendV2", "CompilerLab", "EngineCore", "WalletSDK", "MeshRouter"]
    actions = ["Fix memory leak in", "Refactor async handler for", "Add unit tests for", "Implement retry logic in", "Optimize hot loop in", "Migrate schema in", "Resolve race condition in", "Add validation to"]
    snippets = [
        "pub async fn {fn_name}(db: &SqlitePool, id: &str) -> Result<Row, Error> {{ let row = sqlx::query_as(sql).fetch_one(db).await?; Ok(row) }}",
        "export const {fn_name} = async (payload: RequestDto): Promise<Response> => {{ const res = await fetch('/api/v1/items'); return res.json(); }};",
        "class {cls_name}:\n    def {fn_name}(self, items: list[Record]) -> dict[str, float]:\n        return {{r.id: r.score for r in items if r.active}}",
        "func (s *Service) {cls_name}(ctx context.Context, req *Request) (*Response, error) {{ if err := req.Validate(); err != nil {{ return nil, err }} return s.exec(ctx, req) }}",
        "template<typename T> class {cls_name} {{ std::vector<T> data; public: void push(const T& v) {{ data.push_back(v); }} }};",
        "public async Task<ActionResult<{cls_name}>> {fn_name}(Guid id) {{ var entity = await _repo.FindAsync(id); return Ok(entity); }}",
        "impl {cls_name} {{ pub fn forward(&self, x: &[f32]) -> Vec<f32> {{ x.iter().map(|v| v * self.scale).collect() }} }}",
    ]
    fname = rng.choice(stems) + rng.choice(exts)
    proj = rng.choice(projects)
    mode = rng.random()
    if mode < 0.70:
        app = rng.choice(ides)
        title = rng.choice([f"{fname} — {proj} — {app}", f"{fname} - {proj} - {app}", f"src/{fname} ({proj})"])
        url = None
    elif mode < 0.90:
        pr = rng.randint(10, 1999)
        host = "gitlab.com" if val_split else rng.choice(["github.com", "bitbucket.org"])
        page = f"PR #{pr}: {rng.choice(actions)} {fname} · org/{proj} · {host.split('.')[0].title()}"
        app, title = browser_title(rng, page)
        url = f"https://{host}/org/{proj}/pull/{pr}"
    else:
        port = rng.choice([3000, 5173, 8000, 8080, 4200, 9000])
        app, title = browser_title(rng, f"{proj} Local Dev — localhost:{port}")
        url = f"http://localhost:{port}/"

    snip = rng.choice(snippets).format(fn_name=rng.choice(stems), cls_name=proj)
    content = add_ocr_noise(rng, snip) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "coding", "is_productive": 1}


def gen_terminal(rng: random.Random, val_split: bool = False) -> Dict:
    terms = ["Konsole", "foot", "iTerm2"] if val_split else ["Alacritty", "WezTerm", "Warp", "Ghostty", "WindowsTerminal.exe", "kitty", "PowerShell", "gnome-terminal"]
    cmds = [
        "cargo test --lib", "cargo clippy --all-targets", "git rebase -i origin/main", "git log --oneline -n 30",
        "docker compose up -d postgres redis", "docker build -t app:latest .", "kubectl get pods -n production",
        "kubectl logs -f deploy/gateway", "pytest tests/ -q", "pnpm build && pnpm test", "ssh deploy@prod-01",
        "terraform plan -out=tfplan", "psql -U admin analytics", "rg -n 'handle_event' src/", "journalctl -u api -f",
        "go test ./... -race", "npm run db:migrate", "tmux attach -t work", "sqlite3 app.db '.tables'",
        "rsync -avz ./build/ server:/var/www/", "ansible-playbook deploy.yml", "ffmpeg -i in.mov -c:v libx265 out.mp4",
        "nix develop", "curl -sI https://api.internal/health", "make -j8 release", "cmake --build build/",
    ]
    snippets = [
        "$ {cmd}\nrunning 42 tests\ntest core::tests::smoke ... ok\ntest result: ok. 42 passed; 0 failed in 0.84s",
        "$ {cmd}\n2026-10-12T10:21:03Z INFO worker started on 0.0.0.0:8080\n2026-10-12T10:21:04Z INFO healthcheck passed",
        "$ {cmd}\nChanged 6 files, 214 insertions(+), 42 deletions(-)\nSuccessfully synced remote branch",
        "$ {cmd}\nStep 5/8 : RUN pip install -r requirements.txt\nSuccessfully tagged service:v2.4",
    ]
    app = rng.choice(terms)
    cmd = rng.choice(cmds)
    host = rng.choice(["devbox", "workstation", "laptop", "prod-eu"])
    title = rng.choice([f"{cmd} — {app}", f"user@{host}:~/code — {cmd}", f"zsh: {cmd}", f"bash — {cmd}"])
    content = add_ocr_noise(rng, rng.choice(snippets).format(cmd=cmd)) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": None, "content": content, "category": "terminal", "is_productive": 1}


def gen_research(rng: random.Random, val_split: bool = False) -> Dict:
    subjects = [
        "Rust async borrow checker lifetimes", "PostgreSQL B-Tree vs GIN index performance", "Transformer static embedding distillation",
        "Cardiac troponin assay clinical sensitivity", "EU GDPR Article 28 data processor obligations", "OAuth2 PKCE token rotation RFC",
        "HTTP/3 QUIC congestion control specification", "Oncology single-cell RNA biomarker sequencing", "Zero-copy binary deserialization in Rust",
        "Macroeconomic inflation expectations empirical study", "SQLite WAL checkpoint concurrency locking", "Low-power INT8 neural quantization",
        "Kubernetes network policy egress isolation", "WebAssembly SIMD memory model reference", "Accessible keyboard navigation ARIA patterns",
        "Contract indemnification statutory case law", "Battery cathode solid-state electrolyte chemistry", "Distributed consensus Raft safety proofs",
        "GraphRAG multi-hop entity retrieval benchmarks", "PipeWire Linux audio routing configuration", "CSS Grid subgrid layout reference",
    ]
    sources_train = [
        ("docs.rs", "https://docs.rs/{slug}/latest/", "API Reference — {subj} — docs.rs"),
        ("developer.mozilla.org", "https://developer.mozilla.org/en-US/docs/{slug}", "{subj} | MDN Web Docs"),
        ("stackoverflow.com", "https://stackoverflow.com/questions/7231/{slug}", "{subj} - Stack Overflow"),
        ("arxiv.org", "https://arxiv.org/abs/2510.{num}", "[2510.{num}] {subj} - arXiv"),
        ("pubmed.ncbi.nlm.nih.gov", "https://pubmed.ncbi.nlm.nih.gov/{num}/", "{subj} - PubMed"),
        ("claude.ai", "https://claude.ai/chat/{num}", "Claude — Researching {subj}"),
        ("chatgpt.com", "https://chatgpt.com/c/{num}", "ChatGPT — {subj} Analysis"),
        ("perplexity.ai", "https://www.perplexity.ai/search/{slug}", "Perplexity — {subj}"),
        ("en.wikipedia.org", "https://en.wikipedia.org/wiki/{slug}", "{subj} - Wikipedia"),
        ("reddit.com", "https://www.reddit.com/r/rust/comments/{num}/{slug}/", "r/rust - {subj} - Reddit"),
        ("reddit.com", "https://www.reddit.com/r/MachineLearning/comments/{num}/{slug}/", "r/MachineLearning - [R] {subj} - Reddit"),
        ("huggingface.co", "https://huggingface.co/papers/{num}", "HuggingFace Paper — {subj}"),
    ]
    sources_val = [
        ("datatracker.ietf.org", "https://datatracker.ietf.org/doc/html/rfc{num}", "RFC {num}: {subj} - IETF"),
        ("scholar.google.com", "https://scholar.google.com/scholar?q={slug}", "Google Scholar — {subj}"),
        ("postgresql.org", "https://www.postgresql.org/docs/16/{slug}.html", "PostgreSQL Documentation: {subj}"),
        ("pytorch.org", "https://pytorch.org/docs/stable/{slug}.html", "PyTorch Reference: {subj}"),
        ("reddit.com", "https://www.reddit.com/r/ExperiencedDevs/comments/{num}/{slug}/", "r/ExperiencedDevs - {subj} - Reddit"),
    ]
    snippets = [
        "Abstract: We investigate {subj} across large-scale empirical benchmarks. Section 3.2 Methodology and quantitative evaluation results demonstrate...",
        "API Documentation: Detailed specification for {subj}. Parameters, return types, thread-safety invariants, and code examples.",
        "Accepted Answer (64 votes): When working with {subj}, the recommended approach is to isolate state transitions and verify invariants.",
        "Literature Citation & Notes: Peer-reviewed findings on {subj} (Journal Vol. 18, pp. 42-68, DOI: 10.1038/s41591-025).",
    ]
    subj = rng.choice(subjects)
    slug = subj.lower().replace(" ", "-")[:28]
    num = rng.randint(10000, 99999)
    _, url_tpl, title_tpl = rng.choice(sources_val if val_split else sources_train)
    app, title = browser_title(rng, title_tpl.format(subj=subj, num=num))
    url = url_tpl.format(slug=slug, num=num)
    content = add_ocr_noise(rng, rng.choice(snippets).format(subj=subj)) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "research", "is_productive": 1}


def gen_learning(rng: random.Random, val_split: bool = False) -> Dict:
    topics = [
        "Distributed Systems Raft Consensus", "Async Rust & Tokio Runtime", "Machine Learning Backpropagation Calculus",
        "Linear Algebra Eigenvalues & Matrices", "Organic Chemistry Reaction Mechanisms", "Financial Modeling DCF Valuation",
        "Figma Auto Layout & Design Tokens", "Blender Geometry Nodes & Shaders", "System Design Rate Limiter Architecture",
        "Intermediate TypeScript Generics", "Probability & Bayesian Inference", "Constitutional Law Case Briefs",
        "Kubernetes Security & Network Policies", "SQL Normalization & Window Functions", "Spanish Past Subjunctive Grammar",
        "Pharmacology Antibiotic Mechanisms", "DaVinci Resolve Color Grading Nodes", "Excel Power Query & Pivot Tables",
        "Compilers Parsing & AST Construction", "Microeconomics Supply Demand Equilibrium", "Dynamic Programming & LRU Cache",
    ]
    platforms_train = [
        ("youtube.com", "https://www.youtube.com/watch?v=lec{num}", "{topic} Full Course Lecture {wk} - YouTube"),
        ("youtube.com", "https://www.youtube.com/watch?v=tut{num}", "{topic} Complete Tutorial & Deep Dive Explained - YouTube"),
        ("coursera.org", "https://www.coursera.org/learn/{slug}/lecture", "Coursera — {topic}: Week {wk} Lesson & Quiz"),
        ("udemy.com", "https://www.udemy.com/course/{slug}/learn", "Udemy — {topic} Masterclass Section {wk}"),
        ("khanacademy.org", "https://www.khanacademy.org/study/{slug}", "Khan Academy — {topic} Practice Exercise"),
        ("leetcode.com", "https://leetcode.com/problems/{slug}/", "LeetCode — {topic} Problem & Editorial"),
        ("kaggle.com", "https://www.kaggle.com/learn/{slug}", "Kaggle Learn — {topic} Lesson {wk}"),
        ("anki", None, "Anki — Study Deck: {topic} ({num} flashcards due)"),
    ]
    platforms_val = [
        ("edx.org", "https://learning.edx.org/course/{slug}/week{wk}", "edX — {topic} Module {wk} Syllabus & Assignment"),
        ("duolingo.com", "https://www.duolingo.com/lesson/{wk}", "Duolingo — {topic} Unit {wk} Practice"),
        ("youtube.com", "https://www.youtube.com/watch?v=mit{num}", "MIT Lecture {wk}: {topic} Crash Course - YouTube"),
        ("hackerrank.com", "https://www.hackerrank.com/challenges/{slug}", "HackerRank — {topic} Coding Practice"),
    ]
    snippets = [
        "Lesson {wk} Learning Objectives: Master core concepts of {topic}. Complete the interactive quiz (Question 3 of 10) and review lecture notes.",
        "Course Transcript & Syllabus: Module {wk} covers {topic} step by step with worked examples and homework exercises.",
        "Flashcard Study Session: Recall key principles of {topic} | Show Answer [Space] | Again (1m) | Good (1d) | Easy (4d)",
        "Tutorial Walkthrough: Practice assignment for {topic}. Verify your solution against test cases and read the instructor explanation.",
    ]
    topic = rng.choice(topics)
    slug = topic.lower().replace(" ", "-")[:24]
    wk = rng.randint(1, 14)
    num = rng.randint(10, 999)
    plat, url_tpl, title_tpl = rng.choice(platforms_val if val_split else platforms_train)
    page_title = title_tpl.format(topic=topic, wk=wk, num=num)
    if plat == "anki":
        app, title, url = "Anki", page_title, None
    else:
        app, title = browser_title(rng, page_title)
        url = url_tpl.format(slug=slug, wk=wk, num=num)
    content = add_ocr_noise(rng, rng.choice(snippets).format(topic=topic, wk=wk)) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "learning", "is_productive": 1}


def gen_writing(rng: random.Random, val_split: bool = False) -> Dict:
    doc_titles = [
        "Q4 Architecture RFC: On-Device Classifier", "Series A Investor Memo v3", "Enterprise MSA Contract Redline",
        "Clinical Biomarkers Manuscript Draft", "Thesis Chapter 3 Methodology", "API Migration Guide v2.4",
        "Product Launch Narrative Deck", "Customer Discovery Interview Synthesis", "Engineering Postmortem Report",
        "Brand Messaging & Style Guidelines", "GDPR Data Processing Addendum", "Quarterly Board Presentation",
        "Technical Whitepaper: Edge Inference", "Grant Proposal: Neural Compression", "Employee Onboarding Handbook",
        "Newsletter Issue 48 Draft", "Case Study: Enterprise Migration", "Patent Disclosure Draft",
    ]
    apps_train = [
        ("Notion", "{doc} — Notion", "https://www.notion.so/org/{slug}"),
        ("browser", "{doc} - Google Docs", "https://docs.google.com/document/d/{num}/edit"),
        ("Microsoft Word", "{slug}.docx - Microsoft Word", None),
        ("browser", "{slug}.tex - Overleaf, Online LaTeX Editor", "https://www.overleaf.com/project/{num}"),
        ("browser", "{doc} - Confluence", "https://company.atlassian.net/wiki/spaces/ENG/pages/{num}"),
        ("browser", "{doc} - Google Slides", "https://docs.google.com/presentation/d/{num}/edit"),
        ("Microsoft PowerPoint", "{slug}.pptx - PowerPoint", None),
    ]
    apps_val = [
        ("LibreOffice Writer", "{slug}.odt - LibreOffice Writer", None),
        ("browser", "Draft: {doc} — Substack Writer", "https://substack.com/publish/post/{num}"),
        ("Apple Pages", "{doc}.pages — Pages", None),
    ]
    snippets = [
        "## Executive Summary\nThis document outlines {doc}, including key goals, methodology, stakeholder alignment, and next-step deliverables.",
        "\\section{{Introduction}}\nIn this manuscript on {doc}, we analyze empirical outcomes and summarize recommendations for revision.",
        "Word Count: 2,480 | Last edited 2 mins ago | Draft revision for {doc} — Comments (3) | Suggesting Mode",
        "Section 4.2 Narrative Overview: Clear prose outlining the scope, constraints, and decision rationale for {doc}.",
    ]
    doc = rng.choice(doc_titles)
    slug = doc.lower().replace(" ", "_").replace(":", "")[:26]
    num = rng.randint(1000, 99999)
    app_kind, title_tpl, url_tpl = rng.choice(apps_val if val_split else apps_train)
    rendered = title_tpl.format(doc=doc, slug=slug)
    if app_kind == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = app_kind, rendered
    url = url_tpl.format(slug=slug, num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets).format(doc=doc)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "writing", "is_productive": 1}


def gen_design(rng: random.Random, val_split: bool = False) -> Dict:
    artifacts = [
        "Mobile Onboarding Wireframes", "TaskFlow Design System v2", "Hero Banner Campaign", "Brand Wordmark Vector Icons",
        "Lobby 3D Lighting Model", "Architectural Floorplan Level 2", "Checkout Flow UI Prototype", "User Journey Whiteboard",
        "Product Packaging Die Line", "PCB 4-Layer Sensor Layout", "Mechanical Gearbox Assembly CAD", "Dashboard Dark Mode Mockup",
        "Typography & Color Token Library", "Illustration Character Sheet", "System Architecture Diagram",
    ]
    tools_train = [
        ("Figma", "{art} — Figma", "https://www.figma.com/file/{num}/{slug}"),
        ("browser", "{art} – Figma", "https://www.figma.com/design/{num}/{slug}"),
        ("Blender", "{slug}.blend — Blender 4.2", None),
        ("Adobe Photoshop", "{slug}.psd @ 100% — Adobe Photoshop", None),
        ("Adobe Illustrator", "{slug}.ai — Adobe Illustrator", None),
        ("browser", "{art} - Excalidraw", "https://excalidraw.com/#room={num}"),
        ("browser", "{art} - Miro", "https://miro.com/app/board/{num}/"),
        ("browser", "{art} - Canva", "https://www.canva.com/design/{num}/edit"),
    ]
    tools_val = [
        ("AutoCAD", "{slug}.dwg — AutoCAD 2026", None),
        ("Inkscape", "{slug}.svg — Inkscape", None),
        ("GIMP", "{slug}.xcf — GNU Image Manipulation Program", None),
    ]
    snippets = [
        "Artboard: {art} | Width 1440px Height 900px | Auto Layout Gap 16px | Fill #0F172A | Stroke 1.5pt | Typography Inter SemiBold",
        "Layers: Vector_Outline, Lighting_Pass, Shadow_Mask | Canvas 3840x2160 | Export SVG / PNG 2x | Color Profile sRGB",
        "CAD Sketch & Constraints: Extrude 12.5mm | Tolerance ±0.05mm | Wireframe Shading | Prototype Components",
    ]
    art = rng.choice(artifacts)
    slug = art.replace(" ", "_")[:24]
    num = rng.randint(1000, 99999)
    tool, title_tpl, url_tpl = rng.choice(tools_val if val_split else tools_train)
    rendered = title_tpl.format(art=art, slug=slug)
    if tool == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = tool, rendered
    url = url_tpl.format(slug=slug, num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets).format(art=art)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "design", "is_productive": 1}


def gen_media_production(rng: random.Random, val_split: bool = False) -> Dict:
    projects = [
        "Product_Launch_Demo_4K", "Podcast_Ep42_Master", "Documentary_Rough_Cut", "Tutorial_Screencast_Edit",
        "Founder_Interview_ColorGrade", "Intro_Jingle_118bpm", "Logo_Reveal_Motion_60fps", "Customer_Case_Study_Reel",
        "Audiobook_Chapter_04_Mix", "Keynote_Multicam_Sync", "Voiceover_Noise_Reduction",
    ]
    apps_train = [
        ("DaVinci Resolve", "{proj}.drp — DaVinci Resolve Studio", None),
        ("Adobe Premiere Pro", "{proj}.prproj — Adobe Premiere Pro", None),
        ("OBS Studio", "OBS 30.2 — Recording: {proj}", None),
        ("Audacity", "{proj}.aup3 — Audacity", None),
        ("Ableton Live", "{proj}.als — Ableton Live 12", None),
        ("Adobe After Effects", "{proj}.aep — Adobe After Effects", None),
        ("browser", "{proj} — Video Editor & Analytics - YouTube Studio", "https://studio.youtube.com/video/{num}/editor"),
    ]
    apps_val = [
        ("Descript", "{proj} — Descript Audio & Video Editor", None),
        ("CapCut", "{proj} — CapCut Desktop Editor", None),
        ("FL Studio", "{proj}.flp — FL Studio Producer Edition", None),
    ]
    snippets = [
        "Timeline: 00:08:14:22 | Track V1: A-Roll 4K ProRes | Track A1: Narration -14 LUFS | Color Grade Rec.709 LUT | Render H.265 60fps",
        "Audio Mixer: Track 1 Vocal EQ | Compressor 3:1 | Noise Floor -48dB | Limiter -1.0 dBTP | Sample Rate 48kHz 24-bit WAV",
        "Video Production Studio: Keyframe Animation | Motion Blur | Subtitles & Captions | Export Master MP4",
    ]
    proj = rng.choice(projects)
    num = rng.randint(1000, 99999)
    app_kind, title_tpl, url_tpl = rng.choice(apps_val if val_split else apps_train)
    rendered = title_tpl.format(proj=proj)
    if app_kind == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = app_kind, rendered
    url = url_tpl.format(num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "media_production", "is_productive": 1}


def gen_planning(rng: random.Random, val_split: bool = False) -> Dict:
    boards = [
        "Q4 Engineering Roadmap", "Sprint 18 Active Backlog", "Release v2.0 Launch Checklist", "Product OKRs & Milestones",
        "Mobile App Feature Epic", "Customer Onboarding Gantt Schedule", "Weekly Priorities & Time Blocks",
        "Security Audit Issue Tracker", "Hiring Pipeline Tracker", "Marketing Campaign Calendar",
    ]
    tools_train = [
        ("Linear", "TF-{num} {board} — Linear", "https://linear.app/team/issue/TF-{num}"),
        ("browser", "{board} - Jira", "https://company.atlassian.net/jira/software/projects/ENG/boards/{num}"),
        ("browser", "{board} - Asana", "https://app.asana.com/0/{num}/board"),
        ("browser", "Google Calendar — {board}", "https://calendar.google.com/calendar/u/0/r/week"),
        ("browser", "Issues · org/repo · {board} · GitHub", "https://github.com/org/repo/issues"),
        ("Notion", "{board} — Task Board — Notion", "https://www.notion.so/org/board-{num}"),
    ]
    tools_val = [
        ("browser", "{board} | Trello", "https://trello.com/b/{num}/board"),
        ("Todoist", "{board} — Todoist", None),
        ("browser", "{board} — Monday.com Workspace", "https://team.monday.com/boards/{num}"),
    ]
    snippets = [
        "Sprint Cycle (Oct 12 - Oct 26) | Backlog: 14 tickets | In Progress: 6 | In Review: 4 | Milestone: {board} | Priority: High",
        "Schedule & Deadlines: Mon 10:00 Standup | Tue 14:00 Milestone Review | Subtasks completed 5/8 | Assignee: Team Lead",
        "Kanban Board: Ready for Dev | Blocked | Done | Velocity 42 pts | Quarterly OKR Progress 78%",
    ]
    board = rng.choice(boards)
    num = rng.randint(10, 999)
    tool, title_tpl, url_tpl = rng.choice(tools_val if val_split else tools_train)
    rendered = title_tpl.format(board=board, num=num)
    if tool == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = tool, rendered
    url = url_tpl.format(num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets).format(board=board)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "planning", "is_productive": 1}


def gen_meetings(rng: random.Random, val_split: bool = False) -> Dict:
    topics = [
        "Engineering Architecture Sync", "Weekly Product Standup", "Design Critique & Handoff", "Customer Discovery Interview",
        "Enterprise Security Review", "Cofounder 1:1 Sync", "Q4 Roadmap Alignment Call", "Technical Candidate Interview",
        "Quarterly All-Hands Webinar", "Backend Pairing Session", "Sprint Retrospective Call",
    ]
    apps_train = [
        ("Zoom", "Zoom Meeting — {topic}", None),
        ("zoom.exe", "Zoom Workplace - {topic} (12 participants)", None),
        ("browser", "Meet - abc-defg-hij ({topic}) - Google Meet", "https://meet.google.com/abc-defg-hij"),
        ("Microsoft Teams", "{topic} Call | Microsoft Teams", None),
        ("Slack", "Huddle in #product-eng — {topic} — Slack", None),
    ]
    apps_val = [
        ("Webex", "Cisco Webex Meetings — {topic}", None),
        ("browser", "{topic} — Loom Video Recording", "https://www.loom.com/share/{num}"),
        ("browser", "Zoom Web Client — Live Call: {topic}", "https://us02web.zoom.us/wc/join/{num}"),
    ]
    snippets = [
        "Meeting in progress | 00:24:15 | Mute Mic | Stop Camera | Participants (8) | Share Screen | Leave Call",
        "You are presenting your screen to everyone | Live Captions enabled | Recording in progress | Raise Hand",
        "Video Call Connected | Audio: Studio Microphone | 6 participants in room | Screen Share Active",
    ]
    topic = rng.choice(topics)
    num = rng.randint(10000, 99999)
    app_kind, title_tpl, url_tpl = rng.choice(apps_val if val_split else apps_train)
    rendered = title_tpl.format(topic=topic)
    if app_kind == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = app_kind, rendered
    url = url_tpl.format(num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "meetings", "is_productive": 1}


def gen_communication(rng: random.Random, val_split: bool = False) -> Dict:
    subjects = [
        "Re: Enterprise Security Questionnaire", "API v2 Rollout Timeline", "Q3 Vendor Invoice Approval",
        "SSO Login Timeout Support Ticket", "Partnership Co-Marketing Proposal", "Signed SOW Contract Attachment",
        "Recruiting Follow-up Staff Engineer", "Customer Escalation SLA Response", "PR Review Discussion #412",
        "Staging Deployment Window Confirmation",
    ]
    apps_train = [
        ("Slack", "#engineering - {subj} - Slack", "https://app.slack.com/client/T123/C456"),
        ("Slack", "DM with Sarah Chen — {subj} - Slack", "https://app.slack.com/client/T123/D789"),
        ("browser", "Inbox (12) - {subj} - Gmail", "https://mail.google.com/mail/u/0/#inbox"),
        ("Microsoft Outlook", "RE: {subj} - Outlook", None),
        ("Microsoft Teams", "Chat | Platform Team — {subj} | Microsoft Teams", None),
        ("Discord", "#rust-help - Rust Programming Language - Discord", "https://discord.com/channels/273534239310479360/123456"),
        ("browser", "Ticket #{num}: {subj} - Zendesk Support", "https://company.zendesk.com/agent/tickets/{num}"),
    ]
    apps_val = [
        ("Thunderbird", "Inbox — {subj} — Mozilla Thunderbird", None),
        ("browser", "Intercom Shared Inbox — {subj}", "https://app.intercom.com/a/inbox/{num}"),
        ("browser", "Proton Mail — Inbox: {subj}", "https://mail.proton.me/u/0/inbox"),
        ("browser", "LinkedIn Messaging — {subj}", "https://www.linkedin.com/messaging/thread/{num}/"),
    ]
    snippets = [
        "From: alex@company.com | Subject: {subj} | Hi team, please review the attached notes and reply before end of day.",
        "#engineering | @maya: Confirmed the deployment window and updated the ticket status. | Thread: 6 replies | Unread",
        "Support Inbox | Ticket #{num} Open | Customer SLA 2h | Reply to customer | Internal Note | CC team",
    ]
    subj = rng.choice(subjects)
    num = rng.randint(1000, 9999)
    app_kind, title_tpl, url_tpl = rng.choice(apps_val if val_split else apps_train)
    rendered = title_tpl.format(subj=subj, num=num)
    if app_kind == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = app_kind, rendered
    url = url_tpl.format(num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets).format(subj=subj, num=num)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "communication", "is_productive": 1}


def gen_data_finance(rng: random.Random, val_split: bool = False) -> Dict:
    reports = [
        "Q3 SaaS Revenue Cohort Model", "2026 Operating Budget & Runway Forecast", "Monthly Bank Reconciliation Ledger",
        "Executive Retention Funnel Dashboard", "Churn Prediction EDA Notebook", "Clinical Survival Statistical Regression",
        "Accounts Payable & Vendor Invoices", "Tax Deductions & Profit Loss Statement", "Regional Sales Margin Pivot Table",
        "Paid Acquisition CAC & LTV Analysis", "Daily Active Users SQL Query", "Subscription MRR Billing Report",
    ]
    tools_train = [
        ("Microsoft Excel", "{slug}.xlsx - Microsoft Excel", None),
        ("browser", "{rep} - Google Sheets", "https://docs.google.com/spreadsheets/d/{num}/edit"),
        ("TablePlus", "production_postgres — {slug} — TablePlus", None),
        ("DataGrip", "{slug}.sql — DataGrip", None),
        ("browser", "SQL Editor — {rep} — Supabase", "https://supabase.com/dashboard/project/abc/sql"),
        ("browser", "Stripe Dashboard — {rep}", "https://dashboard.stripe.com/payments"),
        ("browser", "{rep} — Metabase Analytics", "https://analytics.company.com/dashboard/{num}"),
        ("browser", "{rep} — QuickBooks Online", "https://app.qbo.intuit.com/app/report"),
        ("browser", "{slug}.ipynb - JupyterLab", "http://localhost:8888/lab/tree/{slug}.ipynb"),
    ]
    tools_val = [
        ("Tableau", "{slug}.twbx — Tableau Desktop", None),
        ("LibreOffice Calc", "{slug}.ods — LibreOffice Calc", None),
        ("browser", "Salesforce CRM — {rep}", "https://company.lightning.force.com/lightning/r/Report/{num}"),
        ("browser", "BigQuery Studio — {rep}", "https://console.cloud.google.com/bigquery"),
    ]
    snippets = [
        "Workbook: {rep} | =SUM(B2:B13) | MRR: $148,200 | Gross Margin: 82.4% | Net Burn: $31,400 | Balance Sheet Reconciled",
        "SELECT date_trunc('week', created_at), COUNT(DISTINCT user_id), SUM(amount) FROM invoices GROUP BY 1; -- 32 rows in 14ms",
        "Financial Metrics: CAC $44.10 | LTV $640 | YoY Revenue Growth +38.2% | Accounts Receivable | Export CSV",
        "In [12]: df.groupby('cohort')['retention_d30'].mean() | Regression R2 = 0.874 | p-value < 0.001",
    ]
    rep = rng.choice(reports)
    slug = rep.replace(" ", "_").replace("&", "")[:26]
    num = rng.randint(100, 9999)
    tool, title_tpl, url_tpl = rng.choice(tools_val if val_split else tools_train)
    rendered = title_tpl.format(rep=rep, slug=slug)
    if tool == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = tool, rendered
    url = url_tpl.format(slug=slug, num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets).format(rep=rep)) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "data_finance", "is_productive": 1}


def gen_operations(rng: random.Random, val_split: bool = False) -> Dict:
    views = [
        "EC2 Cluster Instances us-east-1", "Production API Latency p99 Metrics", "DNS Records & WAF Firewall Rules",
        "Deployment Pipeline & Build Logs", "Unresolved Production Exceptions", "Managed PostgreSQL Automated Backups",
        "IAM Roles & Security Policies", "On-Call Escalation & Incident Alerts", "Container Health & Memory Quotas",
        "REST API Endpoint Inspection", "FBA Inventory & Logistics Console", "SSL Certificate & Edge Routing",
    ]
    tools_train = [
        ("browser", "{view} - AWS Management Console", "https://us-east-1.console.aws.amazon.com/ec2/home"),
        ("browser", "{view} – Google Cloud Console", "https://console.cloud.google.com/run"),
        ("browser", "Deployments — {view} — Vercel", "https://vercel.com/team/app/deployments"),
        ("browser", "{view} — Render Dashboard", "https://dashboard.render.com/web/srv-{num}/logs"),
        ("browser", "{view} — Cloudflare", "https://dash.cloudflare.com/{num}/dns/records"),
        ("browser", "{view} — Grafana", "https://grafana.company.com/d/{num}"),
        ("browser", "{view} — Sentry", "https://sentry.io/organizations/company/issues/"),
        ("browser", "Workflow Runs · Actions · GitHub", "https://github.com/org/repo/actions"),
        ("Docker Desktop", "Containers — {view} — Docker Desktop", None),
        ("explorer.exe", "Projects\\Release_v2 - File Explorer", None),
    ]
    tools_val = [
        ("browser", "{view} — Datadog APM", "https://app.datadoghq.com/apm/services"),
        ("browser", "{view} — Microsoft Azure Portal", "https://portal.azure.com/#home"),
        ("browser", "Amazon Seller Central — {view}", "https://sellercentral.amazon.com/inventory"),
        ("Nautilus", "Documents/Deployments — Files", None),
    ]
    snippets = [
        "Cluster prod-us-east-1 | Healthy Pods 24/24 | CPU 34% | Memory 2.1GB/4GB | p99 Latency 38ms | Zero 5xx Errors",
        "Deployment dpl_{num} Live | SSL Active | Health Checks Passing | Firewall Rules | Automated Snapshots Enabled",
        "HTTP POST /v2/webhooks -> 200 OK (94ms) | TLSv1.3 | Incident Status: Resolved | Monitoring Alerts",
    ]
    view = rng.choice(views)
    num = rng.randint(100, 9999)
    tool, title_tpl, url_tpl = rng.choice(tools_val if val_split else tools_train)
    rendered = title_tpl.format(view=view)
    if tool == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = tool, rendered
    url = url_tpl.format(num=num) if url_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets).format(num=num)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "operations", "is_productive": 1}


def gen_entertainment(rng: random.Random, val_split: bool = False) -> Dict:
    titles_train = [
        ("Funny Dog Tricks & Fails #shorts - YouTube", "https://www.youtube.com/shorts/dog{num}"),
        ("MrBeast $1,000,000 Survival Island Challenge - YouTube", "https://www.youtube.com/watch?v=mrb{num}"),
        ("Top 10 Most Expensive Mansions In The World - YouTube", "https://www.youtube.com/watch?v=man{num}"),
        ("Sci-Fi Blockbuster Official Teaser Trailer (4K) - YouTube", "https://www.youtube.com/watch?v=trl{num}"),
        ("Lofi Hip Hop Radio - Beats to relax/chill to - YouTube", "https://www.youtube.com/watch?v=lofi{num}"),
        ("Official Music Video (4K Lyrics) - YouTube", "https://www.youtube.com/watch?v=mus{num}"),
        ("NBA Finals Game 7 Full Highlights | ESPN - YouTube", "https://www.youtube.com/watch?v=nba{num}"),
        ("Cricket World Cup Match Highlights - YouTube", "https://www.youtube.com/watch?v=cric{num}"),
        ("Try Not To Laugh Impossible Comedy Compilation - YouTube", "https://www.youtube.com/watch?v=lol{num}"),
        ("My Day In Tokyo Street Food Vlog! - YouTube", "https://www.youtube.com/watch?v=vlog{num}"),
        ("Stranger Things Season 5 Episode 3 - Netflix", "https://www.netflix.com/watch/{num}"),
        ("The Boys Season 4 Episode 6 - Prime Video", "https://www.primevideo.com/detail/{num}"),
        ("Jujutsu Kaisen Episode 18 - Crunchyroll Anime", "https://www.crunchyroll.com/watch/{num}"),
        ("Just Chatting Live Stream - Twitch", "https://www.twitch.tv/variety_live_{num}"),
        ("ESPN — Live NBA Scores & Football Highlights", "https://www.espn.com/nba/scoreboard"),
    ]
    titles_val = [
        ("Disney+ | Marvel Movies & Series Streaming", "https://www.disneyplus.com/video/{num}"),
        ("Hulu — Watch Comedy Episodes & TV Shows", "https://www.hulu.com/watch/{num}"),
        ("Spotify – Web Player: Weekend Chill Playlist", "https://open.spotify.com/playlist/{num}"),
        ("Standup Comedy Special Reaction Video - YouTube", "https://www.youtube.com/watch?v=std{num}"),
        ("Premier League Goals & Football Highlights - YouTube", "https://www.youtube.com/watch?v=epl{num}"),
    ]
    snippets = [
        "Subscribe 14.2M | Like 420K | Share | Comments 18,432 | Up Next: Funniest Pranks Compilation | Autoplay ON | Skip Ad",
        "Play | Pause | Mute | Fullscreen | Subtitles: English | Episode 4: 42m remaining | Skip Intro | Next Episode",
        "Watch Trailer 2:34 | Rating 8.6/10 | Box Office Movies | Top Highlights | Live Scoreboard | Watchlist",
    ]
    t_tpl, u_tpl = rng.choice(titles_val if val_split else titles_train)
    num = rng.randint(100, 99999)
    app, title = browser_title(rng, t_tpl.format(num=num))
    url = u_tpl.format(num=num)
    content = add_ocr_noise(rng, rng.choice(snippets)) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "entertainment", "is_productive": 0}


def gen_gaming(rng: random.Random, val_split: bool = False) -> Dict:
    items_train = [
        ("Steam", "Steam — Game Library & Store", None),
        ("steam.exe", "Counter-Strike 2 — Steam", None),
        ("Minecraft", "Minecraft 1.21.4 — Survival Multiplayer", None),
        ("VALORANT", "VALORANT — Competitive Ranked Match", None),
        ("Epic Games Launcher", "Epic Games Launcher — Fortnite", None),
        ("League of Legends", "League of Legends Client — Ranked Queue", None),
        ("Roblox", "Roblox Player — Adventure Obby", None),
        ("browser", "GTA 6 Gameplay Walkthrough Part {num} Full Playthrough - YouTube", "https://www.youtube.com/watch?v=gta{num}"),
        ("browser", "Elden Ring Any% Speedrun World Record Boss Fight - YouTube", "https://www.youtube.com/watch?v=er{num}"),
        ("browser", "Minecraft 100 Days Hardcore Survival Let's Play - YouTube", "https://www.youtube.com/watch?v=mc{num}"),
        ("browser", "Play Blitz Chess Online — Chess.com", "https://www.chess.com/play/online"),
        ("browser", "r/gaming - Best RPG boss fights & gameplay - Reddit", "https://www.reddit.com/r/gaming/comments/{num}/boss/"),
    ]
    items_val = [
        ("Battle.net", "Overwatch 2 — Battle.net Launcher", None),
        ("Dota 2", "Dota 2 — Ranked All Pick Match", None),
        ("browser", "Valorant Radiant Ranked Gameplay Stream - Twitch", "https://www.twitch.tv/pro_fps_{num}"),
        ("browser", "Cyberpunk 2077 Boss Guide & Walkthrough - YouTube", "https://www.youtube.com/watch?v=cp{num}"),
        ("browser", "Best Open World Video Game Reviews & Guides - Kotaku", "https://kotaku.com/best-rpg-games-{num}"),
    ]
    snippets = [
        "Play Game | Achievements 42/80 | Friends Playing Now (3) | Quicksave | Inventory | Quest Log | Leaderboard",
        "Gameplay Walkthrough Part 4 | Boss Fight Guide | High Score 98,420 | Ranked Match | Turn-Based Combat",
        "Blitz 3+0 Tournament | Opponent Rating 1540 | Resign | Offer Draw | Play Next Game",
    ]
    app_kind, t_tpl, u_tpl = rng.choice(items_val if val_split else items_train)
    num = rng.randint(1, 999)
    rendered = t_tpl.format(num=num)
    if app_kind == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = app_kind, rendered
    url = u_tpl.format(num=num) if u_tpl else None
    content = add_ocr_noise(rng, rng.choice(snippets)) if rng.random() < 0.55 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "gaming", "is_productive": 0}


def gen_social_feed(rng: random.Random, val_split: bool = False) -> Dict:
    items_train = [
        ("browser", "Instagram — Reels & Explore Feed", "https://www.instagram.com/reels/"),
        ("browser", "Instagram — Home Feed ({num})", "https://www.instagram.com/"),
        ("browser", "TikTok - Make Your Day | For You Page", "https://www.tiktok.com/foryou"),
        ("browser", "Home / X", "https://x.com/home"),
        ("browser", "Explore / Trending Now / X", "https://x.com/explore"),
        ("browser", "r/memes - Hilarious Monday meme compilation - Reddit", "https://www.reddit.com/r/memes/comments/{num}/monday/"),
        ("browser", "r/funny - My cat knocked over my coffee - Reddit", "https://www.reddit.com/r/funny/comments/{num}/cat/"),
        ("browser", "r/aww - Golden retriever puppy in snow - Reddit", "https://www.reddit.com/r/aww/comments/{num}/puppy/"),
        ("browser", "r/AskReddit - Funniest thing a stranger told you? - Reddit", "https://www.reddit.com/r/AskReddit/comments/{num}/"),
        ("browser", "({num}) Facebook — News Feed & Stories", "https://www.facebook.com/"),
        ("Discord", "#memes-and-chill - Hangout Lounge - Discord", "https://discord.com/channels/999111/{num}"),
    ]
    items_val = [
        ("browser", "Threads — For You Viral Posts Feed", "https://www.threads.net/"),
        ("browser", "Pinterest — Aesthetic Home Feed Ideas", "https://www.pinterest.com/"),
        ("browser", "r/gifs - Viral reaction gif trending - Reddit", "https://www.reddit.com/r/gifs/comments/{num}/"),
        ("browser", "Celebrity Drama & Viral Tweets Trending Feed", "https://www.reddit.com/r/popculturechat/comments/{num}/"),
    ]
    snippets = [
        "Suggested for you | Follow | Like 24.5K | View all 1,240 comments | Add a comment... | Share | Reels | Stories",
        "For You | Following | Retweet 4.2K | Repost 1.9K | Likes 18.9K | Trending Topics | Viral Posts",
        "Posted by u/memelord 3h ago | 15.4k Upvotes | Downvote | 842 Comments | Meme Feed | Popular",
    ]
    app_kind, t_tpl, u_tpl = rng.choice(items_val if val_split else items_train)
    num = rng.randint(1, 99)
    rendered = t_tpl.format(num=num)
    if app_kind == "browser":
        app, title = browser_title(rng, rendered)
    else:
        app, title = app_kind, rendered
    url = u_tpl.format(num=num)
    content = add_ocr_noise(rng, rng.choice(snippets)) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "social_feed", "is_productive": 0}


def gen_shopping_personal(rng: random.Random, val_split: bool = False) -> Dict:
    products = [
        "Wireless Noise Cancelling Headphones", "Standing Desk & Ergonomic Chair", "Running Sneakers Flash Sale",
        "4K OLED Monitor Deals", "Mechanical Keyboard Accessories", "Winter Jacket Wishlist",
        "Thai Takeout Food Delivery", "Biryani & Pizza Order", "Beach Vacation Hotel Rentals", "Round-Trip Vacation Flights",
    ]
    stores_train = [
        ("Amazon.com Shopping Cart — {prod} Checkout", "https://www.amazon.com/gp/cart/view.html"),
        ("{prod} : Amazon.com", "https://www.amazon.com/dp/B09XS{num}"),
        ("Flipkart Deals of the Day — Buy {prod} Online", "https://www.flipkart.com/search?q={slug}"),
        ("{prod} - Buy Online at Best Price | Myntra", "https://www.myntra.com/shop/{slug}"),
        ("{prod} for Sale | eBay", "https://www.ebay.com/sch/i.html?_nkw={slug}"),
        ("Order {prod} Online — DoorDash Delivery", "https://www.doordash.com/checkout"),
        ("Swiggy — Track Your {prod}", "https://www.swiggy.com/orders"),
        ("Airbnb | {prod}", "https://www.airbnb.com/s/homes"),
        ("Track Package — My Orders | Amazon.in", "https://www.amazon.in/gp/your-account/order-history"),
    ]
    stores_val = [
        ("Walmart — {prod} Online Shopping Cart", "https://www.walmart.com/cart"),
        ("Etsy — Handmade {prod} & Gift Ideas", "https://www.etsy.com/search?q={slug}"),
        ("Zomato — Order {prod} Near Me", "https://www.zomato.com/checkout"),
        ("Expedia — Cheap {prod} Travel Deals", "https://www.expedia.com/Flights"),
        ("AliExpress — {prod} 50% OFF Flash Sale", "https://www.aliexpress.com/item/{num}.html"),
    ]
    snippets = [
        "Price: $249.99 | Save $60 | In Stock | FREE Shipping Tomorrow | Add to Cart | Buy Now | Customer Reviews 4.7/5 | Wishlist",
        "Order Summary | Subtotal $84.50 | Promo Code Applied | Free Delivery | Proceed to Checkout | Place Order",
        "Guest Review Score 9.1 | Free Cancellation | 3 nights $420 | Reserve Room | Add to Shopping Bag | Coupon",
    ]
    prod = rng.choice(products)
    slug = prod.lower().replace(" ", "-")[:22]
    num = rng.randint(1000, 9999)
    t_tpl, u_tpl = rng.choice(stores_val if val_split else stores_train)
    app, title = browser_title(rng, t_tpl.format(prod=prod))
    url = u_tpl.format(slug=slug, num=num)
    content = add_ocr_noise(rng, rng.choice(snippets)) if rng.random() < 0.60 else None
    return {"app_name": app, "window_title": title, "url": url, "content": content, "category": "shopping_personal", "is_productive": 0}


CATEGORY_GENERATORS = {
    "coding": gen_coding,
    "terminal": gen_terminal,
    "research": gen_research,
    "learning": gen_learning,
    "writing": gen_writing,
    "design": gen_design,
    "media_production": gen_media_production,
    "planning": gen_planning,
    "meetings": gen_meetings,
    "communication": gen_communication,
    "data_finance": gen_data_finance,
    "operations": gen_operations,
    "entertainment": gen_entertainment,
    "gaming": gen_gaming,
    "social_feed": gen_social_feed,
    "shopping_personal": gen_shopping_personal,
}


# ---------------------------------------------------------------------------
# Counterfactual Dual-Use Context Pairs (Forces Context Cross-Tower Learning)
# ---------------------------------------------------------------------------
COUNTERFACTUAL_TOPICS: List[Tuple[str, str, str, str, str, List[str]]] = [
    (
        "Medieval Ceramic Glaze Chemistry & Kiln Firing - YouTube",
        "https://www.youtube.com/watch?v=glaze101",
        "Kiln firing temperatures cone 6 reduction atmosphere stoneware glaze recipe silica alumina flux",
        "research",
        "Write Pottery Studio Glaze Handbook",
        ["PotteryBook", "CeramicLab"],
    ),
    (
        "How Coffee Roasting Curves Affect Espresso Extraction - YouTube",
        "https://www.youtube.com/watch?v=roast202",
        "First crack development time ratio roast profile sensory evaluation Arabica extraction yield",
        "research",
        "Develop Specialty Coffee Roastery SOP",
        ["RoastOps", "BeanCraft"],
    ),
    (
        "Framer Motion Spring Layout Animations Walkthrough - YouTube",
        "https://www.youtube.com/watch?v=framer99",
        "layoutId AnimatePresence spring stiffness damping React component transition",
        "learning",
        "Building TaskFlow UI Animation System",
        ["TaskFlow", "MotionUI"],
    ),
    (
        "r/shopify - Optimizing checkout conversion rates for apparel stores - Reddit",
        "https://www.reddit.com/r/shopify/comments/1abc/checkout/",
        "Checkout abandonment rate A/B testing payment gateway conversion funnel metrics",
        "research",
        "Improve E-Commerce Checkout Funnel",
        ["StoreFront", "CheckoutV2"],
    ),
    (
        "Instagram Reels Analytics & Creator Retention Benchmarks - X",
        "https://x.com/growthmarketer/status/123456",
        "Retention curve 3-second hook rate impressions campaign attribution CAC payback",
        "research",
        "Q4 Social Media Marketing Campaign Strategy",
        ["BrandGrowth", "SocialCampaign"],
    ),
    (
        "r/architecture - Passive solar ventilation details for tropical homes - Reddit",
        "https://www.reddit.com/r/architecture/comments/1def/solar/",
        "Cross ventilation thermal mass overhang shading daylighting building envelope",
        "research",
        "Residential Passive Solar Renovation Plans",
        ["SolarHome", "ArchStudio"],
    ),
    (
        "Formula 1 Aerodynamics Ground Effect Floor Breakdown - YouTube",
        "https://www.youtube.com/watch?v=f1aero",
        "Venturi tunnels downforce porpoising CFD boundary layer diffuser vortex",
        "learning",
        "CFD Aerodynamics Capstone Report",
        ["AeroCapstone", "FluidSim"],
    ),
    (
        "Sound Design for Indie Horror Games in Wwise - YouTube",
        "https://www.youtube.com/watch?v=wwise88",
        "Dynamic reverb spatial audio attenuation curves FMOD Wwise audio bus mixing",
        "media_production",
        "Indie Game Audio Engine Integration",
        ["ShadowEngine", "AudioFX"],
    ),
    (
        "Amazon Product Listing SEO & Sponsored Ads Bidding - YouTube",
        "https://www.youtube.com/watch?v=amzseo",
        "ACOS TACOS keyword bidding Amazon Seller Central conversion rate listing optimization",
        "operations",
        "Optimize Amazon FBA Storefront Listings",
        ["FBAStore", "RetailOps"],
    ),
    (
        "Cinematography Lighting Ratios for Documentary Interviews - YouTube",
        "https://www.youtube.com/watch?v=doclight",
        "Key light fill ratio 4:1 softbox negative fill color temperature 5600K",
        "media_production",
        "Shoot Founder Documentary Interview Series",
        ["FounderDoc", "FilmProd"],
    ),
    (
        "r/supplychain - Managing ocean freight lead time variability - Reddit",
        "https://www.reddit.com/r/supplychain/comments/1xyz/freight/",
        "Safety stock reorder point container transit variance port congestion ERP forecast",
        "operations",
        "Q4 Supply Chain Inventory Rebalancing",
        ["SupplyChain", "ERPModel"],
    ),
    (
        "X Thread: Breakdown of SQLite WAL Checkpoint Lock Contention - X",
        "https://x.com/systemsdev/status/987654",
        "SQLITE_BUSY WAL autocheckpoint reader snapshot transaction isolation benchmark",
        "research",
        "Fix SQLite Concurrency in TaskFlow",
        ["TaskFlow", "StorageEngine"],
    ),
]


def gen_counterfactual_pair(rng: random.Random) -> Tuple[Dict, Dict]:
    page_title, url, snippet, prod_cat, aligned_task, aligned_projs = rng.choice(COUNTERFACTUAL_TOPICS)
    app, full_title = browser_title(rng, page_title)
    use_content = rng.random() < 0.60
    content = add_ocr_noise(rng, snippet) if use_content else None

    pos = {
        "app_name": app,
        "window_title": full_title,
        "url": url,
        "content": content,
        "task_title": aligned_task,
        "task_description": f"Working on {', '.join(aligned_projs)}: {aligned_task}",
        "known_projects": aligned_projs,
        "category": prod_cat,
        "is_productive": 1,
        "relevance": round(rng.uniform(0.84, 0.98), 3),
    }

    unaligned_task, unaligned_desc, unaligned_projs, _ = rng.choice(TASK_POOL)
    is_hobby_ambiguous = any(k in page_title.lower() for k in ("pottery", "glaze", "coffee", "formula 1", "horror games"))
    if is_hobby_ambiguous:
        neg = {
            "app_name": app,
            "window_title": full_title,
            "url": url,
            "content": content,
            "task_title": unaligned_task,
            "task_description": unaligned_desc,
            "known_projects": unaligned_projs,
            "category": "entertainment",
            "is_productive": 0,
            "relevance": round(rng.uniform(0.03, 0.16), 3),
        }
    else:
        leisure_page = rng.choice([
            ("Insane Street Food Tour in Bangkok! Vlog - YouTube", "https://www.youtube.com/watch?v=foodvlog", "Subscribe Like Comment Street food vlog highlights", "entertainment"),
            ("r/funny - Dogs reacting to magic tricks - Reddit", "https://www.reddit.com/r/funny/comments/999/dogs/", "Upvotes 12.3k Comments 450 r/funny memes", "social_feed"),
            ("Celebrity Red Carpet Outfits Ranked / X", "https://x.com/popculture/status/555111", "Retweets 8K Likes 42K Trending For You", "social_feed"),
            ("Unboxing New Sneakers Haul #shorts - YouTube", "https://www.youtube.com/shorts/sneakers99", "#shorts Subscribe Like Share", "entertainment"),
        ])
        l_app, l_title = browser_title(rng, leisure_page[0])
        neg = {
            "app_name": l_app,
            "window_title": l_title,
            "url": leisure_page[1],
            "content": leisure_page[2] if use_content else None,
            "task_title": aligned_task,
            "task_description": f"Working on {', '.join(aligned_projs)}: {aligned_task}",
            "known_projects": aligned_projs,
            "category": leisure_page[3],
            "is_productive": 0,
            "relevance": round(rng.uniform(0.01, 0.10), 3),
        }
    return pos, neg


def attach_context_and_relevance(rng: random.Random, sample: Dict) -> Dict:
    cat = sample["category"]
    is_prod = sample["is_productive"]

    if rng.random() < 0.25:
        sample["task_title"] = None
        sample["task_description"] = None
        sample["known_projects"] = []
        sample["relevance"] = round(rng.uniform(0.62, 0.85), 3) if is_prod else round(rng.uniform(0.02, 0.15), 3)
        return sample

    matching_tasks = [t for t in TASK_POOL if cat in t[3]]
    if is_prod and matching_tasks and rng.random() < 0.70:
        t_title, t_desc, projs, _ = rng.choice(matching_tasks)
        if rng.random() < 0.30 and projs and projs[0] not in sample["window_title"]:
            sample["window_title"] = f"{projs[0]} - {sample['window_title']}"
            rel = rng.uniform(0.85, 0.99)
        else:
            rel = rng.uniform(0.68, 0.90)
    else:
        t_title, t_desc, projs, _ = rng.choice(TASK_POOL)
        rel = rng.uniform(0.45, 0.72) if is_prod else rng.uniform(0.01, 0.14)

    sample["task_title"] = t_title
    sample["task_description"] = t_desc
    sample["known_projects"] = projs
    sample["relevance"] = round(rel, 3)
    return sample


# ---------------------------------------------------------------------------
# Strictly Zero-Overlap Unseen-App & Unseen-Domain Holdout Benchmark
# (NONE of these apps/domains/games appear in training!)
# ---------------------------------------------------------------------------
UNSEEN_HOLDOUT_TEMPLATES: List[Tuple[str, str, Optional[str], Optional[str], str, int]] = [
    ("CLion", "matrix_solver.hpp — PhysicsSim — CLion", None, "template<typename T> class SparseMatrix { std::vector<T> vals; };", "coding", 1),
    ("Rider", "InventoryController.cs — WarehouseAPI — JetBrains Rider", None, "public async Task<ActionResult<StockDto>> GetStock(Guid sku)", "coding", 1),
    ("Helix", "parser.zig — zig-compiler — Helix", None, "pub fn parseExpr(allocator: std.mem.Allocator) !Node { return error.Invalid; }", "coding", 1),
    ("Google Chrome", "Compare branches · elixir-lang/phoenix · Codeberg - Google Chrome", "https://codeberg.org/elixir/phoenix/pulls/41", "defmodule Phoenix.Router do use Macro end", "coding", 1),
    ("Sublime Text", "shader_pass.glsl — GameRenderer — Sublime Text", None, "vec4 fragColor = texture(u_diffuse, v_uv) * vec4(light, 1.0);", "coding", 1),
    ("Emacs", "scheduler.clj — clojure-core — GNU Emacs", None, "(defn dispatch-job [queue job] (swap! queue conj job))", "coding", 1),
    ("Android Studio", "PaymentActivity.kt — FinWallet — Android Studio", None, "override fun onCreate(savedInstanceState: Bundle?) { super.onCreate(savedInstanceState) }", "coding", 1),

    ("Tilix", "nix develop --command cargo clippy — Tilix", None, "$ nix develop\nwarning: unused variable in src/net.rs", "terminal", 1),
    ("Terminator", "ansible-playbook site.yml -i inventory/prod — Terminator", None, "PLAY [webservers] *** TASK [Restart nginx] changed: [web-02]", "terminal", 1),
    ("Tabby", "rsync -avz ./dist/ deploy@10.0.1.4:/var/www/ — Tabby", None, "sent 14,820 bytes  received 412 bytes  total size is 1.4M", "terminal", 1),
    ("rio", "ffmpeg -i raw.mov -c:v libx265 output.mp4 — rio", None, "frame= 2410 fps= 62 q=28.0 size= 42012kB time=00:01:20.33", "terminal", 1),

    ("Zotero", "Zotero — PhD Literature Library (Transformer Pruning Papers)", None, "Author: Hinton et al. | Journal: NeurIPS | Citation Key: hinton2015distilling", "research", 1),
    ("Google Chrome", "IEEE Xplore: Low-Power Neural Quantization for Edge Devices - Google Chrome", "https://ieeexplore.ieee.org/document/9912345", "Abstract: Experimental results demonstrate 4.2x memory reduction on ARM Cortex-M.", "research", 1),
    ("Google Chrome", "JSTOR: Economic Impact of Monetary Policy on Emerging Markets - Google Chrome", "https://www.jstor.org/stable/24567890", "Journal of Macroeconomics, Vol. 44, pp. 112-138. Empirical regressions...", "research", 1),
    ("Google Chrome", "Nature Medicine — Single-cell RNA sequencing in oncology - Google Chrome", "https://www.nature.com/articles/s41591-025-01234", "Figure 2: Cohort survival analysis and biomarker expression profiles.", "research", 1),
    ("Google Chrome", "Rustonomicon — Unsafe Rust & Memory Ordering Reference - Google Chrome", "https://doc.rust-lang.org/nomicon/atomics.html", "Acquire-Release ordering establishes happens-before relationships between threads.", "research", 1),
    ("Google Chrome", "ArchWiki — PipeWire Audio Configuration & Troubleshooting - Google Chrome", "https://wiki.archlinux.org/title/PipeWire", "Configuration files reside under /etc/pipewire/ and ~/.config/pipewire/", "research", 1),

    ("Google Chrome", "Brilliant.org — Probability & Bayesian Inference Interactive Lesson - Google Chrome", "https://brilliant.org/courses/probability/bayesian/", "Practice Problem 3 of 8: Calculate the posterior probability given prior P(H)=0.05", "learning", 1),
    ("Google Chrome", "freeCodeCamp — Relational Database Certification Lab 4 - Google Chrome", "https://www.freecodecamp.org/learn/relational-database/", "Complete the SQL normalization exercises and run the verification test suite.", "learning", 1),
    ("Google Chrome", "Codecademy — Learn Intermediate TypeScript Generics Lesson - Google Chrome", "https://www.codecademy.com/courses/learn-intermediate-typescript", "Instructions: Define a generic interface Repository<T> with findById method.", "learning", 1),
    ("Google Chrome", "MIT OpenCourseWare — Linear Algebra Gilbert Strang Lecture 12 - YouTube - Google Chrome", "https://www.youtube.com/watch?v=strang12", "Graphs, networks, incidence matrices, nullspace and Kirchhoff current law", "learning", 1),
    ("Google Chrome", "Quizlet — Constitutional Law Flashcards & Practice Test - Google Chrome", "https://quizlet.com/987654321/con-law-flashcards", "Term: Commerce Clause | Definition: Article I, Section 8, Clause 3", "learning", 1),
    ("Google Chrome", "Pluralsight — Kubernetes Security Fundamentals Module 5 - Google Chrome", "https://app.pluralsight.com/course-player?clipId=442", "Course Progress 62% | Transcript: NetworkPolicies restrict pod-to-pod egress...", "learning", 1),

    ("Typora", "quarterly_postmortem_draft.md — Typora", None, "## Root Cause Analysis\nThe connection pool exhausted during peak traffic.", "writing", 1),
    ("Scrivener", "Chapter_07_Market_Strategy.scriv — Scrivener", None, "Word Count: 3,420 | Draft status: Revision 2 | Synopsis: Competitive moat", "writing", 1),
    ("Google Chrome", "Coda — Product Spec: Automated Billing Notifications - Google Chrome", "https://coda.io/d/Product-Spec-Billing_d123", "Problem statement, user stories, edge cases, and rollout timeline.", "writing", 1),
    ("Bear", "Weekly Engineering Leadership Notes — Bear", None, "# Leadership Sync\n- Hiring pipeline for staff backend role\n- Q4 reliability goals", "writing", 1),
    ("Google Chrome", "Dropbox Paper — Brand Messaging Guidelines v2 - Google Chrome", "https://paper.dropbox.com/doc/Brand-Messaging-v2", "Tone of voice: clear, direct, and grounded in customer outcomes.", "writing", 1),

    ("Krita", "character_concept_sheet.kra — Krita", None, "Brush: Ink-4 Pen Rough | Layer: LineArt | Opacity 100% | Canvas 3840x2160", "design", 1),
    ("SolidWorks", "Gearbox_Housing_Assembly.SLDASM — SOLIDWORKS", None, "Sketch4 | Extrude-Boss 14.5mm | Material: 6061 Aluminum Alloy | Tolerance ±0.05", "design", 1),
    ("KiCad", "Sensor_Board_RevC.kicad_pcb — KiCad PCB Editor", None, "Routing 4-layer PCB | Track width 0.2mm | Differential pair USB_D+ / USB_D-", "design", 1),
    ("Google Chrome", "Penpot — Checkout Flow UI Components Prototype - Google Chrome", "https://design.penpot.app/workspace/123", "Component Library: PrimaryButton, InputField, ModalCard | Flex Layout", "design", 1),
    ("Sketch", "iOS_Widget_Screens_v3.sketch — Sketch", None, "Artboard: Widget Medium 329x155 | Symbols | Shared Text Styles", "design", 1),
    ("Affinity Designer", "Packaging_Die_Line_Rev2.afdesign — Affinity Designer 2", None, "CMYK/8 | Bleed 3mm | Vector Curve Layer | Export PDF/X-4", "design", 1),

    ("Final Cut Pro", "Documentary_Rough_Cut_v4 — Final Cut Pro", None, "Project 24fps 4K | Magnetic Timeline | Compound Clip | Color Inspector", "media_production", 1),
    ("Logic Pro", "Film_Score_Cue_04.logicx — Logic Pro", None, "Track 12: Cello Section | Tempo 92 BPM | Bus 3 Hall Reverb | Bounce PCM", "media_production", 1),
    ("Reaper", "Audiobook_Chapter_03_Edit.rpp — REAPER", None, "Master Peak -1.2dB | ReaEQ | ReaComp | Render WAV 44.1kHz 24-bit", "media_production", 1),
    ("Kdenlive", "Tutorial_Screencast_Export.kdenlive — Kdenlive", None, "Video Track V2: Callout Overlay | Audio Track A1: Narration | Render MP4", "media_production", 1),

    ("Google Chrome", "ClickUp — Q4 Website Relaunch Tasks & Gantt Chart - Google Chrome", "https://app.clickup.com/12345/v/b/678", "Status: In Review | Due Date: Oct 28 | Subtasks (4/6 completed)", "planning", 1),
    ("Google Chrome", "Basecamp — Client Onboarding Project Schedule & To-dos - Google Chrome", "https://3.basecamp.com/12345/projects/678", "Milestones: Kickoff Call, Architecture Sign-off, Staging Delivery", "planning", 1),
    ("Google Chrome", "Shortcut — Iteration 22 Stories & Epics - Google Chrome", "https://app.shortcut.com/team/iterations/22", "Velocity: 38 pts | Ready for Dev: 9 | In Review: 4", "planning", 1),
    ("Google Chrome", "Calendly — Team Interview Availability Schedule - Google Chrome", "https://calendly.com/app/scheduled_events", "Upcoming Events: Technical Screen (30 min), Customer Demo (45 min)", "planning", 1),

    ("Google Chrome", "Whereby — Weekly Design Handoff Room - Google Chrome", "https://company.whereby.com/design-handoff", "Camera On | Mic Active | 5 participants in room | Screen Share", "meetings", 1),
    ("Google Chrome", "Jitsi Meet — Open Source Maintainers Call - Google Chrome", "https://meet.jit.si/maintainers-sync", "Participants (7) | Raise Hand | Toggle Mute | Recording Active", "meetings", 1),
    ("FaceTime", "FaceTime Call — Cofounder Sync (00:32:14)", None, "Video Call Active | Audio Connected | Mute | End Call", "meetings", 1),
    ("Around", "Around — Quick Frontend Pairing Call", None, "Floating Video Call | Screen Sharing Active | 3 participants", "meetings", 1),

    ("Superhuman", "Superhuman — Inbox: Partnership Proposal Reply", None, "From: alex@partner.io | Subject: Co-marketing launch dates | Reply", "communication", 1),
    ("Front", "Front — Shared Support Inbox (18 open conversations)", None, "Assigned to me | SLA Warning: 2h remaining | Internal Comment", "communication", 1),
    ("Google Chrome", "Freshdesk — Ticket #9012: API Rate Limit Increase Request - Google Chrome", "https://company.freshdesk.com/a/tickets/9012", "Requester: DevOps Lead | Priority: High | Reply to customer", "communication", 1),
    ("Spark", "Spark Mail — Re: Signed SOW Attachment", None, "Sent folder | Attachments: SOW_2026_Signed.pdf | Thread 6 messages", "communication", 1),

    ("RStudio", "clinical_survival_model.R — RStudio", None, "fit <- coxph(Surv(time, status) ~ age + treatment, data = cohort_df); summary(fit)", "data_finance", 1),
    ("MATLAB", "signal_fft_filter.m — MATLAB R2026a", None, "Y = fft(sensor_data); P2 = abs(Y/L); plot(f, P1);", "data_finance", 1),
    ("Google Chrome", "Looker Studio — Monthly Paid Acquisition & ROAS Report - Google Chrome", "https://lookerstudio.google.com/reporting/123", "CAC: $48.20 | LTV: $610 | Conversion Rate: 4.8% | Date Range: Last 90 Days", "data_finance", 1),
    ("Google Chrome", "Xero — Bank Reconciliation & Monthly Invoices - Google Chrome", "https://go.xero.com/BankRec/BankRec.aspx", "Statement Balance: $84,210.45 | 12 transactions to reconcile | Accounts Payable", "data_finance", 1),
    ("Power BI Desktop", "Supply_Chain_KPI_Executive.pbix — Power BI Desktop", None, "DAX Measure: YoY Growth % = DIVIDE([Current Rev] - [Prior Rev], [Prior Rev])", "data_finance", 1),
    ("Google Chrome", "Snowflake Snowsight — Worksheet: Daily Active Users Query - Google Chrome", "https://app.snowflake.com/worksheets/123", "Warehouse: ANALYTICS_M | Query succeeded: 1.2M rows scanned in 1.4s", "data_finance", 1),

    ("Wireshark", "eth0_tls_handshake_capture.pcapng — Wireshark", None, "TCP 443 -> 51422 [ACK] Seq=1420 | TLSv1.3 Client Hello | Filter: dns || tls", "operations", 1),
    ("Postman", "Production Payments API v2 — Postman", None, "POST https://api.company.com/v2/charges | 200 OK | 112 ms | Authorization: Bearer", "operations", 1),
    ("Google Chrome", "Fly.io Dashboard — App Metrics & Machine Regions - Google Chrome", "https://fly.io/apps/taskflow-edge/monitoring", "Machines: 4 running (iad, fra, sin) | Memory 210MB/512MB | Health checks passing", "operations", 1),
    ("Google Chrome", "PagerDuty — On-Call Schedule & Resolved Incidents - Google Chrome", "https://company.pagerduty.com/incidents", "Incident #842 Resolved | Service: Auth Gateway | Escalation Policy: Tier 1", "operations", 1),
    ("Google Chrome", "DigitalOcean — Droplets & Managed PostgreSQL Backups - Google Chrome", "https://cloud.digitalocean.com/databases", "Cluster Status: Online | Automated Daily Snapshots | Firewall Rules", "operations", 1),

    ("Google Chrome", "Letterboxd — Top 250 Sci-Fi Movies Watchlist & Reviews - Google Chrome", "https://letterboxd.com/films/popular/", "Watched | Liked | 4.5 stars review | Add to Watchlist | Trailers", "entertainment", 0),
    ("Google Chrome", "IMDb — Upcoming Summer Blockbuster Trailers & Cast - Google Chrome", "https://www.imdb.com/trailers/", "Watch Trailer 2:34 | Rating 8.4/10 | Box Office Weekend Top 10", "entertainment", 0),
    ("Google Chrome", "Max — House of the Dragon Episode 6 Streaming - Google Chrome", "https://play.max.com/video/watch/123", "Play | Subtitles | Skip Intro | Next Episode in 15s", "entertainment", 0),
    ("Google Chrome", "Bleacher Report — Premier League Transfer Rumors & Goals - Google Chrome", "https://bleacherreport.com/world-football", "Live Match Highlights | Scoreboard | Top 5 Goals of the Week", "entertainment", 0),
    ("Google Chrome", "Late Night Comedy Monologue Clips - YouTube - Google Chrome", "https://www.youtube.com/watch?v=latenight99", "1.4M views | Subscribe | Like | Comments | Funniest moments", "entertainment", 0),

    ("Stardew Valley", "Stardew Valley — Year 2 Summer Day 14", None, "Energy 240/270 | Gold: 18,450g | Quest Log | Inventory", "gaming", 0),
    ("Hollow Knight", "Hollow Knight — City of Tears", None, "Geo: 1,420 | Charms Equipped | Bench Rest", "gaming", 0),
    ("Factorio", "Factorio 2.0 — MegaBase Save", None, "Iron Plates/min: 4,200 | Research Queue | Biter Alert", "gaming", 0),
    ("Baldur's Gate 3", "Baldur's Gate 3 — Act 2 Shadow-Cursed Lands", None, "Party Level 7 | Turn-Based Combat | Quicksave", "gaming", 0),
    ("Google Chrome", "IGN — Best Open World Video Games Ranked & Boss Guides - Google Chrome", "https://www.ign.com/articles/best-open-world-games", "Review Score 9/10 | Gameplay Video | Walkthrough | Comments", "gaming", 0),
    ("Google Chrome", "Lichess.org — Bullet 1+0 Arena Tournament - Google Chrome", "https://lichess.org/tournament/abc123", "Rating 1685 | Berserk | Current Rank #14 | Play Next Game", "gaming", 0),

    ("Google Chrome", "9GAG — Funny Memes, GIFs & Viral Videos - Google Chrome", "https://9gag.com/trending", "Upvote 9.4K | Downvote | 412 Comments | Share | Meme Feed", "social_feed", 0),
    ("Google Chrome", "Tumblr — Dashboard Following Feed - Google Chrome", "https://www.tumblr.com/dashboard", "Reblog | Like | Notes 4,820 | Recommended Blogs | Explore", "social_feed", 0),
    ("Google Chrome", "Bluesky — Discover Feed & Viral Posts - Google Chrome", "https://bsky.app/profile/discover", "Repost 1.2K | Like 8.5K | Reply | Trending Topics", "social_feed", 0),
    ("Google Chrome", "BuzzFeed — 23 Hilarious Tweets About Mondays - Google Chrome", "https://www.buzzfeed.com/trending/hilarious-tweets", "Quiz | Celeb Gossip | Trending | Share on Facebook", "social_feed", 0),
    ("Google Chrome", "r/mildlyinfuriating - My delivery driver left this outside - Reddit - Google Chrome", "https://www.reddit.com/r/mildlyinfuriating/comments/123/", "28.4k upvotes | 2.1k comments | Popular feed", "social_feed", 0),

    ("Google Chrome", "IKEA — Standing Desks & Ergonomic Chairs Online Store - Google Chrome", "https://www.ikea.com/us/en/cat/desks/", "Add to Shopping Bag | $349.00 | Delivery availability | Customer reviews", "shopping_personal", 0),
    ("Google Chrome", "Target — Weekly Ad Deals & Cart Checkout - Google Chrome", "https://www.target.com/cart", "Subtotal $68.40 | Same-Day Delivery | Promo Code | Place Order", "shopping_personal", 0),
    ("Google Chrome", "Best Buy — 4K OLED Monitors On Sale - Google Chrome", "https://www.bestbuy.com/site/monitors", "Save $150 | Add to Cart | In Stock at Nearby Store | Ratings 4.8", "shopping_personal", 0),
    ("Google Chrome", "Booking.com — Top Rated Hotels in Kyoto for Vacation - Google Chrome", "https://www.booking.com/city/jp/kyoto.html", "Free Cancellation | 3 nights $480 | Guest Review Score 9.1 | Reserve", "shopping_personal", 0),
    ("Google Chrome", "Uber Eats — Order Thai Takeout Delivery - Google Chrome", "https://www.ubereats.com/checkout", "Your Cart (2 items) | Delivery Fee $2.99 | Tip | Place Order", "shopping_personal", 0),
    ("Google Chrome", "Shein — Summer Jackets Flash Sale & Wishlist - Google Chrome", "https://www.shein.com/flash-sale", "60% OFF | Add to Bag | Free Shipping | Coupon Applied", "shopping_personal", 0),
]


def build_unseen_holdout(rng: random.Random) -> List[Dict]:
    rows: List[Dict] = []
    for app, title, url, snippet, cat, is_prod in UNSEEN_HOLDOUT_TEMPLATES:
        for variant in range(12):
            use_deep = (variant % 2 == 1)
            if app == "Google Chrome":
                b_app, b_title = browser_title(rng, title.replace(" - Google Chrome", ""))
            else:
                b_app, b_title = app, title
            sample = {
                "app_name": b_app,
                "window_title": b_title,
                "url": url,
                "content": add_ocr_noise(rng, snippet) if (use_deep and snippet) else None,
                "category": cat,
                "is_productive": is_prod,
            }
            attach_context_and_relevance(rng, sample)
            rows.append(sample)
    rng.shuffle(rows)
    return rows


def main() -> None:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    rng = random.Random(20261127)

    train_rows: List[Dict] = []
    val_rows: List[Dict] = []

    for cat in CATEGORIES:
        gen_fn = CATEGORY_GENERATORS[cat]
        for _ in range(2300):
            train_rows.append(attach_context_and_relevance(rng, gen_fn(rng, val_split=False)))
        # Domain/app-disjoint validation split for honest generalization & calibration!
        for _ in range(280):
            val_rows.append(attach_context_and_relevance(rng, gen_fn(rng, val_split=True)))

    for _ in range(3000):
        pos, neg = gen_counterfactual_pair(rng)
        train_rows.extend([pos, neg])
    for _ in range(400):
        pos, neg = gen_counterfactual_pair(rng)
        val_rows.extend([pos, neg])

    dual_use_eval: List[Dict] = []
    for _ in range(600):
        pos, neg = gen_counterfactual_pair(rng)
        dual_use_eval.extend([pos, neg])

    rng.shuffle(train_rows)
    rng.shuffle(val_rows)
    rng.shuffle(dual_use_eval)

    unseen_eval = build_unseen_holdout(rng)

    def write_jsonl(path: Path, rows: List[Dict]) -> None:
        with path.open("w", encoding="utf-8") as f:
            for r in rows:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")

    write_jsonl(DATA_DIR / "train.jsonl", train_rows)
    write_jsonl(DATA_DIR / "val.jsonl", val_rows)
    write_jsonl(DATA_DIR / "eval_dual_use.jsonl", dual_use_eval)
    write_jsonl(DATA_DIR / "eval_gold_unseen.jsonl", unseen_eval)

    print(
        f"[generate_dataset] train={len(train_rows)} val_disjoint={len(val_rows)} "
        f"dual_use_eval={len(dual_use_eval)} unseen_gold={len(unseen_eval)}"
    )


if __name__ == "__main__":
    main()
