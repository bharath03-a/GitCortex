# GitCortex Demo — Recording Script

**Format:** Silent screen recording, ~3–4 min  
**Repo:** Django (`~/demo-django`)  
**Core message:** Same question to Claude Code — without GitCortex, then with. The contrast tells the story.

---

## Setup before you hit record

```bash
cd ~/demo-django

# Write MCP config
cat > .mcp.json << 'EOF'
{"mcpServers":{"gcx":{"command":"gcx","args":["serve"]}}}
EOF

# Index the repo
gcx init

# Confirm indexed
gcx status

# Clean terminal
clear
```

Large font size. Two terminal windows or split panes ready.

---

## Shot list

### Shot 1 — Benchmark report hero (5 sec)
Open benchmark report HTML in browser.  
Sit on the stat cards: **58% cheaper**, **2.15×** ratio.

---

### Shot 2 — Without GitCortex (60 sec)

Disable gcx, open Claude Code:
```bash
cd ~/demo-django
mv .mcp.json .mcp.json.bak
claude
```

Ask:
```
Where is authentication handled in this codebase?
List the key files and functions.
```

Show the **tool calls panel** — Claude will grep repeatedly, read multiple files, many turns.  
Let it finish. Note the turn count.

```bash
# exit claude
mv .mcp.json.bak .mcp.json
```

---

### Shot 3 — Title card (3 sec)
Text overlay or just type in terminal: `# WITH GitCortex`

---

### Shot 4 — With GitCortex (45 sec)

```bash
claude
```

**Same question:**
```
Where is authentication handled in this codebase?
List the key files and functions.
```

Tool calls panel shows: `search_code` → one call → done in 2–3 turns.  
Faster. Cleaner. Fewer reads.

---

### Shot 5 — Refactor impact (30 sec)

Still in Claude with gcx:
```
If I change BaseBackend, what breaks?
Show me the direct callers.
```

`find_callers` → instant structured list with file + line.

---

### Shot 6 — Tour (20 sec)

```
Give me a tour of this codebase — what are the main entry points?
```

`start_tour` → centrality-ranked entry points. One call.

---

### Shot 7 — Viz (30 sec)

```bash
# exit claude
gcx viz
```

Browser opens. Pan the Django graph.  
Press Cmd+K → search "authenticate" → zoom to node → Inspector panel.

---

### Shot 8 — Report close (20 sec)

Back to browser, scroll the benchmark report slowly:
- Ratio bar chart (green bars = graph wins)
- Cost table (ripgrep 55%, hono 58% cheaper)
- Stat card: **58% cheaper**

End there.

---

## Key contrast
Shot 2 (no gcx): Claude reads ~10 files, 8–12 turns, $0.04+  
Shot 4 (gcx): Claude uses 1–2 graph calls, 2–3 turns, half the cost

That gap is the whole pitch.

---

## Editing notes
- No voice needed — tool call panel + numbers do the work
- Add split title: `WITHOUT GitCortex` → `WITH GitCortex` between Shot 2 and Shot 4
- Cut any long Claude thinking pauses
- Zoom in on tool call panel during Shot 2 vs Shot 4 for clarity
- Export 1080p, upload to GitHub → paste URL into README `Demo` section

---

## Commands in order

```bash
# Setup
cd ~/demo-django && gcx init && clear

# Shot 2 — no gcx
mv .mcp.json .mcp.json.bak
claude
# ask: "Where is authentication handled?"
# /exit

# Re-enable
mv .mcp.json.bak .mcp.json

# Shot 4 — with gcx
claude
# same question
# more questions
# /exit

# Shot 7
gcx viz
```
