use anyhow::{Context, Result};
use clap::Parser;
use nix_script_directives::Directives;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

/// `nix-script-haskell` is a wrapper around `nix-script` with options for
/// scripts written in Haskell.
///
/// I pay attention to all the same #! directives as nix-script, so you can
/// still use `#!runtimeInputs` and friends to get external dependencies. (There
/// is no need to specify `#!build` or `#!buildInputs` with regards to GHC or
/// packages, though; I take care of that.)
///
/// In addition, I pay attention to some additional directives specific to
/// Haskell programs:
///
/// `#!haskellPackages` should contain a list of packages the compiling GHC
/// instance will know about. The available set of packages depends on your
/// Nix installation; look in `haskellPackages` on `search.nixos.org` to get a
/// full list.
///
/// `#!ghcFlags` should be a string of command-line options to pass to `ghc`
/// when compiling.
#[derive(Debug, Parser)]
#[clap(version, trailing_var_arg = true)]
pub struct Opts {
    /// Launch a ghcid session watching the script
    #[clap(long, conflicts_with("shell"))]
    ghcid: bool,

    /// Enter a shell with all script dependencies
    #[clap(long, conflicts_with("ghcid"))]
    shell: bool,

    /// In shell mode, run this command instead of a shell.
    #[clap(long, requires("shell"))]
    run: Option<String>,

    /// In shell mode, run a "pure" shell (that is, one that isolates the
    /// shell a little more from what you have in your environment.)
    #[clap(long, requires("shell"))]
    pure: bool,

    #[clap(long, default_value("nix-script"), hide(true))]
    nix_script_bin: PathBuf,

    /// The script and args to pass to nix-script
    #[arg(num_args = 1.., required = true)]
    script_and_args: Vec<String>,
}

impl Opts {
    pub fn run(&self) -> Result<ExitStatus> {
        let (script, args) = self
            .get_script_and_args()
            .context("could not get script and args")?;

        let directives = Directives::from_file("#!", &script)
            .context("could not parse directives from script")?;

        let mut nix_script = Command::new(&self.nix_script_bin);

        let build_command = format!(
            "mv $SRC $SRC.hs; ghc {} -o $OUT $SRC.hs",
            directives
                .all
                .get("ghcFlags")
                .map(|ps| ps.join(" "))
                .unwrap_or_default()
        );
        log::debug!("build command is `{}`", build_command);
        nix_script.arg("--build-command").arg(build_command);

        let compiler = format!(
            "haskellPackages.ghcWithPackages (ps: with ps; [ {} ])",
            directives
                .all
                .get("haskellPackages")
                .map(|ps| ps.join(" "))
                .unwrap_or_default()
        );
        log::debug!("compiler is `{}`", &compiler);
        nix_script.arg("--build-input").arg(compiler);

        if self.shell {
            log::debug!("entering shell mode");
            nix_script.arg("--shell");
        } else if self.ghcid {
            log::debug!("entering ghcid mode");
            nix_script
                .arg("--shell")
                .arg("--runtime-input")
                .arg("ghcid")
                .arg("--run")
                .arg(format!("ghcid {}", script.display()));
        }

        nix_script.arg(script);
        nix_script.args(args);

        let mut child = nix_script.spawn().with_context(|| {
            format!(
                "could not call {}. Is it on the PATH?",
                self.nix_script_bin.display()
            )
        })?;

        child.wait().context("could not run the script")
    }

    fn get_script_and_args(&self) -> Result<(PathBuf, Vec<String>)> {
        log::trace!("parsing script and args");

        let script = PathBuf::from(
            self.script_and_args
                .first()
                .context("no script to run; this is a bug; please report")?,
        );

        Ok((script, self.script_and_args[1..].to_vec()))
    }
}
