mod cmd;
pub mod exit;
pub mod style;

use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

use cmd::blast_radius::BlastFormat;
use gitcortex_viz::VizFormat;

/// The repo's actual current branch, so commands default to indexing/querying
/// where the user's history actually is instead of a hardcoded "main" that
/// silently returns nothing on any repo whose default branch is named
/// something else (master, trunk, develop, ...). Mirrors gitcortex-mcp's
/// `detect_current_branch`.
fn default_branch() -> String {
    std::process::Command::new("git")
        .args(["symbolic-ref", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "main".to_owned())
}

const FIRST_ANSWER_HELP: &str = "\
First-answer path (replace <fragment> and <symbol> with names from your repo):
  1. gcx query search <fragment>          Find matching symbols
  2. gcx query symbol-context <symbol>    Inspect definition, callers, callees, usages
  3. gcx query find-callers <symbol>      Check callers before editing

Setup:
  gcx init --editor none                  CLI-only setup (non-TTY default)
  gcx init --editor <token>               Configure an editor (claude, cursor, windsurf, copilot, antigravity, codex, all)
  gcx doctor                              Verify hooks, store, readiness, and registrations";

#[derive(Parser)]
#[command(
    name = "gcx",
    version,
    about = "GitCortex knowledge-graph CLI",
    after_help = FIRST_ANSWER_HELP
)]
struct Cli {
    /// When to emit ANSI colour: auto (TTY only), always, never.
    /// Also respects NO_COLOR, CLICOLOR=0, TERM=dumb when set to auto.
    #[arg(long, value_enum, default_value_t = style::ColorMode::Auto, global = true)]
    color: style::ColorMode,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install git hooks and run the initial index for this repo.
    Init {
        /// Also install the GitHub Actions blast-radius workflow.
        #[arg(long)]
        ci: bool,
        /// Editor to configure: none, auto, claude, cursor, windsurf, copilot, antigravity, codex, all.
        /// In an interactive TTY on first run, omitting --editor opens a one-time chooser;
        /// in non-TTY / CI runs it stays CLI-only (none) unless --editor is passed.
        #[arg(long, value_name = "EDITOR")]
        editor: Option<String>,
        /// Permit changes to editor configuration outside this repository.
        #[arg(long)]
        global_editor_config: bool,
        /// Permit changes to a Git hooks path shared outside this repository.
        #[arg(long)]
        shared_git_hooks: bool,
    },
    /// Remove GitCortex hooks and editor integrations from this repository.
    Deinit {
        /// Show every planned change without modifying files.
        #[arg(long)]
        dry_run: bool,
        /// Also remove .gitcortex/ and this repository's machine-local graph data.
        #[arg(long)]
        purge: bool,
        /// Also remove GitCortex entries from editor configuration outside this repository.
        #[arg(long)]
        global_editor_config: bool,
        /// Permit removing GitCortex blocks from a shared external Git hooks path.
        #[arg(long)]
        shared_git_hooks: bool,
    },
    /// Incremental index triggered by a git hook.
    Hook {
        /// Called from post-checkout; records the new branch without re-indexing.
        #[arg(long)]
        branch_switch: bool,
    },
    /// Start the MCP server (stdio transport by default).
    Serve {
        /// Expose all individual tools in addition to the `gcx` dispatch tool.
        /// By default only the compact single-tool schema is exposed to minimise
        /// per-turn token overhead (~200 tokens vs ~14 000 in full mode).
        #[arg(long)]
        full: bool,
    },
    /// Internal repository daemon used to multiplex local MCP clients.
    #[command(name = "__serve-daemon", hide = true)]
    ServeDaemon {
        #[arg(long)]
        repo_root: std::path::PathBuf,
    },
    /// One-shot query commands — useful for manual testing.
    #[command(subcommand, after_help = FIRST_ANSWER_HELP)]
    Query(QueryCmd),
    /// Visualise the knowledge graph in the browser or as DOT output.
    Viz {
        /// Branch to visualise.
        #[arg(long, default_value_t = default_branch())]
        branch: String,
        /// Output format.
        #[arg(long, default_value = "web", value_enum)]
        format: VizFormat,
        /// HTTP port (web mode only).
        #[arg(long, default_value_t = 5678)]
        port: u16,
    },
    /// Show the blast radius of changes between two branches.
    ///
    /// `--base` defaults to the current checkout branch. To compare a feature
    /// branch against a different target branch, pass `--base <target> --head HEAD`
    /// explicitly (for example: `gcx blast-radius --base main --head HEAD`).
    BlastRadius {
        /// Base branch (defaults to the current checkout branch; pass `--base main` to compare against main).
        #[arg(long, default_value_t = default_branch())]
        base: String,
        /// Head branch or ref (defaults to HEAD).
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// BFS depth for transitive caller discovery.
        #[arg(long, default_value_t = 2)]
        depth: u8,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: BlastFormat,
    },
    /// Export the knowledge graph as a Markdown map, versioned graph JSON, DOT/SVG/GraphML/Cypher/HTML, or a CLAUDE.md symbol block.
    Export {
        /// Branch to export (defaults to current branch).
        #[arg(long)]
        branch: Option<String>,
        /// Output format: markdown (default codebase map), json (legacy symbols + edges),
        /// graph-json (versioned graph v1), mermaid, dot, svg, graphml, cypher, or html.
        #[arg(long, default_value = "markdown", value_enum)]
        format: cmd::export::ExportFormat,
        /// Upsert a compressed top-symbols table into CLAUDE.md between
        /// `<!-- gcx:symbols -->` markers, so assistants get high-value symbols
        /// pre-loaded with zero tool calls. Overrides --format.
        #[arg(long)]
        claude_md: bool,
        /// How many top-ranked symbols to inject with --claude-md.
        #[arg(long, default_value_t = 40)]
        top: usize,
        /// Optional seed symbol (node id, qualified name, or exact short name) to scope traversal.
        #[arg(long)]
        seed: Option<String>,
        /// Traversal depth from --seed (0–5).
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(0..=5))]
        depth: u8,
        /// Traversal direction from --seed: up (incoming), down (outgoing), or both.
        #[arg(long, default_value = "both", value_enum)]
        direction: cmd::export::ExportDirection,
        /// Comma-separated edge kinds to traverse and emit (e.g. calls,imports).
        #[arg(long, value_delimiter = ',')]
        edge_kinds: Option<Vec<String>>,
        /// Maximum number of nodes to emit (defaults to 5000, or 100 for mermaid).
        #[arg(long)]
        max_nodes: Option<usize>,
        /// Write output to PATH instead of stdout (or instead of `.gitcortex/context.md` for markdown).
        #[arg(long, value_name = "PATH")]
        out: Option<std::path::PathBuf>,
        /// Export even when the branch index is not `ready` (records the state in metadata).
        #[arg(long)]
        allow_stale: bool,
        /// Overwrite non-empty output targets where applicable.
        #[arg(long)]
        force: bool,
    },
    /// Show indexed node/edge counts for the current branch.
    Status {
        /// Branch to inspect (defaults to current branch).
        #[arg(long)]
        branch: Option<String>,
    },
    /// Wipe the graph store for this repo so a fresh full index can run.
    Clean,
    /// Diagnose setup issues: hooks, store, index freshness, MCP registration.
    Doctor,
    /// Check for a newer release and print the right update command.
    Update,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AgentOutputFormat {
    Text,
    AgentJson,
}

#[derive(Subcommand)]
pub enum QueryCmd {
    /// Look up all nodes with the given name.
    LookupSymbol {
        name: String,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Find all callers of a function. Use --depth for multi-hop traversal (1–5).
    FindCallers {
        /// Exact short name or qualified symbol name. Ambiguous short names
        /// return candidates without traversing unrelated call graphs.
        name: String,
        #[arg(long, default_value_t = 1)]
        depth: u8,
        /// Maximum caller evidence rows returned after ranking.
        #[arg(long, default_value_t = 25)]
        limit: usize,
        /// Global response budget used by agent-json output.
        #[arg(long, default_value_t = 600)]
        budget_tokens: usize,
        #[arg(long, value_enum, default_value_t = AgentOutputFormat::Text)]
        format: AgentOutputFormat,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// List all definitions in a source file.
    ListDefinitions {
        file: String,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// 360° view of a symbol: definition, callers, callees, and type usages.
    SymbolContext {
        name: String,
        /// Maximum callers/callees/used_by rows returned per relation.
        #[arg(long, default_value_t = 25)]
        limit: usize,
        /// Global response budget used by agent-json output.
        #[arg(long, default_value_t = 800)]
        budget_tokens: usize,
        #[arg(long, value_enum, default_value_t = AgentOutputFormat::Text)]
        format: AgentOutputFormat,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Find all functions/methods called by a function. Use --depth for multi-hop (1–5).
    FindCallees {
        name: String,
        #[arg(long, default_value_t = 1)]
        depth: u8,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Find all types that implement or inherit a trait or interface.
    FindImplementors {
        name: String,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Find the shortest call path between two functions (max 6 hops).
    TracePath {
        from: String,
        to: String,
        #[arg(long, value_enum, default_value_t = AgentOutputFormat::Text)]
        format: AgentOutputFormat,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Find symbols with no callers or type references (dead code candidates).
    FindUnused {
        /// Optional kind filter: function, method, struct, trait, interface, enum, constant.
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Show all nodes and edges within N hops of a seed symbol.
    GetSubgraph {
        name: String,
        #[arg(long, default_value_t = 1)]
        depth: u8,
        /// Direction: in (callers/ancestors), out (callees/descendants), both.
        #[arg(long, default_value = "both")]
        direction: String,
        /// Maximum ranked relation evidence rows returned.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Global response budget used by agent-json output.
        #[arg(long, default_value_t = 400)]
        budget_tokens: usize,
        #[arg(long, value_enum, default_value_t = AgentOutputFormat::Text)]
        format: AgentOutputFormat,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Render a wiki-style markdown page for a symbol.
    Wiki {
        name: String,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Fuzzy search over the graph by name + qualified path.
    Search {
        query: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        include_details: bool,
        #[arg(long, default_value_t = 600)]
        budget_tokens: usize,
        #[arg(long, value_enum, default_value_t = AgentOutputFormat::Text)]
        format: AgentOutputFormat,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Generate a guided tour of the codebase (omit --seed for global tour).
    Tour {
        #[arg(long)]
        seed: Option<String>,
        #[arg(long, default_value_t = 6)]
        limit: usize,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Find high-centrality hub symbols with many inbound calls.
    FindGodNodes {
        #[arg(long, default_value_t = 10)]
        min_in_degree: u32,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
    /// Detect code communities via label-propagation clustering.
    FindClusters {
        #[arg(long, default_value_t = 3)]
        min_cluster_size: usize,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value_t = default_branch())]
        branch: String,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    style::init(cli.color);

    let result = match cli.command {
        Commands::Init {
            ci,
            editor,
            global_editor_config,
            shared_git_hooks,
        } => cmd::init::run(
            ci,
            editor.as_deref(),
            global_editor_config,
            shared_git_hooks,
        ),
        Commands::Deinit {
            dry_run,
            purge,
            global_editor_config,
            shared_git_hooks,
        } => cmd::deinit::run(dry_run, purge, global_editor_config, shared_git_hooks),
        Commands::Hook { branch_switch } => cmd::hook::run(branch_switch),
        Commands::Serve { full } => cmd::serve::run(!full),
        Commands::ServeDaemon { repo_root } => cmd::serve::run_daemon(repo_root),
        Commands::Query(q) => cmd::query::run(q),
        Commands::Viz {
            branch,
            format,
            port,
        } => gitcortex_viz::run(branch, port, format),
        Commands::BlastRadius {
            base,
            head,
            depth,
            format,
        } => cmd::blast_radius::run(base, head, depth, format),
        Commands::Export {
            branch,
            format,
            claude_md,
            top,
            seed,
            depth,
            direction,
            edge_kinds,
            max_nodes,
            out,
            allow_stale,
            force,
        } => cmd::export::run(cmd::export::ExportOptions {
            branch,
            format,
            claude_md,
            top,
            seed,
            depth,
            direction,
            edge_kinds,
            max_nodes,
            out,
            allow_stale,
            force,
        }),
        Commands::Status { branch } => cmd::status::run(branch),
        Commands::Clean => cmd::clean::run(),
        Commands::Doctor => cmd::doctor::run(),
        Commands::Update => cmd::update::run(),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(exit::exit_code(&e));
    }
}
