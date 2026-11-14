# jj-fetch

Fetches the bookmarks of all remotes of every [Jujutsu](https://jj-vcs.dev) repository in some
directories, once or periodically while the Mac is plugged in.

## Usage

```sh
jj-fetch list ~/src            # the repositories that would be fetched
jj-fetch fetch ~/src           # fetch them all once
jj-fetch power                 # the current power source, according to IOKit
jj-fetch daemon ~/src          # fetch every 15 minutes, while on AC power
```

For each repository, `jj-fetch` runs:

```sh
jj --repository <workspace> --ignore-working-copy git fetch --all-remotes
```

- **Which repositories:** each directory, and its subdirectories up to `--max-depth` (default 3)
  levels below it, are searched for jj workspaces. The search doesn't descend into workspaces or
  follow symlinks. A repository with several workspaces is fetched once.
- **Which bookmarks:** all bookmarks of every remote, unless `remotes.<name>.fetch-bookmarks` in
  your jj configuration (or the remote's Git refspecs) say otherwise.
- **The working copy** is neither snapshotted nor updated, so a background fetch never touches
  files you're editing. If a fetch abandons commits your working-copy commit was based on, jj will
  ask you to run `jj workspace update-stale`.
- **Concurrency and timeouts:** up to `--jobs` (default 4) repositories are fetched at once, and a
  fetch running longer than `--timeout` (default 5m) is stopped. Git won't prompt for credentials.
- **The daemon** checks the power source every `--check-interval` (default 1m), and fetches when on
  AC power (not battery or UPS) and `--interval` (default 15m) has passed since the last fetch, so
  it catches up within a minute of being plugged in. Fetches that changed nothing aren't logged.

Run `jj-fetch help <command>` for all options.

## nix-darwin

The flake's `darwinModules.default` runs the daemon for each configured user as a launchd daemon
(in `/Library/LaunchDaemons`), so it starts at boot and runs whether or not the user is logged in:

```nix
{
  inputs.jj-fetch.url = "<this flake>";

  outputs = { nix-darwin, jj-fetch, ... }: {
    darwinConfigurations.my-mac = nix-darwin.lib.darwinSystem {
      modules = [
        jj-fetch.darwinModules.default
        {
          services.jj-fetch = {
            enable = true;
            users.alice.directories = [ "/Users/alice/src" ];
            users.bob = {
              directories = [ "/Users/bob/code" ];
              interval = "1h";
            };
          };
        }
      ];
    };
  };
}
```

Each user's daemon runs as that user, with `HOME` set to their home directory, Nix's `git` and then
the system's `ssh` on `PATH`, and logs to `~/Library/Logs/jj-fetch.log`. See
[the module](nix/darwin-module.nix) for all options, including `interval`, `maxDepth`, `extraArgs`,
`logFile`, `jujutsuPackage` and `gitPackage`.

Since the daemon may run while nobody is logged in, it can't use an SSH agent or prompt for a key's
passphrase. Fetching over SSH needs a key without a passphrase, and over HTTPS a credential helper
that works non-interactively. (A passphrase saved in the macOS keychain, via `UseKeychain` in
`~/.ssh/config`, only works while the user is logged in and their keychain is unlocked.)
Directories protected by macOS privacy controls, like `~/Documents`, can't be searched.

## Development

```sh
nix develop          # a shell with Rust, jj, and git
cargo test           # unit tests, and end-to-end tests with real jj and git repositories
nix flake check      # the tests, clippy, rustfmt, and a nix-darwin evaluation of the module
```
