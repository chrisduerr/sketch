use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, about, version)]
pub struct Options {
    /// Existing sketch file.
    #[clap(short, long)]
    pub file: Option<PathBuf>,
    /// Output file.
    #[clap(short, long)]
    pub output: Option<PathBuf>,
}
