mod cmd;
mod mcp;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use cmd::blast_radius::BlastFormat;

#[derive(Parser)]
#[command(name = "gcx", version, about = "GitCortex knowledge-graph CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::ValueEnum, Clone)]
pub enum VizFormat {
    /// Open an interactive force-directed graph in the browser.
    Web,
    /// Print a Graphviz DOT file to stdout.
    Dot,
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
    /// Export the knowledge graph as a readable Markdown codebase map.
    Export {
        /// Branch to export (defaults to current branch).
        #[arg(long)]
        branch: Option<String>,
    },
    /// Show indexed node/edge counts for the current branch.
    Status {
        /// Branch to inspect (defaults to current branch).
        #[arg(long)]
        branch: Option<String>,
    },
    /// Wipe the graph store for this repo so a fresh full index can run.
    Clean,
}

#[derive(Subcommand)]
enum QueryCmd {
    /// Look up all nodes with the given name.
    LookupSymbol {
        name: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Find all callers of a function.
    FindCallers {
        name: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// List all definitions in a source file.
    ListDefinitions {
        file: String,
        #[arg(long, default_value = "main")]
        branch: String,
    },
    /// Show call-graph context for all definitions in a source file.
    Context {
        /// Repo-relative or absolute path to the source file.
        file: String,
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
        } => cmd::viz::run(branch, port, format),
        Commands::BlastRadius {
            base,
            head,
            depth,
            format,
        } => cmd::blast_radius::run(base, head, depth, format),
        Commands::Export { branch } => cmd::export::run(branch),
        Commands::Status { branch } => cmd::status::run(branch),
        Commands::Clean => cmd::clean::run(),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
