# Runs `jj-fetch daemon` for each configured user, as a launchd daemon, from boot, regardless of
# whether the user is logged in.
{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  inherit (lib) mkOption types;
  cfg = config.services.jj-fetch;
  darwinUsers = config.users.users;

  userOptions =
    { name, config, ... }:
    {
      options = {
        directories = mkOption {
          type = types.nonEmptyListOf types.str;
          example = [ "/Users/alice/src" ];
          description = "Directories to search for Jujutsu repositories to fetch.";
        };

        interval = mkOption {
          type = types.str;
          default = cfg.interval;
          defaultText = lib.literalExpression "config.services.jj-fetch.interval";
          description = "How often to fetch while on AC power.";
        };

        maxDepth = mkOption {
          type = types.ints.unsigned;
          default = cfg.maxDepth;
          defaultText = lib.literalExpression "config.services.jj-fetch.maxDepth";
          description = "How many levels of subdirectories to search below each directory.";
        };

        home = mkOption {
          type = types.str;
          default =
            if darwinUsers ? ${name} && darwinUsers.${name}.home != null then
              darwinUsers.${name}.home
            else
              "/Users/${name}";
          defaultText = lib.literalExpression ''config.users.users.''${name}.home or "/Users/''${name}"'';
          description = ''
            The user's home directory, where `jj` and `git` look for their configuration, and
            `ssh` for its keys.
          '';
        };

        logFile = mkOption {
          type = types.str;
          default = "${config.home}/Library/Logs/jj-fetch.log";
          defaultText = lib.literalExpression ''"''${home}/Library/Logs/jj-fetch.log"'';
          description = "Where to write the daemon's log.";
        };

        extraArgs = mkOption {
          type = types.listOf types.str;
          default = [ ];
          example = [
            "--jobs"
            "8"
          ];
          description = "Additional arguments to `jj-fetch daemon`.";
        };
      };
    };
in
{
  options.services.jj-fetch = {
    enable = lib.mkEnableOption ''
      fetching all remotes of users' Jujutsu repositories periodically while on AC power, from
      boot, whether or not the users are logged in
    '';

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.jj-fetch;
      defaultText = lib.literalMD "the package from the jj-fetch flake";
      description = "The jj-fetch package to use.";
    };

    jujutsuPackage = lib.mkPackageOption pkgs "jujutsu" { };

    gitPackage = lib.mkPackageOption pkgs "git" {
      extraDescription = "Jujutsu runs `git` to fetch.";
    };

    interval = mkOption {
      type = types.str;
      default = "15m";
      example = "1h";
      description = "How often to fetch while on AC power, unless overridden for a user.";
    };

    maxDepth = mkOption {
      type = types.ints.unsigned;
      default = 3;
      description = ''
        How many levels of subdirectories to search below each directory, unless overridden for a
        user.
      '';
    };

    users = mkOption {
      type = types.attrsOf (types.submodule userOptions);
      default = { };
      example = lib.literalExpression ''
        {
          alice.directories = [ "/Users/alice/src" ];
          bob = {
            directories = [ "/Users/bob/code" ];
            interval = "1h";
          };
        }
      '';
      description = "The users to fetch repositories for, by user name.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    launchd.daemons = lib.mapAttrs' (
      user: userCfg:
      lib.nameValuePair "jj-fetch-${user}" {
        command = lib.escapeShellArgs (
          [
            (lib.getExe cfg.package)
            "daemon"
            "--jj"
            (lib.getExe cfg.jujutsuPackage)
            "--interval"
            userCfg.interval
            "--max-depth"
            (toString userCfg.maxDepth)
          ]
          ++ userCfg.extraArgs
          ++ [ "--" ]
          ++ userCfg.directories
        );
        environment = {
          HOME = userCfg.home;
          USER = user;
          # The system's `ssh` comes after Nix's `git`, so that it can use the macOS keychain.
          PATH = "${lib.makeBinPath [ cfg.gitPackage ]}:/usr/bin:/bin:/usr/sbin:/sbin";
        };
        serviceConfig = {
          UserName = user;
          RunAtLoad = true;
          KeepAlive = true;
          ProcessType = "Background";
          StandardOutPath = userCfg.logFile;
          StandardErrorPath = userCfg.logFile;
        };
      }
    ) cfg.users;
  };
}
