{
  lib,
  rustPlatform,
  git,
  jujutsu,
}:

let
  manifest = (lib.importTOML ../Cargo.toml).package;
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  inherit (manifest) version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      (lib.fileset.maybeMissing ../tests)
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  # The integration tests drive real `jj` and `git` binaries.
  nativeCheckInputs = [
    git
    jujutsu
  ];

  meta = {
    inherit (manifest) description;
    mainProgram = manifest.name;
    platforms = lib.platforms.darwin;
  };
}
