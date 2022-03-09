mod derivation;
mod directives;

use crate::derivation::Derivation;
use crate::directives::Directives;
use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(version, trailing_var_arg = true)]
struct Opts {
    /// What indicator do directives start with in the source file?
    #[clap(long, default_value = "#!")]
    indicator: String,

    /// The script to run
    script: PathBuf,

    /// Any positional arguments after the script name will be passed on to
    /// the script.
    script_args: Vec<String>,
}

impl Opts {
    fn run(&self) -> Result<()> {
        let source = fs::read_to_string(&self.script).context("could not read script")?;

        println!(
            "{:#?}",
            Directives::parse(&self.indicator, &source)
                .context("could not construct a directive parser")?
        );

        let derivation =
            Derivation::new(&self.script).context("could not create a Nix derivation")?;
        println!("{:#?}", derivation);
        println!("{}", derivation);

        Ok(())
    }
}

fn main() {
    let opts = Opts::parse();

    if let Err(err) = opts.run() {
        eprintln!("{:?}", err);
        std::process::exit(1)
    }
}