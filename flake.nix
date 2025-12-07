{
  description = "A code formatter for Nushell";

  inputs = {
    rust-overlay.url = "github:oxalica/rust-overlay";
    systems.url = "github:nix-systems/default";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      systems,
    }:

    let
      inherit (nixpkgs) lib;

      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      version = cargoToml.workspace.package.version;

      rustOverlays = [ (import rust-overlay) ];

      eachSystem = lib.flip lib.mapAttrs (
        lib.genAttrs (import systems) (
          system:
          import nixpkgs {
            inherit system;
            overlays = rustOverlays;
          }
        )
      );

      # Package builder that can be used with any pkgs
      mkNufmt =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "nufmt";
          inherit version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          meta = {
            description = "A code formatter for Nushell";
            homepage = "https://github.com/psychollama/nufmt";
            license = lib.licenses.mit;
            mainProgram = "nufmt";
          };
        };
    in

    {
      overlays.default = final: prev: {
        nufmt = mkNufmt final;
      };

      packages = eachSystem (
        system: pkgs: {
          nufmt = mkNufmt pkgs;

          docs = pkgs.rustPlatform.buildRustPackage {
            pname = "nufmt-docs";
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            buildPhase = ''
              cargo doc --no-deps --package nufmt-core
            '';

            installPhase = ''
              mkdir -p $out
              cp -r target/doc/* $out/
            '';

            meta = {
              description = "Documentation for nufmt";
              homepage = "https://github.com/psychollama/nufmt";
              license = lib.licenses.mit;
            };
          };

          default = self.packages.${system}.nufmt;
        }
      );

      devShells = eachSystem (
        system: pkgs: {
          default = pkgs.mkShell {
            packages = [
              (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
              pkgs.just
              pkgs.nixfmt-rfc-style
              pkgs.treefmt
            ];
          };
        }
      );
    };
}
