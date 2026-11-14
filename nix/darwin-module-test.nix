# Evaluates a nix-darwin system using the module, and checks the launchd daemons it defines.
{
  pkgs,
  nix-darwin,
  module,
}:

let
  system = nix-darwin.lib.darwinSystem {
    modules = [
      module
      {
        nixpkgs.hostPlatform = pkgs.stdenv.hostPlatform.system;
        system.stateVersion = 6;
        users.users.alice.home = "/Users/alice-home";
        services.jj-fetch = {
          enable = true;
          users.alice.directories = [ "/Users/alice-home/src" ];
          users.bob = {
            directories = [
              "/Users/bob/code"
              "/Users/bob/work stuff"
            ];
            interval = "1h";
            maxDepth = 5;
          };
        };
      }
    ];
  };
  inherit (system.config) environment;
  plist = user: environment.launchDaemons."org.nixos.jj-fetch-${user}.plist".source;
in
pkgs.runCommand "jj-fetch-darwin-module-test"
  {
    nativeBuildInputs = [ pkgs.python3 ];
    alice = plist "alice";
    bob = plist "bob";
    daemons = builtins.toJSON (builtins.attrNames system.config.launchd.daemons);
    hasPackage = builtins.toJSON (
      builtins.any (p: (p.pname or "") == "jj-fetch") environment.systemPackages
    );
    jj = pkgs.lib.getExe pkgs.jujutsu;
    git = "${pkgs.git}/bin";
  }
  ''
    python3 ${./darwin-module-test.py}
    touch $out
  ''
