36 hours into DSU DevHack 3.0, our team (VoidX) has been building TaskFlow — a local-first desktop app that automatically turns daily engineering activity into an Obsidian second-brain knowledge graph.

Repo: https://github.com/Kaushik4141/TaskFlow

With 3 contributors (Kaushik H S, pratham9-pr, sathvik-2) working across three distinct stacks (Rust/Tauri backend, Python FastAPI sidecar, React frontend), GitHub management was what kept us moving fast:

• 7 PRs merged in 36 hours: Every milestone went through clean PR reviews before hitting main (PR #1 Privacy layer, PR #2 GraphRAG, PR #3 & #6 UI updates, #4 User auth, #5 Cloud MCP server, #7 In-app semantic search).
• Feature branch isolation: Teammates worked on isolated branches (privacy-layer, GraphRag, ui-updation, harness-v2), preventing conflicts between frontend and backend capture pipelines.
• Conventional commit history: Standardized commits (feat, fix, chore) made tracking changes and debugging between the Rust core and Python sidecar fast and predictable.
• Keeping main deployable: Merging strictly through GitHub PRs meant main stayed stable and testable throughout the entire sprint.

The cool part? We're actually using TaskFlow's own MCP server right now to pull this data, and Hermes is writing this post!

Big thanks to Dayananda Sagar University (DSU) for hosting and GitHub Education for providing the developer platform that makes sprints like this possible.

Drop a like to support our team before the 3:00 PM challenge cutoff! 🚀

#githubatdsudevhack3 #GitHubEducation #DSUDevHack #TaskFlow #GitHub
