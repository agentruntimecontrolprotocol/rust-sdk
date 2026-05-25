//! `arcp` — command-line interface for the ARCP reference runtime.
//!
//! Phase 7 ships `serve`, `version`, and a placeholder `tail` subcommand.
//! Full CLI surfaces (`tail`, `send`, `replay` with rich filter flags) are
//! a follow-up.

#![deny(unsafe_code)]
#![allow(clippy::expect_used)]

#[cfg(feature = "transport-ws")]
use std::time::Duration;

#[cfg(feature = "transport-ws")]
use arcp_core::messages::Capabilities;
#[cfg(feature = "transport-ws")]
use arcp_core::transport::websocket::WebSocketTransport;
#[cfg(feature = "transport-ws")]
use arcp_runtime::auth::{BearerAuthenticator, NoneAuthenticator};
#[cfg(feature = "transport-ws")]
use arcp_runtime::runtime::ARCPRuntime;
use clap::{Parser, Subcommand};
#[cfg(feature = "transport-ws")]
use tokio::signal;
use tracing_subscriber::EnvFilter;

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

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the protocol and crate versions and exit.
    Version,
    /// Run an ARCP runtime, accepting one connection.
    Serve {
        /// Transport to bind. Currently only `ws` is supported.
        #[arg(long, default_value = "ws")]
        transport: String,
        /// Bind address. Default `127.0.0.1:7777` (PLAN.md §A4.15).
        #[arg(long, default_value = "127.0.0.1:7777")]
        bind: String,
        /// Bearer token to accept (omit to advertise anonymous auth).
        #[arg(long)]
        bearer: Option<String>,
        /// Principal name to associate with `--bearer` (default: `cli-user`).
        #[arg(long, default_value = "cli-user")]
        principal: String,
    },
    /// Placeholder — full implementation lands with subscription dispatch.
    Tail {
        /// Session id to filter on.
        #[arg(long)]
        session: Option<String>,
    },
}

fn install_tracing(verbose: u8) {
    let level = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    install_tracing(cli.verbose);

    match cli.command {
        Command::Version => {
            println!(
                "arcp {} (protocol {}, kind {})",
                arcp_core::IMPL_VERSION,
                arcp_core::PROTOCOL_VERSION,
                arcp_core::IMPL_KIND,
            );
        }
        Command::Tail { session } => {
            println!(
                "tail subcommand is a placeholder; subscription dispatch lands in a follow-up. \
                 (filter: session={session:?})"
            );
        }
        Command::Serve {
            transport,
            bind,
            bearer,
            principal,
        } => {
            handle_serve(transport, bind, bearer, principal).await?;
        }
    }
    Ok(())
}

#[cfg(feature = "transport-ws")]
async fn handle_serve(
    transport: String,
    bind: String,
    bearer: Option<String>,
    principal: String,
) -> Result<(), Box<dyn std::error::Error>> {
    match transport.as_str() {
        "ws" => serve_ws(&bind, bearer.as_deref(), &principal).await,
        other => {
            eprintln!("unsupported transport: {other} (only `ws` for now)");
            std::process::exit(2);
        }
    }
}

#[cfg(not(feature = "transport-ws"))]
#[allow(clippy::unused_async)] // Signature must match the cfg(transport-ws) variant.
async fn handle_serve(
    _transport: String,
    _bind: String,
    _bearer: Option<String>,
    _principal: String,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("the `transport-ws` feature is not enabled in this build");
    std::process::exit(2);
}

#[cfg(feature = "transport-ws")]
async fn serve_ws(
    bind: &str,
    bearer: Option<&str>,
    principal: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut caps = Capabilities {
        streaming: Some(true),
        artifacts: Some(true),
        ..Default::default()
    };
    let mut builder = ARCPRuntime::builder();
    if let Some(token) = bearer {
        builder = builder.with_authenticator(Box::new(
            BearerAuthenticator::new().with_token(token, principal),
        ));
    } else {
        caps.anonymous = Some(true);
        builder = builder.with_authenticator(Box::new(NoneAuthenticator::new()));
    }
    let runtime = builder.with_capabilities(caps).build().await?;

    let listener = tokio::net::TcpListener::bind(bind).await?;
    let local = listener.local_addr()?;
    println!(
        "arcp serve: listening on ws://{local}/  (protocol {})",
        arcp_core::PROTOCOL_VERSION
    );

    let serve_loop = async {
        loop {
            let (sock, peer) = listener.accept().await?;
            tracing::info!(%peer, "accepted connection");
            let ws = match tokio_tungstenite::accept_async(sock).await {
                Ok(ws) => ws,
                Err(e) => {
                    tracing::warn!(error = %e, "ws handshake failed");
                    continue;
                }
            };
            let transport = WebSocketTransport::accept_stream(ws);
            let _h = runtime.serve_connection(transport);
        }
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    };

    tokio::select! {
        result = serve_loop => result?,
        _ = signal::ctrl_c() => {
            println!("\narcp serve: ctrl-c received, shutting down");
            // Brief grace period for in-flight handshakes / writers.
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    Ok(())
}
