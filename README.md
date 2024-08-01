This version of `nix-script` is a fork of the [original version by Brian
Hicks](https://github.com/BrianHicks/nix-script) which has been flagged
unmaintained. I am fixing bugs and regressions caused by the Rust ecosystem
moving on, but I am currently not adding new functionality.

Contact me, if you want to participate and improve `nix-script`!

Version 3.0.0 is the first release by myself (Dominik Schrempf). See the
[Changelog](./CHANGELOG.md) for more information.

# Nix-script

With `nix-script`, you can
- write quick scripts in compiled languages,
- transparently compile and cache them, and
- pull in whatever dependencies you need from the Nix ecosystem.

Please also see the [blog post by the original author Brian Hicks explaining
`nix-script`](https://bytes.zone/posts/nix-script/).

# Installation

## Installation to your profile

```
nix-env -if https://github.com/dschrempf/nix-script/archive/main.tar.gz
```

This project's CI also pushes Linux and macOS builds to
[`dschrempf-nix-script.cachix.org`](https://app.cachix.org/cache/dschrempf-nix-script)
automatically, meaning `cachix add dschrempf-nix-script` should set you up to
compile fewer things.

## Installation with Flakes

We provide these `package` attributes:

- `nix-script-all` (the default package): contains everything below
- `nix-script`: only `nix-script`
- `nix-script-bash`: only `nix-script-bash`
- `nix-script-haskell`: only `nix-script-haskell`

We also provide a Nixpkgs `overlay`, which has all of these.

## Installation with Niv

1. Add `nix-script` to Niv with `niv add BrianHicks/nix-script`.
2. Then, `import sources.nix-script { };`
3. You have the same things described in the Flakes section above, except you
   will have to explicitly reference things like
   `overlay."${builtins.currentSystem}"`.

# Commands

## `nix-script`

The normal `nix-script` invocation is controlled using shebang directives (lines
starting with `#!` by default, although you can change the indicator with the
`--indicator` flag).

Starting your file with `#!/usr/bin/env nix-script` makes these options
available:

| What?                                 | Shebang line      | Notes                                                                             |
|---------------------------------------|-------------------|-----------------------------------------------------------------------------------|
| How to compile the script to a binary | `#!build`         | The command specified here must read from `$SRC` and write to `$OUT`              |
| Use all files in the given directory  | `#!buildRoot`     | Must be a parent directory of the script                                          |
| Specify build-time dependencies       | `#!buildInputs`   | A space-separated list of Nix expressions                                         |
| Use an alternative interpreter        | `#!interpreter`   | Run this script with the given binary (must be in `runtimeInputs`)                |
| Specify runtime dependencies          | `#!runtimeInputs` | This should be a space-separated list of Nix expressions.                         |
| Access auxillary files at runtime     | `#!runtimeFiles`  | Make these files available at runtime (at the path given in `RUNTIME_FILES_ROOT`) |

You can also control these options with equivalent command-line flags to
`nix-script` (see the `--help` output for exact names).

`nix-script` also lets your compiled script know the original location by
setting the `SCRIPT_FILE` environment variable to what you would have gotten in
`$0` if it had been a shell script.

### Shell Mode

Building a new version for every change can get tiresome while developing. If
you want a quicker feedback loop, you can include `--shell` in your `nix-script`
invocation (e.g. `nix-script --shell path/to/script`) to drop into a development
shell with your build-time and runtime dependencies. This won't run your build
command, but it will let you run it yourself, play around in REPLs, etc.

If you are making a wrapper script, you may find the `--run` flag useful: it
allows you to specify what command to run in the shell. If your language
ecosystem has some common watcher script, it might be nice to add a special mode
to your wrapper! (For example, `nix-script-haskell` has a `--ghcid` flag for
this purpose).

### Exporting a script

Version 2 of `nix-script` introduced two new flags: `--build-root` and
`--export` to handle multiple files. In detail, if your script needs multiple
files, tell `nix-script` about the project root with `#!buildRoot` (or
`--build-root`), and it will include all the files in that directory during
builds.

You can also export (`--export`) the Nix derivation `default.nix` created by
`nix-script`. If you put that file (or any `default.nix`) in your build root,
`nix-script` will use that one instead of generating a new one.

Once you get to the point of having a directory with a `default.nix`, you have
arrived at a "real" derivation, and you may use any Nix tooling to further
modify your project.

### Parsing Directives

If you are making a wrapper script for a new language, you can also use
`--build-root` to hold package manager files and custom `build.nix` files. We
also provide a `--parse` flag which will ask `nix-script` to parse any
directives in the script and give them to you as JSON on stdout.

**Caution:** be aware that the format here is not stable yet. If you have any
feedback on the data returned by `--parse`, please open an issue!

## `nix-script-bash`

`nix-script-bash` lets you specify dependencies of Bash scripts. For example:

```bash
#!/usr/bin/env nix-script-bash
#!runtimeInputs jq

jq --help
```

## `nix-script-haskell`

`nix-script-haskell` is a convenience wrapper for Haskell scripts. In addition
to the regular `nix-script` options, `nix-script-haskell` has some
Haskell-specific options:

| Shebang line        | Notes                                                                                                         | Example                        |
|---------------------|---------------------------------------------------------------------------------------------------------------|--------------------------------|
| `#!haskellPackages` | Haskell dependencies (you can get a list of available packages with `nix-env -qaPA nixpkgs.haskellPackages`.) | `#!haskellPackages text aeson` |
| `#!ghcFlags`        | Additional compiler flags.                                                                                    | `#!ghcFlags -threaded`         |

You can get compilation feedback with [`ghcid`](https://github.com/ndmitchell/ghcid) by running `nix-script-haskell --ghcid path/to/your/script.hs`.

# Controlling the Nixpkgs version

`nix-script` will generate derivations that `import <nixpkgs> {}` by default.
This means your scripts are built with the Nixpkgs version set in the `NIX_PATH`
environment variable.

For example, you can use a specific Nixpkgs version available in your Nix store with

```
NIX_PATH=nixpkgs=/nix/store/HASHHASHHASH-source
```

The `NIX_PATH` environment variable is included in cache key calculations, so if
you change your package set your scripts will automatically be rebuilt the next
time you run them.

# Climate Action

The original author Brian Hicks has added the following note which I fully
support:

> I want my open-source work to support projects addressing the climate crisis
> (for example, projects in clean energy, public transit, reforestation, or
> sustainable agriculture.) If you are working on such a project, and find a bug
> or missing feature in any of my libraries, **please let me know and I will
> treat your issue as high priority.** I'd also be happy to support such
> projects in other ways, just ask!

# License

`nix-script` is licensed under the BSD 3-Clause license, located at `LICENSE`.
