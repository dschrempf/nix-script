mod builder;
mod clean_path;
mod derivation;
mod opts;

use clap::Parser;
use opts::Opts;

fn main() {
    env_logger::Builder::from_env("NIX_SCRIPT_LOG").init();

    let opts = Opts::parse();
    log::trace!("opts: {opts:?}");

    match opts.run().map(|status| status.code()) {
        Ok(Some(code)) => std::process::exit(code),
        Ok(None) => {
            log::warn!("No exit code; was the script killed with a signal?");
            std::process::exit(1)
        }
        Err(err) => {
            eprintln!("{err:?}");
            std::process::exit(1)
        }
    }
}
