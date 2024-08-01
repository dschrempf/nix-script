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
    let
      rustTarget =
        {
          name,
          version,
          pkgs,
          naerskLib,
          postInstall ? "",
        }:
        naerskLib.buildPackage {
          inherit name;
          inherit version;
          inherit postInstall;

          root = ./.;

          buildInputs = [
            pkgs.clippy
            pkgs.makeWrapper
          ];
          target = [ name ];

          doCheck = true;
          checkPhase = ''
            cargo clippy -- --deny warnings

            # Make sure the version of the packages and the Nix derivations match.
            grep -q -e 'version = "${version}"' ${name}/Cargo.toml || \
              (echo "Nix Flake version mismatch ${version}!" && exit 1)
          '';

          copyBinsFilter = ''select(.reason == "compiler-artifact" and .executable != null and .profile.test == false and .target.name == "${name}")'';
        };

      mkNixScript =
        pkgs: naerskLib:
        rustTarget {
          name = "nix-script";
          version = "3.0.0";
          inherit pkgs;
          naerskLib = naerskLib;
        };

      mkNixScriptBash =
        pkgs:
        pkgs.writeShellScriptBin "nix-script-bash" ''
          exec ${pkgs.nix-script}/bin/nix-script \
            --build-command 'cp $SRC $OUT' \
            --interpreter bash \
            "$@"
        '';

      mkNixScriptHaskell =
        pkgs: naerskLib:
        rustTarget rec {
          name = "nix-script-haskell";
          version = "3.0.0";
          inherit pkgs;
          inherit naerskLib;

          postInstall = ''
            # avoid trying to package the dependencies step
            if test -f $out/bin/${name}; then
              wrapProgram $out/bin/${name} \
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.nix-script ]}
            fi
          '';
        };

      mkNixScriptAll =
        pkgs:
        pkgs.symlinkJoin {
          name = "nix-script-all";
          paths = [
            pkgs.nix-script
            pkgs.nix-script-haskell
            pkgs.nix-script-bash
          ];
        };
    in
    {
      overlay =
        final: prev:
        let
          naerskLib = naersk.lib."${final.system}";
        in
        {
          nix-script = mkNixScript prev naerskLib;
          nix-script-bash = mkNixScriptBash final;
          nix-script-haskell = mkNixScriptHaskell final naerskLib;
          nix-script-all = mkNixScriptAll final;
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
          nix-script-bash = pkgs.nix-script-bash;
          nix-script-haskell = pkgs.nix-script-haskell;
          nix-script-all = pkgs.nix-script-all;
        };

        defaultPackage = pkgs.nix-script-all;

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

            # System.
            libiconv
          ];
        };
      }
    );
}
