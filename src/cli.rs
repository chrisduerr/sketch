use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, about, version)]
pub struct Options {
    /// output file
    #[clap(short, long)]
    pub output: Option<PathBuf>,
}
