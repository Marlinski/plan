mod cli;
mod epic;
mod session;
mod state;
mod ticket;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli::run(cli)
}
