{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";

    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";

    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    {
      self,
      flake-utils,
      naersk,
      nixpkgs,
    }:
    {
      overlay =
        final: prev:
        let
          naerskLib = naersk.lib."${final.system}";
          nixScript = naerskLib.buildPackage rec {
            name = "nix-script";
            version = "3.0.0";

            root = ./.;

            nativeBuildInputs = [ prev.clippy ];

            preBuild = ''
              # Make sure the version of the packages and the Nix derivations match.
              grep -q -e 'version = "${version}"' ${name}/Cargo.toml || \
                (echo "Nix Flake version mismatch ${version}!" && exit 1)
            '';

            doCheck = true;
            checkPhase = ''
              cargo clippy -- --deny warnings
            '';
          };
          nixScriptBash = prev.writeShellScriptBin "nix-script-bash" ''
            exec ${nixScript}/bin/nix-script \
              --build-command 'cp $SRC $OUT' \
              --interpreter bash \
              "$@"
          '';
          nixScriptAll = prev.symlinkJoin {
            name = "nix-script-all";
            paths = [
              nixScript
              nixScriptBash
            ];
          };
        in
        {
          nix-script = nixScriptAll;
        };
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ self.overlay ];
        };
      in
      {
        packages = {
          nix-script = pkgs.nix-script;
        };

        defaultPackage = pkgs.nix-script;

        devShell = pkgs.mkShell {
          NIX_PKGS = nixpkgs;
          packages = with pkgs; [
            # Rust.
            cargo
            clippy
            rustc
            rustfmt
            rust-analyzer

            # External Cargo commands.
            cargo-audit
            cargo-edit
            cargo-udeps
          ];
        };
      }
    );
}
