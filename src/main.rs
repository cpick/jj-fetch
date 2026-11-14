mod discover;
mod fetch;
mod log;

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};

use crate::discover::Discovery;
use crate::fetch::{Fetcher, indent};
use crate::log::log;

/// Fetch all remotes of every Jujutsu repository in a directory.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch all remotes of every repository once
    Fetch(FetchArgs),
    /// List the repositories that would be fetched
    List(SearchArgs),
}

#[derive(Args)]
struct SearchArgs {
    /// Directories to search for Jujutsu repositories
    #[arg(required = true)]
    dirs: Vec<PathBuf>,

    /// How many levels of subdirectories to search below each directory
    #[arg(long, default_value_t = 3)]
    max_depth: usize,
}

#[derive(Args)]
struct FetchArgs {
    #[command(flatten)]
    search: SearchArgs,

    /// The `jj` executable to run
    #[arg(long, default_value = "jj")]
    jj: PathBuf,

    /// How many repositories to fetch at once
    #[arg(long, short, default_value = "4")]
    jobs: NonZeroUsize,

    /// How long to let a repository's fetch run before stopping it
    #[arg(long, default_value = "5m", value_parser = parse_duration)]
    timeout: Duration,
}

/// Parses a positive, human-friendly duration, like `90s` or `1h 30m`.
fn parse_duration(s: &str) -> Result<Duration, String> {
    match humantime::parse_duration(s) {
        Ok(Duration::ZERO) => Err("must be longer than zero".to_owned()),
        Ok(duration) => Ok(duration),
        Err(error) => Err(error.to_string()),
    }
}

impl FetchArgs {
    /// Fetches every repository found, logging the results, and returns whether all succeeded.
    fn run(&self) -> bool {
        let started = Instant::now();
        let discovery = self.search.discover();
        let workspaces: Vec<PathBuf> = discovery.repos.into_iter().map(|r| r.workspace).collect();
        let fetcher = Fetcher {
            jj: self.jj.clone(),
            timeout: self.timeout,
        };
        let failures = AtomicUsize::new(0);
        fetcher.fetch_all(&workspaces, self.jobs.get(), |workspace, result| {
            match result {
                // Keep the log quiet when there's nothing new.
                Ok(output) if output.trim_end().ends_with("\nNothing changed.") => {}
                Ok(output) => log!("fetched {}{}", workspace.display(), indent(&output)),
                Err(error) => {
                    failures.fetch_add(1, Ordering::Relaxed);
                    log!("couldn't fetch {}: {error}", workspace.display());
                }
            }
        });
        let failures = failures.into_inner();
        log!(
            "fetched {} of {} repositories in {}s",
            workspaces.len() - failures,
            workspaces.len(),
            started.elapsed().as_secs(),
        );
        failures == 0 && discovery.errors.is_empty()
    }
}

impl SearchArgs {
    /// Finds the repositories to fetch, logging any paths that couldn't be searched.
    fn discover(&self) -> Discovery {
        let discovery = discover::discover(&self.dirs, self.max_depth);
        for (path, error) in &discovery.errors {
            log!("couldn't search {}: {error}", path.display());
        }
        discovery
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Fetch(fetch) => exit_code(fetch.run()),
        Command::List(search) => {
            let discovery = search.discover();
            for repo in &discovery.repos {
                println!("{}", repo.workspace.display());
            }
            exit_code(discovery.errors.is_empty())
        }
    }
}

fn exit_code(success: bool) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_positive_durations() {
        assert_eq!(parse_duration("90s"), Ok(Duration::from_secs(90)));
        assert_eq!(parse_duration("1h 30m"), Ok(Duration::from_secs(90 * 60)));
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("soon").is_err());
    }
}
