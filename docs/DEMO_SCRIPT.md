# GitCortex Demo Video — Recording Guide

Goal: a **90–120 second** screen recording that shows GitCortex turning a real
repo into a queryable graph an AI editor uses. Land the "aha" in the first
20 seconds: AI answering a precise code question without reading files.

The README has a `<!-- VIDEO_SLOT -->` marker (in the `## Demo` section). Drop
the final file there.

---

## Setup before recording

- **Repo to demo on:** `psf/requests` — small, famous, indexes in ~1.5s, and
  everyone recognizes `Session`/`HTTPAdapter`. (Do *not* demo on django: the
  7→? second index is real but undercuts the "instant" story for a 2-min clip.)
- Terminal: large font (≥18pt), dark theme, 1920×1080 capture, clear the
  scrollback.
- Have the locally-built binary on PATH:
  `cargo build --release -p gitcortex && export PATH="$PWD/target/release:$PATH"`
- Pre-clone so the clone isn't in the video:
  `git clone --depth 1 https://github.com/psf/requests /tmp/demo-requests`
- Claude Code (or Cursor) open in `/tmp/demo-requests` with the gitcortex MCP
  server registered (gcx init does this).

---

## Shot list

### 1. Index it (0:00–0:20)  — the hook
```bash
cd /tmp/demo-requests
gcx init
```
Show the output line: `Graph: 754 nodes | 2699 edges  (~1.5s)`.
On-screen caption: *"One command. Whole repo → graph. Updates on every commit."*

### 2. Ask the AI a precise question (0:20–0:55)  — the payoff
In Claude Code, type a question that forces graph use, e.g.:
> "What calls `Session.send` and what would break if I change its signature?"

Show Claude invoking the MCP tools (`find_callers`, `symbol_context`,
`blast_radius`) and answering with file:line citations — **without** opening and
scanning files. Caption: *"No file scanning. Structured answers from the graph."*

Backup prompts (pick whichever demos cleanest):
- "Give me a guided tour of this codebase." → `start_tour`
- "Show me a wiki page for `HTTPAdapter`." → `wiki_symbol`
- "Find anything related to retries." → `search_code`

### 3. CLI queries (0:55–1:15)  — for the terminal crowd
```bash
gcx query wiki HTTPAdapter        # signature + docstring + callers + callees
gcx query search retry --limit 5  # ranked fuzzy match
gcx query tour --limit 8          # centrality-ranked guided tour
gcx query blast-radius --base main --head HEAD
```
Caption: *"Same graph, scriptable from the CLI."*

### 4. Interactive viz (1:15–1:45)  — the eye candy
```bash
gcx viz
```
Browser opens the Cosmograph WebGL graph. Do three moves:
- Cmd+K → search `Session` → it zooms to the node.
- Click the node → Inspector shows callers/callees.
- Toggle a density mode (Focused → Full) in the header.
Caption: *"GPU-rendered graph of the whole repo, branch-aware."*

### 5. Incremental update (1:45–2:00)  — the kicker
```bash
echo '# touched' >> src/requests/api.py
git commit -am "tweak"        # post-commit hook fires
gcx status                    # graph already current, sub-second
```
Caption: *"Every commit re-indexes only what changed."*

---

## Examples cheat-sheet (verified to exist in psf/requests)

| Symbol | Kind | Good for |
|---|---|---|
| `Session` | class | wiki, callers (56), tour top |
| `HTTPAdapter` | class | wiki w/ rich docstring, inheritance (BaseAdapter) |
| `Request` / `PreparedRequest` | class | symbol_context, uses edges |
| `get` / `post` / `request` | function | search ranking, fan-out |
| `prepare` | method | callers, central node |
| `BaseAdapter` | class | find-implementors → HTTPAdapter |

---

## Recording tips
- Tools: macOS `⌘⇧5` or QuickTime for screen; or `asciinema` for the terminal
  segments (crisp text, small file) then stitch.
- Keep each command on screen long enough to read the output (~3s).
- Trim dead air; target ≤2 min. GitHub inlines `.mp4`/`.mov` ≤10MB in markdown.
- Export 1080p H.264. If >10MB, upload as a GitHub Release asset or to the
  repo's `user-attachments` and link it in the README slot.

## After recording
1. Drop the file at `docs/demo.mp4` (or upload + grab the URL).
2. Replace the `<!-- VIDEO_SLOT -->` block in `README.md` with the embed/link.
3. Keep a 10s GIF version near the top of the README for browsers that don't
   autoplay video.
