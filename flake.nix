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

      mkNixScript =
        pkgs: naerskLib:
        naerskLib.buildPackage rec {
          name = "nix-script";
          version = "3.0.0";

          root = ./.;

          buildInputs = [
            pkgs.clippy
            pkgs.makeWrapper
          ];

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

      mkNixScriptBash =
        pkgs:
        pkgs.writeShellScriptBin "nix-script-bash" ''
          exec ${pkgs.nix-script}/bin/nix-script \
            --build-command 'cp $SRC $OUT' \
            --interpreter bash \
            "$@"
        '';

      mkNixScriptAll =
        pkgs: naerskLib:
        pkgs.symlinkJoin {
          name = "nix-script-all";
          paths = [
            (mkNixScript pkgs naerskLib)
            (mkNixScriptBash pkgs)
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
          nix-script = mkNixScriptAll prev naerskLib;
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

            # System.
            libiconv
          ];
        };
      }
    );
}
