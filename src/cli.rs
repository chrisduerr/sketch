use std::path::PathBuf;

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Options {
    /// Output file.
    #[structopt(short, long)]
    pub output: Option<PathBuf>,
}
