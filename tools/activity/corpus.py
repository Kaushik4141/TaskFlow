"""Public synthetic scenarios only: no captures, history, DB, or private inputs.

Whole authored scenario templates and all metadata/OCR/app/session variants stay
in one partition. Slots are plausible invented filenames/projects, never random
topic words pasted onto a title. Holdout vocabulary naturally overlaps training;
these checks do not measure real activity accuracy or calibrated confidence.
"""

import random

from features import CATEGORIES

BROWSERS = ("firefox", "chrome.exe", "msedge.exe", "chromium", "helium")
EDITORS = ("code", "Code.exe", "com.visualstudio.code", "cursor", "nvim")
TERMINALS = ("ghostty", "com.mitchellh.ghostty", "WindowsTerminal.exe", "kitty", "alacritty")

# app family, title, public/example host/path, existing text. These are prose and
# plausible screen fragments rather than lists of category synonyms.
TRAIN = {
    "coding_debugging": [
        (TERMINALS, "cargo check", "", "Checking demo v0.1.0 error E0308 mismatched types expected Result found Option src/router.rs:42"),
        (TERMINALS, "cargo build --release", "", "Compiling demo Finished release target in 2.3s warning unused import std::fmt"),
        (TERMINALS, "npm run test", "", "FAIL parser.test.ts Expected 200 Received 500 AssertionError at describe request handler"),
        (EDITORS, "router.rs - {project} - Visual Studio Code", "", "fn route(path: &str) -> Result<Response> { let request = parse(path)?; Ok(response) }"),
        (EDITORS, "service.ts - {project}", "", "import { Router } from './router'; export async function handle(req: Request) { return await fetch(req.url); }"),
        (EDITORS, "worker.py - {project}", "", "def dispatch(job): return queue.submit(job) Traceback ValueError missing argument test_worker failed"),
        (BROWSERS, "Fix parser regression · Pull request #24", "github.com/sample/parser/pull/24", "Files changed +12 -3 Review changes async function parse(input) return tokens commit 8af2"),
        (TERMINALS, "git diff src/handler.rs", "", "diff --git a/src/handler.rs b/src/handler.rs @@ -30 +30 fn handle() error handling patch"),
        (EDITORS, "CMakeLists.txt - {project}", "", "add_executable(sample src/runner.cpp) target_link_libraries(sample pthread) build failed undefined reference"),
    ],
    "research_learning": [
        (BROWSERS, "Ownership - The Rust Programming Language", "doc.rust-lang.org/book/ch04-01-what-is-ownership.html", "Ownership is a set of rules that govern how a Rust program manages memory. Read the examples below."),
        (BROWSERS, "HTTP caching - MDN Web Docs", "developer.mozilla.org/en-US/docs/Web/HTTP/Caching", "This guide explains cache validation and conditional requests with examples of response headers."),
        (BROWSERS, "A survey of graph retrieval methods", "arxiv.org/abs/2401.01234", "Abstract We compare retrieval methods using an experimental benchmark. Related work and references discuss prior findings."),
        (BROWSERS, "Linear algebra | Course lesson 3", "example.org/course/vectors", "Lesson objectives Understand vector spaces. Watch the lecture and try the practice exercise."),
        (BROWSERS, "Binary search explained", "en.wikipedia.org/wiki/Binary_search", "Algorithm overview History Complexity References Further reading This article describes searching a sorted array."),
        (BROWSERS, "Why does this iterator consume its input?", "stackoverflow.com/questions/12345", "Question Answer The documentation explains ownership. Here is an example showing the difference between iter and into_iter."),
        (("evince", "AcroRd32.exe", "okular"), "distributed-systems-paper.pdf", "", "Research article Methods Results Discussion We evaluate consensus performance across several network conditions."),
        (BROWSERS, "Comparing indexing approaches", "example.org/research/indexes", "Analysis of alternatives: tradeoffs in memory overhead and lookup latency. The study provides evidence for each method."),
        (BROWSERS, "Learn CSS Grid | Tutorial", "web.dev/learn/css/grid", "In this tutorial you will learn how rows and columns work. Follow the examples and complete the exercise."),
    ],
    "writing_documentation": [
        (EDITORS, "README.md - {project}", "", "# Installation Install the package using the instructions below. ## Usage This section describes the configuration options."),
        (EDITORS, "release-notes.md - {project}", "", "# Release notes This update adds a revised setup guide. Edit the introductory paragraph and clarify the upgrade instructions."),
        (("soffice.bin", "WINWORD.EXE", "writer"), "Quarterly report draft.docx", "", "Introduction Our work this quarter focused on a smaller set of goals. Revise this paragraph before publishing the report."),
        (BROWSERS, "API design proposal - Google Docs", "docs.google.com/document/d/sample/edit", "Draft proposal Scope Background Edit the wording in section two. Add a concise conclusion and proofread the document."),
        (EDITORS, "docs/setup.md - {project}", "", "## Getting started Open the settings panel and choose a folder. Document the steps and revise the troubleshooting guide."),
        (("notepad.exe", "gedit", "kate"), "Article draft.txt", "", "A first draft of the article. The opening sentence needs editing. Replace repeated wording and reorganize these paragraphs."),
        (TERMINALS, "nvim handbook.md", "", "# Handbook ## Onboarding Write the introduction here. Add headings and edit the markdown instructions for new contributors."),
        (BROWSERS, "Editing the user guide", "example.org/wiki/User_guide/edit", "Edit source Preview changes Publish page This guide explains the installation procedure and frequently asked questions."),
        (("soffice.bin", "WINWORD.EXE", "writer"), "Project specification.odt", "", "Specification draft Overview Requirements Revise the prose to clarify the scope. Track changes Insert heading Proofread."),
    ],
    "communication": [
        (BROWSERS, "Inbox (3) - Gmail", "mail.google.com/mail/u/0/", "Compose Inbox Sent Drafts Subject From Reply Forward Thanks for the update. Can we discuss tomorrow?"),
        (BROWSERS, "Mail - Outlook", "outlook.live.com/mail/0/inbox", "New message Inbox Sent items To Subject Reply all Forward We received your message and will follow up."),
        (("thunderbird", "OUTLOOK.EXE", "org.mozilla.Thunderbird"), "Inbox - Mail", "", "Get messages Write Address book Subject From Sent Reply to the conversation about the next meeting."),
        (("slack", "Slack.exe", "com.slack.Slack"), "#engineering - Workspace - Slack", "", "Team member 10:31 Can you review the patch later? Reply in thread Send message Add reaction"),
        (BROWSERS, "Direct messages | Slack", "app.slack.com/client/sample/dm", "Messages Can you send the notes? Sure, I will share them after lunch. Message Reply in thread"),
        (BROWSERS, "Messages - Discord", "discord.com/channels/@me", "Direct Messages Friends Online Hey, are you joining the call? Type a message Attach file"),
        (("teams", "ms-teams.exe", "zoom"), "Team standup | Meeting", "", "Participants Mute camera Share Leave meeting Chat Let's discuss the handoff and the next steps."),
        (BROWSERS, "Chat - Microsoft Teams", "teams.microsoft.com/v2/chat", "Chat New message Activity Colleague: thanks for checking that. You: I'll reply with the details."),
        (("signal-desktop", "telegram-desktop", "Signal.exe"), "Conversation - Messenger", "", "Message received Yesterday 16:04 Are you available for a quick call? Reply Send emoji"),
    ],
    "planning_administration": [
        (BROWSERS, "Sprint board - Jira", "sample.atlassian.net/jira/software/projects/DEMO/boards/1", "Backlog To do In progress Done Assign owner Estimate priority Schedule next sprint"),
        (BROWSERS, "Team roadmap - Linear", "linear.app/sample/roadmap", "Milestones Due dates Project status Plan next quarter Prioritize issues and assign tasks"),
        (("calendar", "gnome-calendar", "OUTLOOK.EXE"), "October 2026 - Calendar", "", "Week Month New event Appointment 09:00 Meeting Schedule reminder Add attendees Availability"),
        (BROWSERS, "Calendar - Week view", "calendar.google.com/calendar/u/0/r/week", "Create event Today Mon Tue Wed Agenda Appointment recurring meeting schedule"),
        (("EXCEL.EXE", "soffice.bin", "calc"), "Expenses.xlsx", "", "Expense ledger Date Description Amount Subtotal Budget Invoice Paid Total SUM B2:B12"),
        (BROWSERS, "Project checklist - Notion", "notion.so/sample/project-plan", "To do Next steps Owner Deadline Organize agenda Review priorities Estimate work"),
        (BROWSERS, "Invoices | Administration", "example.org/billing/invoices", "Invoice number Due date Payment status Balance Approve expense Download statement"),
        (BROWSERS, "Board | Trello", "trello.com/b/sample/project", "Tasks Add card Backlog Doing Done Due date Checklist Members Schedule priorities"),
        (TERMINALS, "nvim agenda.txt", "", "Monday agenda Plan quarterly milestones Assign owners Check deadlines Prepare budget and schedule appointments"),
    ],
    "entertainment_media": [
        (BROWSERS, "Comedy clips - YouTube", "youtube.com/watch", "Funny moments compilation Play Pause Subscribe Comments Recommended videos Enjoy this comedy clip."),
        (BROWSERS, "Live gameplay - Twitch", "twitch.tv/sample", "Playing adventure game Live stream Followers Subscribe Chat Match highlights next level"),
        (("steam", "steam.exe", "com.valvesoftware.Steam"), "Library - Steam", "", "Play Install Game library Achievements Friends Multiplayer match Controller settings"),
        (BROWSERS, "Movie trailer - Watch now", "example.org/movies/trailer", "Official trailer Upcoming film Watch video Cast Cinema Comedy series recommendations"),
        (("vlc", "vlc.exe", "mpv"), "Adventure-film.mp4 - VLC media player", "", "Playing movie Pause Audio Subtitle Playback 00:42:15 / 01:38:02"),
        (("spotify", "Spotify.exe", "rhythmbox"), "Evening playlist - Music", "", "Now playing Songs Album Artist Shuffle Next track Add to playlist Listen"),
        (BROWSERS, "Funny pictures and memes", "reddit.com/r/funny", "Popular posts Memes Comedy Upvote Comments Just for fun Browse community pictures"),
        (BROWSERS, "Series episode 4 - Streaming", "example.org/watch/series", "Continue watching Episode Season Play trailer Entertainment Recommendations"),
        (("lutris", "steam", "game.exe"), "Multiplayer lobby", "", "Join match Invite friend Play game Scoreboard Level inventory Achievements"),
    ],
    "system_navigation": [
        (("explorer.exe", "dolphin", "nautilus"), "Downloads", "", "Name Date modified Type Size Folders Copy Paste Rename Properties Sort by Open location"),
        (("thunar", "org.gnome.Nautilus", "explorer.exe"), "Home - File Manager", "", "Documents Pictures Downloads Recent Files Navigate directories Move to trash New folder"),
        (("SystemSettings.exe", "gnome-control-center", "systemsettings"), "Display - Settings", "", "Brightness Resolution Refresh rate Scale Night light Apply display configuration"),
        (TERMINALS, "sudo apt update", "", "Reading package lists Done Building dependency tree Updates available Install package manager"),
        (TERMINALS, "df -h", "", "Filesystem Size Used Avail Use% Mounted on /dev/sda1 /home Disk usage storage"),
        (TERMINALS, "systemctl status NetworkManager", "", "Loaded enabled Active running Main PID Tasks Network service restart system configuration"),
        (("taskmgr.exe", "gnome-system-monitor", "ksysguard"), "System Monitor", "", "Processes CPU Memory Disk Network End task Resource usage Running process"),
        (("SystemSettings.exe", "gnome-control-center", "systemsettings"), "Wi-Fi - Settings", "", "Network connection Devices Bluetooth Pair device Adapter Configure system preferences"),
        (TERMINALS, "ls -la ~/Downloads", "", "drwxr-xr-x . .. archive.zip pictures Move files mkdir backup cp mv file permissions"),
    ],
    "other": [
        (("krita", "mspaint.exe", "drawing"), "Untitled - Drawing", "", "Brush Canvas Layers Opacity Color Paint sketch Illustration Select shape"),
        (("blender", "blender.exe"), "Scene - Blender", "", "Viewport Mesh Material Object Sculpt Render Texture animation Transform"),
        (BROWSERS, "Walking directions - Maps", "openstreetmap.org/directions", "Route Distance Travel Map Location Walking Terrain Destination Directions"),
        (("calculator", "calc.exe", "gnome-calculator"), "Calculator", "", "Standard Scientific 42 + 17 = 59 Multiply Divide Numerical result"),
        (BROWSERS, "Forecast - Weather", "example.org/weather", "Today temperature rain wind cloudy conditions Tomorrow forecast Humidity"),
        (("inkscape", "Illustrator.exe"), "Poster.svg - Vector drawing", "", "Draw Bezier path Fill Stroke Layers Artwork Align Shape Color"),
        (BROWSERS, "Sketch canvas", "example.org/draw/canvas", "Brush width Palette Paint Layers Export illustration Eraser"),
        (("qgis", "QGIS.exe"), "Regional terrain map", "", "Map layers Coordinates Location Distance Terrain Zoom Select geographic feature"),
        (("audacity", "Audacity.exe"), "Untitled audio project", "", "Waveform Record track Effects Trim selection Fade Mixer Edit sound"),
    ],
}

# Independently authored heldout template families, not a shuffled row split.
HOLDOUT = {
    "coding_debugging": [
        (TERMINALS, "pytest tests/test_routes.py -q", "", "FAILED test_routes AssertionError expected status code 200 got 404 traceback app/routes.py:51"),
        (EDITORS, "storage.cpp — {project} — editor", "", "#include <vector> int write_record(const Record& value) { return store.insert(value); } compiler diagnostic"),
        (BROWSERS, "Review change to request handler", "github.com/sample/server/compare", "Commit changes diff src/request.ts function execute Patch failing integration test"),
    ],
    "research_learning": [
        (BROWSERS, "Understanding database transactions | Reference", "docs.example.org/database/transactions", "Read the reference explanation of transaction isolation. Examples demonstrate concurrent access and consistency."),
        (("okular", "evince"), "networking-survey.pdf", "", "Literature review This paper studies packet routing methods. We compare evidence from previous experiments."),
        (BROWSERS, "Statistics lesson: sampling", "example.org/learn/statistics", "Study notes Introduction Learn probability from this tutorial and practice the exercise."),
    ],
    "writing_documentation": [
        (EDITORS, "CONTRIBUTING.md — {project}", "", "# Contributor guide Describe how to submit changes. Revise the instructions and proofread the markdown document."),
        (("WINWORD.EXE", "writer"), "Manuscript revision", "", "Reorganize this paragraph. Improve the conclusion and edit the draft article before publishing."),
        (BROWSERS, "Editing: Deployment handbook", "docs.google.com/document/d/handbook/edit", "Document outline Heading Introduction Draft installation instructions Revise prose Suggest wording"),
    ],
    "communication": [
        (BROWSERS, "Inbox — Proton Mail", "mail.proton.me/u/0/inbox", "New message Sender Subject Reply Forward Sent Conversation Please respond when you have a moment."),
        (("telegram-desktop", "signal-desktop"), "Direct conversation", "", "Friend: See you tomorrow. Reply Send message Notifications Attach file"),
        (BROWSERS, "Channel conversation", "app.slack.com/client/sample/channel", "Colleague posted a message Reply thread Thanks for the clarification Send response"),
    ],
    "planning_administration": [
        (BROWSERS, "Quarterly milestone overview", "trello.com/b/sample/milestones", "Prioritize backlog Assign tasks Track deadline Project schedule Checklist Due soon"),
        (("calc", "EXCEL.EXE"), "Budget review worksheet", "", "Accounting Expense totals Invoice number Balance Due date Spreadsheet SUM monthly budget"),
        (BROWSERS, "Appointments this week", "calendar.google.com/calendar/u/0/r/agenda", "Agenda Create event Availability Schedule appointment Set reminder Weekly meeting"),
    ],
    "entertainment_media": [
        (BROWSERS, "Sketch comedy playlist", "youtube.com/playlist", "Watch funny video Next episode Comedy clip Relaxing Recommended entertainment"),
        (("steam", "lutris"), "Co-op game session", "", "Play match Game level Friends online Invite Achievements Multiplayer"),
        (("mpv", "vlc.exe"), "Space-movie.mkv", "", "Playback Pause Movie subtitles Audio track Watching film 01:10:34"),
    ],
    "system_navigation": [
        (("thunar", "explorer.exe"), "Pictures — Directory browser", "", "Move Rename Copy File properties Folders Sort by size Filesystem path"),
        (TERMINALS, "journalctl -u bluetooth", "", "System service Device adapter Network configuration daemon started Restart service"),
        (("systemsettings", "gnome-control-center"), "Devices and preferences", "", "Adjust desktop brightness Display resolution Storage Network device configuration"),
    ],
    "other": [
        (("krita", "inkscape"), "Landscape artwork", "", "Brush paint Canvas Shapes Layers Opacity Sketch illustration Creative drawing"),
        (BROWSERS, "Cycling route planner", "openstreetmap.org/directions", "Map terrain Travel destination Location Route distance Directions"),
        (("blender", "blender.exe"), "Sculpting viewport", "", "Mesh material Render object Texture modeling Scene sculpture Transform"),
    ],
}

# Shared chrome and unrelated neutral text across every class; never label cues.
TRAIN_NOISE = (
    "File Edit View Help Search Open Recent Save Preferences Window Close Back Next Status Ready",
    "Overview Details More options Search all Sort Filter Share Home Help Menu Select Cancel Continue",
    "Welcome Workspace Sidebar Tabs History Bookmarks Tools Zoom Find New window Settings Print Refresh",
    "Monday October 7 09:41 English Connected 100% Loading Updated Navigation Panel Expand Minimize",
    "The sample contains several entries. A selected item appears in the center. More information is available below.",
)
HOLDOUT_NOISE = (
    "Menu Toolbar Navigation pane Find text Toggle sidebar Fit page Backward Forward Reload Options Help center",
    "09:52 Tuesday Connected English US Search everywhere Open tab Recent items Bookmarks View full screen",
    "This panel shows additional information. Select an entry to continue. The current view has been updated.",
)
PROJECTS = {"train": ("SampleServer", "DemoToolkit", "PracticeApp"),
            "holdout": ("ReferenceClient", "SandboxTools", "TrialService")}


def examples(partition):
    if partition not in ("train", "holdout"):
        raise ValueError("expected train or holdout")
    templates = TRAIN if partition == "train" else HOLDOUT
    noise = TRAIN_NOISE if partition == "train" else HOLDOUT_NOISE
    rows = []
    for category in CATEGORIES:
        for template_id, (apps, title, url, content) in enumerate(templates[category]):
            for session_id, project in enumerate(PROJECTS[partition]):
                rng = random.Random(f"{partition}/{category}/{template_id}/{session_id}")
                app = apps[rng.randrange(len(apps))]
                shuffled = list(noise)
                rng.shuffle(shuffled)
                chrome = " ".join(shuffled)
                # Both OCR signal positions, chrome-only content and metadata-only
                # cases. At most 128 tokens remain in the deployed extractor.
                variants = ("", content, chrome + " " + content,
                            content + " " + chrome, chrome)
                for variant_id, text in enumerate(variants):
                    rows.append({"category": category,
                                 "session": f"{partition}/{category}/{template_id}/{session_id}",
                                 "template": f"{partition}/{category}/{template_id}",
                                 "variant": variant_id, "app": app,
                                 "title": title.format(project=project), "url": url,
                                 "content": text})
    return rows
