{
  description = "Fetch all remotes of every Jujutsu repository in a directory, periodically while on AC power";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;
      forAllSystems =
        f: lib.genAttrs [ "aarch64-darwin" "x86_64-darwin" ] (system: f nixpkgs.legacyPackages.${system});
    in
    {
      overlays.default = final: _prev: {
        jj-fetch = final.callPackage ./nix/package.nix { };
      };

      packages = forAllSystems (pkgs: {
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.jj-fetch;
        jj-fetch = pkgs.callPackage ./nix/package.nix { };
      });

      checks = forAllSystems (
        pkgs:
        let
          jj-fetch = self.packages.${pkgs.stdenv.hostPlatform.system}.jj-fetch;
        in
        {
          # Builds the crate and runs `cargo test`.
          inherit jj-fetch;

          clippy = jj-fetch.overrideAttrs (old: {
            pname = "${old.pname}-clippy";
            nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.clippy ];
            buildPhase = ''
              runHook preBuild
              cargo clippy --offline --all-targets -- --deny warnings
              runHook postBuild
            '';
            doCheck = false;
            installPhase = "touch $out";
          });

          rustfmt =
            pkgs.runCommand "jj-fetch-rustfmt"
              {
                nativeBuildInputs = [
                  pkgs.cargo
                  pkgs.rustfmt
                ];
              }
              ''
                cd ${jj-fetch.src}
                cargo fmt --check
                touch $out
              '';
        }
      );

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.jj-fetch ];
          packages = [
            pkgs.clippy
            pkgs.git
            pkgs.jujutsu
            pkgs.rust-analyzer
            pkgs.rustfmt
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
