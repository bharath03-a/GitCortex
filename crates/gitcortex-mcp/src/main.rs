mod cmd;
mod mcp;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

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
    Init,
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
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => cmd::init::run(),
        Commands::Hook { branch_switch } => cmd::hook::run(branch_switch),
        Commands::Serve => cmd::serve::run(),
        Commands::Query(q) => cmd::query::run(q),
        Commands::Viz { branch, format, port } => cmd::viz::run(branch, port, format),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
