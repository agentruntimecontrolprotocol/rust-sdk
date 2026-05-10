//! `arcp` — command-line interface for the ARCP reference runtime.
//!
//! Phase 0 ships a stub binary that prints version information and exits.
//! Full subcommands (`serve`, `tail`, `send`, `replay`) land in Phase 7.

use clap::Parser;

/// Top-level CLI definition.
#[derive(Debug, Parser)]
#[command(
    name = "arcp",
    version,
    about = "Reference CLI for the Agent Runtime Control Protocol",
    long_about = None
)]
struct Cli {
    /// Increase logging verbosity. Repeat for more (`-v`, `-vv`, `-vvv`).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

fn main() {
    let _cli = Cli::parse();
    println!(
        "arcp {} (protocol {}) — phase 0 skeleton; subcommands land in phase 7.",
        arcp::IMPL_VERSION,
        arcp::PROTOCOL_VERSION,
    );
}
