//! Standalone `tunnelmuxd` entry point.
//!
//! All of the daemon lives in the library crate so the desktop app can host the
//! exact same control plane in-process. This binary is the headless path: it
//! parses flags, then hands them to the shared implementation.

use clap::Parser;

use tunnelmuxd::{DaemonArgs, init_tracing, serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    serve(DaemonArgs::parse()).await
}
