mod cmd;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use cmd::blast_radius::BlastFormat;
use gitcortex_viz::VizFormat;

#[derive(Parser)]
#[command(name = "gcx", version, about = "GitCortex knowledge-graph CLI")]
struct Cli {
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
        /// Editor to configure: claude, cursor, windsurf, copilot, antigravity, all.
        /// Defaults to auto-detecting from environment variables; installs for all editors
        /// when no editor-specific env var is found.
        #[arg(long, value_name = "EDITOR")]
        editor: Option<String>,
    },
    /// Incremental index triggered by a git hook.
    Hook {
        /// Called from post-checkout; records the new branch without re-indexing.
        #[arg(long)]
        branch_switch: bool,
    },
    /// Start the MCP server (stdio transport by default).
    Serve,
    /// One-shot query commands — useful for manual testing.
    #[command(subcommand)]
    Query(QueryCmd),
    /// Visualise the knowledge graph in the browser or as DOT output.
    Viz {
        /// Branch to visualise.
        #[arg(long, default_value = "main")]
        branch: String,
        /// Output format.
        #[arg(long, default_value = "web", value_enum)]
        format: VizFormat,
        /// HTTP port (web mode only).
        #[arg(long, default_value_t = 5678)]
        port: u16,
    },
    /// Show the blast radius of changes between two branches.
    BlastRadius {
        /// Base branch (the target you're merging into).
        #[arg(long, default_value = "main")]
        base: String,
        /// Head branch (the branch with changes).
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// BFS depth for transitive caller discovery.
        #[arg(long, default_value_t = 2)]
        depth: u8,
        /// Output format.
        #[arg(long, default_value = "text", value_enum)]
        format: BlastFormat,
    },
    /// Export the knowledge graph as a Markdown map, JSON, or a CLAUDE.md symbol block.
    Export {
        /// Branch to export (defaults to current branch).
        #[arg(long)]
        branch: Option<String>,
        /// Output format: markdown (default codebase map), json (symbols + edges).
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

#[derive(Subcommand)]
pub enum QueryCmd {
    /// Look up all nodes with the given name.
    LookupSymbol {
        name: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Find all callers of a function. Use --depth for multi-hop traversal (1–5).
    FindCallers {
        name: String,
        #[arg(long, default_value_t = 1)]
        depth: u8,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// List all definitions in a source file.
    ListDefinitions {
        file: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// 360° view of a symbol: definition, callers, callees, and type usages.
    SymbolContext {
        name: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Find all functions/methods called by a function. Use --depth for multi-hop (1–5).
    FindCallees {
        name: String,
        #[arg(long, default_value_t = 1)]
        depth: u8,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Find all types that implement or inherit a trait or interface.
    FindImplementors {
        name: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Find the shortest call path between two functions (max 6 hops).
    TracePath {
        from: String,
        to: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Find symbols with no callers or type references (dead code candidates).
    FindUnused {
        /// Optional kind filter: function, method, struct, trait, interface, enum, constant.
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Show all nodes and edges within N hops of a seed symbol.
    GetSubgraph {
        name: String,
        #[arg(long, default_value_t = 2)]
        depth: u8,
        /// Direction: in (callers/ancestors), out (callees/descendants), both.
        #[arg(long, default_value = "both")]
        direction: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Render a wiki-style markdown page for a symbol.
    Wiki {
        name: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Fuzzy search over the graph by name + qualified path.
    Search {
        query: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Generate a guided tour of the codebase (omit --seed for global tour).
    Tour {
        #[arg(long)]
        seed: Option<String>,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long, default_value = "main")]
        branch: String,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { ci, editor } => cmd::init::run(ci, editor.as_deref()),
        Commands::Hook { branch_switch } => cmd::hook::run(branch_switch),
        Commands::Serve => cmd::serve::run(),
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
        } => cmd::export::run(branch, format, claude_md, top),
        Commands::Status { branch } => cmd::status::run(branch),
        Commands::Clean => cmd::clean::run(),
        Commands::Doctor => cmd::doctor::run(),
        Commands::Update => cmd::update::run(),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
