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

      overlays = [ (import rust-overlay) ];

      eachSystem = lib.flip lib.mapAttrs (
        lib.genAttrs (import systems) (
          system:
          import nixpkgs {
            inherit system overlays;
          }
        )
      );
    in

    {
      packages = eachSystem (
        system: pkgs: {
          nufmt = pkgs.rustPlatform.buildRustPackage {
            pname = "nufmt";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = "A code formatter for Nushell";
              homepage = "https://github.com/psychollama/nufmt";
              license = lib.licenses.mit;
              mainProgram = "nufmt";
            };
          };

          docs = pkgs.rustPlatform.buildRustPackage {
            pname = "nufmt-docs";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            buildPhase = ''
              cargo doc --no-deps --package nufmt
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
            ];
          };
        }
      );
    };
}
