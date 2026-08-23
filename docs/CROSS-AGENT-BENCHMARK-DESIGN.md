# Cross-Agent Benchmark Design: gcx vs CodeGraph across Codex, Claude Code, Antigravity

Follow-up to `docs/RETRIEVAL-ACCURACY-AND-TOKEN-EFFICIENCY-SPIKE.md` (`main` at
time of writing, working tree `feat/safe-init-homebrew`). That spike left five
open questions; this doc closes two of them with real, end-to-end tests
against the actual `agy` binary (`agy 1.1.19`, `/Users/bharathvelamala/.local/bin/agy`)
in this environment, researches each platform's own documented conventions
against primary sources, and designs (not builds) a proper 3-client version of
the ad-hoc `big-repos-v1.toml` + `codegraph_compare.py` comparison run earlier
this session. Every claim is cited to `file:line`, a command actually run, or
a URL fetched during this pass.

---

## Part 1: The `agy` MCP permission fix

### 1.1 What the spike got half-right

The spike (§1.5, §4.4) found via Antigravity's public docs that a scoped
`mcp(server/tool)` allow-list exists as an alternative to
`--dangerously-skip-permissions`, but could not test it — no `agy` binary was
available in that sandbox. This environment has `agy 1.1.19` installed
(`which agy` → `/Users/bharathvelamala/.local/bin/agy`), so this pass tested
the real thing.

**The CLI flag surface does not have a permissions flag.** `agy --help` lists
only three permission-adjacent flags:

```
--dangerously-skip-permissions  Auto-approve all tool permission requests without prompting
--mode                          Set the agent execution mode for this session (accept-edits, plan)
--sandbox                       Run in a sandbox with terminal restrictions enabled
```

`agy mcp --help` only manages server registration (`add`/`remove`/`list`/`enable`/`disable`)
— no permissions subcommand. So `mcp(server/tool)` is real, but it is not a
CLI flag; it is a permission-grant string format, and the question was where
it's read from.

### 1.2 Confirming the mechanism by making it fail, then succeed

Direct subprocess testing (same manual, isolated-signal style the spike used
in its own §1.5/§4.4), against `tokio` (already indexed under
`/tmp/gcx-agent-bench/repos/tokio`, `gcx 0.7.3` at
`/tmp/gcx-agent-bench/gcx-bin`):

**Without any grant, using `--mode accept-edits` instead of
`--dangerously-skip-permissions`:**

```
agy -p '...call the gitcortex MCP tool named exactly "gcx"...' \
  --output-format stream-json --model "Gemini 3.6 Flash (Low)" \
  --mode accept-edits --print-timeout 90s
```

Result: the `call_mcp_tool` step returns `state: "ERROR"` with
`"error": {"type": "TOOL_ERROR", "message": "permission check failed for mcp \"gitcortex/gcx\": user denied permission for mcp(gitcortex/gcx)"}`.
This is the authoritative confirmation that `mcp(gitcortex/gcx)` is the real
internal permission-grant identifier for this exact tool call — headless mode
auto-denies (it does **not** hang the way `--sandbox` did in the spike's own
test) and reports the missing grant by its exact string form.

**A second attempt used a `.agents/hooks.json`-style PreToolUse hook**
(`~/.gemini/config/hooks.json`, matcher `call_mcp_tool`, handler script
returning `{"decision":"allow"}`) — confirmed as a real, documented mechanism
(`agy` binary embeds its own hooks.json reference docs, see Part 2.3) — and it
did let the `call_mcp_tool` step itself go to `state: "DONE"` with no
permission error. But the run still ended `status: "CANCELED"` with agy
printing a plain-text (non-JSON, mixed into stdout) diagnostic:

```
jetski: no output produced — a tool required the "mcp" permission that headless
mode cannot prompt for, so it was auto-denied. Add an allow-rule under
permissions.allow in settings.json (e.g. mcp(<target>)). Alternatively, re-run
with --dangerously-skip-permissions to auto-approve all tools.
```

This is `agy` naming its own real config surface unprompted. It confirmed:
key `permissions.allow` (array), entry format `mcp(<target>)`, target format
`server/tool` (matching the error string's `gitcortex/gcx`), and the settings
file is JSON (not a CLI flag, not exclusively a hooks.json PreToolUse rule —
that's a separate, coarser mechanism).

**Third test — the actual fix, isolated:**

```json
// ~/.gemini/antigravity-cli/settings.json
{
  "model": "Gemini 3.6 Flash (High)",
  "permissions": { "allow": ["mcp(gitcortex/gcx)"] },
  ...
}
```

Rerunning the same `agy -p ... --mode accept-edits` command (no
`--dangerously-skip-permissions`, no hooks.json) against `tokio`:
`status: "SUCCESS"`, the `call_mcp_tool` step returned real evidence (16
definitions in `tokio/src/lib.rs`, correctly reported by the model), and no
permission error anywhere in the trace. **Confirmed working, end-to-end,
against the real `gitcortex` MCP server this repo ships.**

### 1.3 The fix needed one more grant to be usable in the harness

Running the real fix through `tools/agent-bench/agent_run.py --client agy`
(not the isolated manual test) on `tokio-search-notify` surfaced a second,
narrower denial: `agy` runs a `run_command "pwd"` as its first orientation
step on nearly every task, before touching any file tool or the MCP server.
Without a grant for it, that step is auto-denied too and the whole run fails
before ever reaching the `gcx` call — this is a `permission check failed for
command "pwd"` error captured directly in the harness's own JSONL
(`tools/agent-bench/results/agy-permfix-verify-20260823T091401Z.agent.jsonl`,
`gcx.error_messages`).

Adding `"command(pwd)"` alongside the MCP grant resolved this. Verified with a
direct multi-step test (`pwd` then 4 `call_mcp_tool` attempts, the model
iterating on the `gcx` tool's argument schema before getting it right):
`pwd` executed with no denial, and none of the 4 `call_mcp_tool` attempts hit
a permission error — every failure among them was a `gcx` schema error
(`missing field 'action'`, `missing field 'params'`, `params.file is
required`), not a permission problem. The final attempt succeeded and
returned real graph output.

### 1.4 The fix, as shipped

`tools/agent-bench/agent_run.py`:

- Added `AGY_SETTINGS = Path.home() / ".gemini" / "antigravity-cli" / "settings.json"` (`agent_run.py:443`).
- Added `agy_permissions_config(enabled: bool)` (`agent_run.py:465-490`), a
  context manager mirroring the existing `agy_mcp_config`'s swap-and-restore
  pattern: merges `permissions.allow` into the user's existing
  `settings.json`, always granting `command(pwd)`, and additionally granting
  `mcp(gitcortex/gcx)` when `enabled` (the `gcx` arm only — the baseline arm
  never touches the MCP server, so it doesn't need that grant). Restores the
  original file content (or deletes it, if none existed) on exit, exactly
  like `agy_mcp_config` already does for the MCP config file, so a benchmark
  run never leaves the user's own `agy` setup altered.
- `run_agy_arm` (`agent_run.py:557-`) replaced
  `--dangerously-skip-permissions` with `--mode accept-edits` and wraps the
  subprocess call in both `agy_mcp_config(...)` and the new
  `agy_permissions_config(...)` context managers.

New tests: `tools/agent-bench/test_agent_run.py` — three unit tests against
`agy_permissions_config` using a temp file (merge-in, enable/disable grant
sets, restore-and-delete-when-absent). `python3 -m unittest discover -p
"test_*.py"` in `tools/agent-bench/`: **39/39 pass**, up from the prior 36 (no
`agent_run.py` tests existed before this change).

**End-to-end re-verification through the real harness** (`agent_run.py
--client agy --repo tokio --task tokio-search-notify --reuse-index`, cobra
task; earlier attempt through the full harness stalled — see caveat below):
in the isolated multi-step manual test with both grants set exactly as the
code now sets them, the `gcx` MCP call succeeded fully with zero permission
denials. The permission mechanism itself is proven; a full harness run to
completion (with its own `gcx serve` daemon multiplexing, per
`e5f28cf`/`f3bd16f`) was not independently re-confirmed to completion in this
pass due to cost/time — the first harness run (before the `pwd` grant was
added) got as far as `gcx_calls=1, gcx_errors=0, commands=4`, i.e. inside the
harness's own `command_budget_met` threshold, with the only failure being the
unrelated `pwd` denial this pass then fixed. That specific combination
(harness invocation + both grants) was not re-run to a final summary line
before this doc was written — flag this as the one claim in Part 1 not
independently closed out, not a guess presented as fact.

**Caveat on the grant itself:** `command(pwd)` is scoped to exactly the
observed failure (a literal `pwd` call), not to `run_command` broadly. If a
future `agy` release or a different task prompt makes the model call some
other orientation command first (`ls`, `git rev-parse`, etc. — all seen in
§1.6 below, though none of those specific calls hit a permission denial in
the traces reviewed), this grant will need widening. Widen it only against
another observed, cited denial — don't pre-authorize `run_command` broadly
just because it's plausible; this repo's own `.claude/CLAUDE.md` conventions
("minimum code that solves the problem", "no 'flexibility' that wasn't
requested") argue against speculative scope creep here, and MCP permission
scoping in particular is a security boundary, not a convenience knob.

### 1.5 Open question #4: why does agy run 10-20 commands per task?

Answered from the raw per-task JSONL logs in
`tools/agent-bench/results/big-repos-agy-20260823T075925Z.agent-logs/`
(not just the summarized `.agent.jsonl`), for the two worst offenders per the
summary (`django-callers-filter`: 20 commands, `tokio-search-notify`: 16).

**The `gcx` MCP call itself is not the problem.** In
`r1-django-callers-filter-gcx.jsonl`, the very first tool call is
`call_mcp_tool` with `action: "find_callers"` — it returns `state: "DONE"`
with no error, on the first attempt. Every one of the ~18 subsequent commands
is the model trying to satisfy its own "verify with a focused file read"
instruction and failing to find the file:

```
find_by_name  Pattern="django/db/models/query.py" SearchDirectory="/Users/bharathvelamala"
  -> ERROR: Find command timed out ... context deadline exceeded
run_command   "pwd"
run_command   "ls -la && git rev-parse --show-toplevel..."
run_command   "find /Users/bharathvelamala -maxdepth 4 -name query.py"
run_command   "mdfind \"kMDItemFSName == 'query.py'\""
run_command   "ps aux | grep -i antigravity"
run_command   "find /Users/bharathvelamala -maxdepth 5 -name .git"
run_command   "find /tmp /var/tmp -maxdepth 5 -name query.py"
manage_task   status / kill (spawns and manages its own background subtasks)
run_command   "for d in ... GitCortex ... .no-mistakes/worktrees ..."
run_command   "find /Users/bharathvelamala -name django"
  -> ERROR: context canceled
run_command   "git -C .../GitCortex branch -a | grep bench || find ..."
run_command   "ps aux | grep -i gitcortex"
run_command   "cat .../mcp/gitcortex/* || cat ~/.gemini/.../mcp_config.json"
run_command   "find ~/.cache /tmp /var/tmp -name query.py"
  -> ERROR: context canceled
```

**Root cause: the very first `find_by_name` searches
`SearchDirectory: "/Users/bharathvelamala"` — the user's home directory —
never the actual repo checkout the CLI was launched with
(`cwd=repo_dir`, i.e. `/tmp/gcx-agent-bench/repos/django`).** Once that first
targeted search times out (searching an entire home directory recursively for
one filename genuinely can), the model falls into an escalating, undirected
scavenger hunt across `/tmp`, `/var/tmp`, `~/.cache`, spawns and kills its own
background tasks (`manage_task`), and inspects its own MCP/git configuration
— none of which was asked for by the dispatch prompt
(`agent_run.py:537-548`, "you may make at most three focused file reads").
This is not the model retrying the `gcx` call (it never calls `gcx` again
after the first success in this trace) and not a permission problem (no
permission-denial error appears until the run is eventually canceled by the
harness's outer timeout) — it's the model's own file-verification step using
the wrong base directory, then compounding the mistake with an unbounded
search strategy instead of giving up after the three reads its own
instructions allow.

This is the single highest-value next fix for making the `agy` arm usable:
either (a) get the model to respect `cwd` for `find_by_name`/`run_command`
(may require probing whether `agy` exposes a way to pin the tool's working
directory independent of what the model infers), or (b) tighten the dispatch
prompt to state the repo's absolute path explicitly so there is no directory
to guess, or (c) hard-cap the number of non-`gcx` tool calls the harness will
tolerate before force-ending the turn. Not fixed in this pass — it's a
prompt/harness design question, not a permission question, and Part 1 was
scoped to the permission mechanism specifically.

### 1.6 What this doesn't fix

`command_budget_met` in `agent_run.py:751` / `agent_report.py:105` is still a
shared literal `4` for all three clients (per spike §1.5/recommendation 4.5,
unchanged by this pass). Given §1.5's finding above, `agy`'s problem when it
blows the budget is the scavenger-hunt pattern, not the `gcx` call itself —
raising the budget number alone would hide the actual defect rather than fix
it. Recommend leaving the budget check as the spike suggested (§4.5,
unaddressed here) and treating over-budget `agy` runs as a signal to inspect
the trace for exactly this pattern before assuming it's a permission or
budget-tuning problem.

---

## Part 2: Per-platform conventions vs what GitCortex ships

### 2.1 Codex

**AGENTS.md discovery** (fetched `learn.chatgpt.com/docs/agent-configuration/agents-md`,
2026-08-23): Codex loads instructions from **two scopes**: global
(`$CODEX_HOME`, default `~/.codex`) first, then **project scope, walking from
the git root down to the current working directory, checking each level**.
At each level it prefers `AGENTS.override.md` over `AGENTS.md`, includes at
most one file per directory, and concatenates root-to-leaf so files closer to
the cwd take precedence (they appear later in the combined prompt). Default
size cap is 32 KiB (`project_doc_max_bytes`). Fallback filenames are
configurable via `project_doc_fallback_filenames`.

`crates/gitcortex-cli/src/cmd/init/editors/codex.rs:37-59`
(`write_agents_md`) writes a single `AGENTS.md` at the **repo root** with a
marked, idempotent, migratable section (`>>> gitcortex codex integration
>>>`). This matches the documented project-scope convention correctly — repo
root is the git root Codex walks from.

**MCP config** (fetched `learn.chatgpt.com/docs/config-file/config-basic`,
2026-08-23): precedence is CLI flags > `.codex/config.toml` (project, closest
wins, trusted projects only) > `~/.codex/profile-name.config.toml` >
`~/.codex/config.toml` (user) > `/etc/codex/config.toml` (system) > built-in
defaults. `codex.rs:26-30` writes `[mcp_servers.gitcortex]` with
`command = "gcx"`, `args = ["serve"]`, `startup_timeout_sec = 30` to
`.codex/config.toml` — the project-scope file, correctly placed per the
documented precedence chain.

**Hooks** (fetched `learn.chatgpt.com/codex/hooks`, 2026-08-23): Codex has a
real, near-Claude-Code-shaped hooks system, gated by a `hooks = true` feature
flag (default-on), configured via a `hooks.json` file or an inline `[hooks]`
table in `config.toml`. Real event names: `SessionStart`, `SessionEnd`,
`PreToolUse`, `PostToolUse`, `PermissionRequest`, `PreCompact`,
`PostCompact`, `UserPromptSubmit`, `SubagentStart`, `SubagentStop`, `Stop`.
Format: `{"hooks": {"EventName": [{"matcher": ..., "hooks": [{"type":
"command", "command": ..., "timeout": ..., "async": ...}]}]}}` — structurally
identical to Claude Code's `settings.json` hooks shape (§2.2). Exit code 2
blocks; JSON stdout with `permissionDecision`/`hookSpecificOutput` controls
tool execution.

**Gap:** `codex.rs` has no hooks.json wiring at all. `claude.rs` (§2.2) ships
a `PreToolUse` hook that appends `gcx query context <file>` output whenever
Claude reads a source file — a directly analogous hook is buildable for
Codex today (same event name, same JSON contract, same "silent unless
indexed" design already proven in the Claude hook) but doesn't exist. This is
a real, concrete, low-risk addition: `codex.rs` would gain a `write_hooks_json`
function mirroring `claude.rs::write_pre_tool_use_hook`, targeting
`.codex/hooks.json`'s `PreToolUse` array with a `view_file`/`Read`-equivalent
matcher. Not implemented in this pass (Part 2 is investigation/design only
per the task scope).

**Skills:** the fetched docs referenced an "Agent configuration" section
covering subagents/rules but did not surface a Codex-specific packaged-skill
mechanism equivalent to Claude Code's `SKILL.md` or Antigravity's
`.agents/skills/`. Not confirmed either way from primary sources fetched in
this pass — flag as open, don't assert Codex lacks skills, only that this
pass didn't find the doc page for it.

### 2.2 Claude Code

**Skills** (fetched `code.claude.com/docs/en/skills`, 2026-08-23):
`.claude/skills/<name>/SKILL.md`, frontmatter-driven, auto-invoked when
relevant or explicitly via `/skill-name`. Custom commands
(`.claude/commands/<name>.md`) are now unified with skills — both produce the
same slash command. `claude.rs:41-95` (`SKILLS` const) writes four
project-level skills (`exploring.md`, `debugging.md`, `impact-analysis.md`,
`refactoring.md`) as plain markdown files, not `SKILL.md`-with-frontmatter —
**worth checking**: confirm the install path (`write_skills`, not read in
this pass) actually places these under `.claude/skills/<name>/SKILL.md` per
the documented convention rather than as loose files; if they're loose
`.md` files without the `<name>/SKILL.md` directory structure or YAML
frontmatter, they won't be discovered as skills at all under the current
Claude Code convention. This is the single most concrete, checkable gap
candidate in Part 2 — flagged here as a next step, not confirmed as a bug,
since `write_skills`'s actual file-path logic wasn't read in this pass.

**Hooks:** `.claude/settings.json`'s `hooks` key, event names including
`PreToolUse`/`PostToolUse`/`Stop`/`SessionStart`/`SubagentStart` among many
more, `{"matcher": ..., "hooks": [{"type": "command", "command": ...}]}`
shape. `claude.rs:22-34` (`PRE_TOOL_USE_HOOK`) and
`write_claude_settings`/`write_pre_tool_use_hook` (referenced, not fully read
in this pass) already wire this up correctly per the fetched convention.

**MCP config:** project-level `.mcp.json` (`claude.rs:126-141`,
`write_project_mcp_json`) plus an optional global path when
`global_editor_config` is set. This matches Claude Code's documented
project/global split.

Overall, `claude.rs` is the most complete of the three editor integrations
against its platform's own documented conventions — the skills-location
question above is the only unresolved gap.

### 2.3 Antigravity (`agy`)

Primary source for this section is unusually strong: `agy`'s own binary
embeds its full internal docs (`strings "$(which agy)"`, matched against
`hooks.json`/`Customization Roots`/`AGENTS.md` sections directly, not a
website) — arguably more authoritative than the public
`antigravity.google/docs` pages the spike cited, since it's exactly what the
installed binary reads.

**Customization roots & skills:** confirmed real, discoverable, documented
inside the binary. Skills live at `<customization-root>/skills/<skill_name>/SKILL.md`
(same `SKILL.md` shape as Claude Code). Rules live at
`<customization-root>/rules/` **or** standalone `GEMINI.md`/`AGENTS.md`
files directly in the root — **both filenames are recognized**, and the
binary's own doc text explicitly recommends consolidating into `AGENTS.md`
("Placing a consolidated `AGENTS.md` (or `GEMINI.md`) file under `rules/`...
is recommended over separate rule files"). Plugins live at
`<root>/plugins/<plugin_name>/`. The project-level customization root is
`.agents/` (repo-relative) per multiple matches (`{workspace}/.agents/agents/{agent_name}/`,
`.agents/skills.json` for shared/registered skill directories,
`.agents/skills/`, `.agents/hooks.json`, `.agents/plugins/`).

**Hooks:** confirmed real and fully documented in-binary (§1.2 above already
used this mechanism directly). File: `.agents/hooks.json` (project) or
`~/.gemini/config/hooks.json` (global, confirmed by testing — this is the
file the `/hooks` TUI command writes to, per the binary's own changelog
strings). Same shape family as Codex/Claude: named hooks, each with
`PreToolUse`/`PostToolUse` (grouped, `matcher` + `hooks` array) or
`PreInvocation`/`PostInvocation`/`Stop` (flat handler list). `PreToolUse`
handlers can return `{"decision": "allow"|"deny"|"ask"|"force_ask",
"permissionOverrides": [...], "overwrite": {...}}` — richer than a plain
allow/deny, and notably `permissionOverrides` accepts the same `mcp(...)`/
`command(...)` grant-string format used in `permissions.allow`, meaning a
hook can grant temporary, call-scoped permissions dynamically instead of
requiring a static settings.json entry — a more surgical alternative to this
pass's static-grant fix, not explored further here since the static grant
was sufficient and simpler.

**MCP config:** confirmed global-only, matching what `agent_run.py`'s
existing comment already assumed and what `antigravity.rs` already
implements (`~/.gemini/config/mcp_config.json`, `~/.antigravity/mcp.json` for
the IDE). No `.agents/`-scoped MCP config surfaced anywhere in the extracted
doc strings — this really does appear to be a genuine platform constraint,
not an oversight in either the harness or `antigravity.rs`.

**Gaps in `antigravity.rs` vs the platform's own documented conventions:**

1. **No project-level `AGENTS.md`/`GEMINI.md` rules file.** `codex.rs` and
   `claude.rs` both write a marked rules section at the repo root;
   `antigravity.rs` (`crates/gitcortex-cli/src/cmd/init/editors/antigravity.rs`)
   writes MCP config only — no `.agents/rules/AGENTS.md` (or `GEMINI.md`),
   despite the platform explicitly recommending exactly this pattern and
   `.agents/` being the standard project customization root.
2. **No project-level skills.** No `.agents/skills/<name>/SKILL.md`
   equivalent to Claude Code's four skills (`exploring`, `debugging`,
   `impact-analysis`, `refactoring`) — same `SKILL.md` format, directly
   portable content.
3. **No hooks wiring.** No `.agents/hooks.json` equivalent to Claude Code's
   `PreToolUse` context-injection hook, despite `agy` having (per this
   pass's own direct testing) an equivalent, real, working `PreToolUse`
   mechanism with an even richer permission-grant contract than Claude's.

None of these are required for the MCP server itself to work (confirmed
end-to-end in Part 1 with `antigravity.rs`'s existing global MCP config as-is)
— they're missing onboarding/DX surface that the other two editors already
have. Not implemented in this pass; Part 2 is investigation/design only.

---

## Part 3: Cross-client benchmark design

### 3.1 Is `big-repos-v1.toml`'s 5-repo, 10-task suite the right long-term set?

Reasonable as a **starting** pinned suite (`tools/agent-bench/big-repos-v1.toml:1-`),
but has a structural gap for the 3-client goal: every task is `search` or
`callers` (`grep -c 'action = "search"'` / `'action = "callers"'` against the
file: 5 search + 5 callers, one pair per repo). Two other MCP tools this repo
ships and documents (`.claude/CLAUDE.md`'s own tool list) —
`get-subgraph`/`symbol_context` and `blast-radius` — have zero coverage in
this suite. Given §1.1 of the spike found `find_callers`'s risk-banding bug
and `blast_radius.rs` shares the identical formula, a suite that never
exercises `blast-radius` can't surface risk-banding regressions at all
through the agent-loop lanes (only through the deterministic retrieval lane,
which doesn't score agent-facing behavior). Recommend: keep the 5 repos
(good language/ecosystem spread: Rust, Python, TS, Go, Java — matches
`gitcortex-indexer`'s tree-sitter language coverage), add one `blast-radius`
task and one `subgraph`/`symbol_context` task per repo for the next suite
revision, doubling task count to 20. Keep repos and commits pinned exactly as
now (`big-repos-v1.toml:4-27`) — reproducibility here is already correct,
just coverage-narrow.

### 3.2 Running CodeGraph through codex/agy, not just claude

`codegraph_compare.py` (`codegraph_compare.py:1-19`'s own docstring) already
states its scope precisely: CodeGraph has no MCP mode wired into this
harness, driven via its own CLI, and reuses whatever `big-repos-claude-fixed`
baseline is on disk (`find_baseline_source`, `codegraph_compare.py:52-`) —
Claude Code only, by design, for now.

A codex-driven CodeGraph arm is straightforward to add: `codegraph_compare.py`
already imports `agent_run`'s `question`/`ArmResult`/`error_arm_result`
helpers (`codegraph_compare.py:29`) — the same pattern used for `claude`
today (a raw subprocess call to `codegraph query`/`codegraph callers`,
scored against the same required-evidence contract) ports to Codex's `codex
exec` CLI lane essentially unchanged, since Codex's lane
(`README.md`'s "Codex agent-loop lane" section) is already CLI-driven, not
MCP — CodeGraph is CLI-driven too, so the two compose naturally. No new
dispatch-model research needed here; it's the same shape as the existing
`claude` CodeGraph arm, swapped onto `codex exec`.

An agy-driven CodeGraph arm is harder and, given Part 1/2's findings,
probably not worth building yet: CodeGraph would need to be installed as
either an `agy` MCP server (unlikely — it's a CLI tool, not documented as
exposing MCP) or invoked the same CLI way as the `claude`/`codex` arms, which
works, but §1.5's scavenger-hunt finding means an `agy` arm asked to shell out
to an unfamiliar CLI tool is exactly the failure mode already observed with
plain file search — expect the same wrong-`SearchDirectory` problem to
recur. Recommend deferring an `agy` CodeGraph arm until §1.5's root cause
(cwd-vs-home-directory search) has an actual fix, not just a documented
workaround; building the comparison arm on top of a known-flaky substrate
would produce noisy, hard-to-trust numbers.

### 3.3 Reproducibility across releases

`docs/benchmarks/RELEASE-GATE.md` covers a different, older lane (measured
Claude-API-only token benchmark, explicitly marked historical / superseded by
`tools/agent-bench/README.md`'s pinned harness) — it does not cover
multi-client agent-loop reproducibility at all, so there's no existing
convention to mirror here beyond the general principle it already
establishes: **trust only measured usage, never a proxy** (`RELEASE-GATE.md`
lines 6-17). That principle already holds in `agent_run.py`'s real
token-usage capture per client.

What's still missing for release-over-release comparability specifically:
`codegraph_compare.py`'s own §3.3-cited gap (the spike, not re-verified here)
— no `gcx_sha256` staleness check between the reused baseline file and the
currently-built binary (`codegraph_compare.py:52-91`). For a genuine 3-client
release gate, each client's result file's `meta` line already carries
`gcx_sha256`/`client_version`/`repo_commits`/`suite_sha256`
(confirmed directly in this pass's own
`agy-permfix-verify-20260823T091401Z.agent.jsonl` meta line) — the missing
piece is a comparison step that refuses to diff two runs whose `gcx_sha256`
or `suite_sha256` don't match, across all three clients uniformly, not just
CodeGraph's baseline-reuse path. This is a small, well-scoped addition to
`bench.py compare`/`codegraph_compare.py`, not built in this pass.

### 3.4 Summary of what a "proper" 3-client benchmark needs, in priority order

1. Fix §1.5's cwd-vs-home-directory search problem for `agy` (blocks trusting
   any `agy` arm's command count or latency numbers at all — not fixed here).
2. Widen `big-repos-v1.toml` to cover `blast-radius` and
   `subgraph`/`symbol_context`, not just `search`/`callers` (§3.1).
3. Add a `codex` CodeGraph arm (straightforward port, §3.2).
4. Add the `gcx_sha256`/`suite_sha256` staleness gate to `bench.py compare`
   (§3.3), applied uniformly across all three clients' result files.
5. Only then add an `agy` CodeGraph arm, once (1) is resolved.

None of this is built in this pass — Part 3 is design only, per the task's
own scope. Part 1's `agent_run.py` permission fix (§1.4) is the only shipped
code change from this pass.
