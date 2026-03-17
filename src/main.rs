mod cli;
mod epic;
mod hub;
mod session;
mod state;
mod ticket;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli::run(cli)
}
