use crate::builder::Builder;
use crate::clean_path::clean_path;

use anyhow::{Context, Result};
use clap::Parser;
use fs2::FileExt;
use nix_script_directives::expr::Expr;
use nix_script_directives::Directives;
use std::env;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

// TODO: Options for the rest of the directives.
#[derive(Debug, Parser)]
#[clap(version, trailing_var_arg = true)]
pub struct Opts {
    /// What indicator do directives start with in the source file?
    #[clap(long, default_value = "#!")]
    indicator: String,

    /// How should we build this script? (Will override any `#!build` line
    /// present in the script.)
    #[clap(long)]
    build_command: Option<String>,

    /// Add build inputs to those specified by the source directives.
    #[clap(long("build-input"))]
    build_inputs: Vec<String>,

    /// Run the script by passing it to this interpreter instead of running
    /// the compiled binary directly. The interpreter must be included via some
    /// runtime input.
    #[clap(long("interpreter"))]
    interpreter: Option<String>,

    /// Add runtime inputs to those specified by the source directives.
    #[clap(long("runtime-input"))]
    runtime_inputs: Vec<String>,

    /// Override the configuration that will be passed to nixpkgs on import.
    #[clap(
        long("nixpkgs-config"),
        value_parser = clap::value_parser!(Expr),
        env("NIX_SCRIPT_NIXPKGS_CONFIG")
    )]
    nixpkgs_config: Option<Expr>,

    /// Instead of executing the script, parse directives from the file and
    /// print them as JSON to stdout.
    #[clap(long("parse"), conflicts_with_all(&["export", "shell"]))]
    parse: bool,

    /// Instead of executing the script, print the derivation we build
    /// to stdout.
    #[clap(long("export"), conflicts_with_all(&["parse", "shell"]))]
    export: bool,

    /// Enter a shell with build-time and runtime inputs available.
    #[clap(long, conflicts_with_all(&["parse", "export"]))]
    shell: bool,

    /// In shell mode, run this command instead of a shell.
    #[clap(long, requires("shell"))]
    run: Option<String>,

    /// In shell mode, run a "pure" shell (that is, one that isolates the
    /// shell a little more from what you have in your environment.)
    #[clap(long, requires("shell"))]
    pure: bool,

    /// Use this folder as the root for any building we do. You can use this
    /// to bring other files into scope in your build. If there is a `default.nix`
    /// file in the specified root, we will use that instead of generating our own.
    #[clap(long)]
    build_root: Option<PathBuf>,

    /// Include files for use at runtime (relative to the build root).
    #[clap(long)]
    runtime_files: Vec<PathBuf>,

    /// Where should we cache files?
    #[clap(long("cache-directory"), env("NIX_SCRIPT_CACHE"))]
    cache_directory: Option<PathBuf>,

    /// The script to run (required), plus any arguments (optional). Any positional
    /// arguments after the script name will be passed on to the script.
    // Note: it'd be better to have a "script" and "args" field separately,
    // but there's a parsing issue in Clap (not a bug, but maybe a bug?) that
    // prevents passing args starting in -- after the script if we do that. See
    // https://github.com/clap-rs/clap/issues/1538
    #[clap(num_args = 1.., required = true)]
    script_and_args: Vec<String>,
}

impl Opts {
    pub fn run(&self) -> Result<ExitStatus> {
        // First things first: what are we running? Where does it live? What
        // are its arguments?
        let (mut script, args) = self
            .parse_script_and_args()
            .context("could not parse script and args")?;
        script = clean_path(&script).context("could not clean path to script")?;

        if self.shell && !args.is_empty() {
            log::warn!("You specified both `--shell` and script args. I am going to ignore the args! Use `--run` if you want to run something in the shell immediately.");
        }

        let script_name = script
            .file_name()
            .context("script did not have a file name")?
            .to_str()
            .context("filename was not valid UTF-8")?;

        // Parse our directives, but don't combine them with command-line arguments yet!
        let mut directives = Directives::from_file(&self.indicator, &script)
            .context("could not parse directives from script")?;

        let mut build_root = self.build_root.to_owned();
        if build_root.is_none() {
            if let Some(from_directives) = &directives.build_root {
                let out = script
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));

                out.join(from_directives)
                    .canonicalize()
                    .context("could not canonicalize final path to build root")?;

                log::debug!("path to root from script directive: {}", out.display());

                build_root = Some(out);
            }
        };
        if build_root.is_none()
            && (!self.runtime_files.is_empty() || !directives.runtime_files.is_empty())
        {
            log::warn!("Requested runtime files without specifying a build root. I am assuming it is the parent directory of the script for now, but you should set it explicitly!");
            build_root = Some(
                script
                    .parent()
                    .map(|p| p.to_owned())
                    .unwrap_or_else(|| PathBuf::from(".")),
            );
        }

        let mut builder = if let Some(build_root) = &build_root {
            Builder::from_directory(build_root, &script)
                .context("could not initialize source in directory")?
        } else {
            Builder::from_script(&script)
        };

        // First place we might bail early: if a script just wants to parse
        // directives using our parser, we dump JSON and quit instead of running.
        if self.parse {
            println!(
                "{}",
                serde_json::to_string(&directives).context("could not serialize directives")?
            );
            return Ok(ExitStatus::from_raw(0));
        }

        // We don't merge command-line and script directives until now because
        // we shouldn't provide them in the output of `--parse` without showing
        // where each option came from. For now, we're assuming that people who
        // write wrapper scripts know what they want to pass into `nix-script`.
        directives.maybe_override_build_command(&self.build_command);
        directives
            .merge_build_inputs(&self.build_inputs)
            .context("could not add build inputs provided on the command line")?;
        if let Some(interpreter) = &self.interpreter {
            directives.override_interpreter(interpreter)
        }
        directives
            .merge_runtime_inputs(&self.runtime_inputs)
            .context("could not add runtime inputs provided on the command line")?;
        directives.merge_runtime_files(&self.runtime_files);
        if let Some(expr) = &self.nixpkgs_config {
            directives
                .override_nixpkgs_config(expr)
                .context("could not set nixpkgs config provided on the command line")?;
        }

        // Second place we might bail early: if we're requesting a shell instead
        // of building and running the script.
        if self.shell {
            return self.run_shell(script, &directives);
        }

        // Third place we can bail early: if someone wants the generated
        // derivation to do IFD or similar.
        if self.export {
            // We check here instead of inside while isolating the script or
            // similar so we can get an early bail that doesn't create trash
            // in the system's temporary directories.
            if build_root.is_none() {
                anyhow::bail!(
                    "I do not have a root to refer to while exporting, so I cannot isolate the script and dependencies. Specify a --build-root and try this again!"
                )
            }

            println!(
                "{}",
                builder
                    .derivation(&directives, true)
                    .context("could not create a Nix derivation from the script")?
            );
            return Ok(ExitStatus::from_raw(0));
        }

        let cache_directory = self
            .get_cache_directory()
            .context("could not get cache directory")?;
        log::debug!(
            "using `{}` as the cache directory",
            cache_directory.display()
        );

        // Create hash, check cache.
        let hash = builder
            .hash(&directives)
            .context("could not calculate cache location for the compiled versoin of the script")?;

        let target_unique_id = format!("{hash}-{script_name}");
        let target = cache_directory.join(target_unique_id.clone());
        log::trace!("cache target: {}", target.display());

        // Before we perform the build, we need to check if the symlink target
        // has gone stale. This can happen when you run `nix-collect-garbage`,
        // since we don't pin the resulting derivations. We have to do things
        // in a slightly less ergonomic way in order to not follow symlinks.
        if fs::symlink_metadata(&target).is_ok() {
            let link_target = fs::read_link(&target).context("failed to read existing symlink")?;

            if !link_target.exists() {
                log::info!("removing stale (garbage-collected?) symlink");
                fs::remove_file(&target).context("could not remove stale symlink")?;
            }
        }

        if !target.exists() {
            log::debug!("hashed path does not exist; building");

            // Initialize build lock.
            //
            // We lock the build after checking for the target. This has the
            // advantage that all subsequent executions will not bother with
            // creating lock files and obtaining locks. However, it has the
            // disadvantage that we always move on to building the derivation,
            // even when another builder has done the job for us in the
            // meantime.
            let lock_file_path = env::temp_dir().join(target_unique_id);
            log::debug!("creating lock file path: {:?}", lock_file_path);
            let lock_file =
                File::create(lock_file_path.clone()).context("could not create lock file")?;
            log::debug!("locking");
            // Obtain lock.
            // TODO: Obtain lock with timeout.
            lock_file
                .lock_exclusive()
                .context("could not obtain lock")?;
            log::debug!("obtained lock");

            let out_path = builder
                .build(&cache_directory, &hash, &directives)
                .context("could not build derivation from script")?;

            if let Err(err) = symlink(out_path, &target) {
                match err.kind() {
                    ErrorKind::AlreadyExists => {
                        // We could hypothetically detect if the link is
                        // pointing to the right location, but the Nix paths
                        // change for minor reasons that don't matter for script
                        // execution. Instead, we just warn here and trust our
                        // cache key to do the right thing. If we get a
                        // collision, we do!
                        log::warn!("detected a parallel write to the cache");
                    }
                    _ => return Err(err).context("could not create symlink in cache"),
                }
            }

            // Make sure that we remove the temporary build directory before releasing the lock.
            drop(builder);
            // Release lock.
            log::debug!("releasing lock");
            fs2::FileExt::unlock(&lock_file).context("could not release lock")?;
            // Do not remove the lock file because other tasks may still be
            // waiting for obtaining a lock on the file.
        } else {
            log::debug!("hashed path exists; skipping build");
        }

        let mut child = Command::new(target.join("bin").join(script_name))
            .args(args)
            .spawn()
            .context("could not start the script")?;

        child.wait().context("could not run the script")
    }

    fn parse_script_and_args(&self) -> Result<(PathBuf, Vec<String>)> {
        log::trace!("parsing script and args");
        let mut script_and_args = self.script_and_args.iter();

        let script = PathBuf::from(
            script_and_args
                .next()
                .context("no script name; this is a bug; please report")?,
        );

        Ok((script, self.script_and_args[1..].to_vec()))
    }

    fn get_cache_directory(&self) -> Result<PathBuf> {
        let mut target = match &self.cache_directory {
            Some(explicit) => explicit.to_owned(),
            None => {
                let dirs = directories::ProjectDirs::from("zone", "bytes", "nix-script").context(
                    "couldn't load HOME (set --cache-directory explicitly to get around this.)",
                )?;

                dirs.cache_dir().to_owned()
            }
        };

        if target.is_relative() {
            target = std::env::current_dir()
                .context("no the current directory while calculating absolute path to the cache")?
                .join(target)
        }

        if !target.exists() {
            log::trace!("creating cache directory");
            std::fs::create_dir_all(&target).context("could not create cache directory")?;
        }

        Ok(target)
    }

    fn run_shell(&self, script_file: PathBuf, directives: &Directives) -> Result<ExitStatus> {
        log::debug!("entering shell mode");

        let mut command = Command::new("nix-shell");

        log::trace!("setting SCRIPT_FILE to `{}`", script_file.display());
        command.env("SCRIPT_FILE", script_file);

        if self.pure {
            log::trace!("setting shell to pure mode");
            command.arg("--pure");
        }

        for input in &directives.build_inputs {
            log::trace!("adding build input `{}` to packages", input);
            command.arg("-p").arg(input.to_string());
        }

        for input in &directives.runtime_inputs {
            log::trace!("adding runtime input `{}` to packages", input);
            command.arg("-p").arg(input.to_string());
        }

        if let Some(run) = &self.run {
            log::trace!("running `{}`", run);
            command.arg("--run").arg(run);
        }

        command
            .spawn()
            .context("could not start nix-shell")?
            .wait()
            .context("could not start the shell")
    }
}
