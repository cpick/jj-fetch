mod discover;
mod log;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use crate::discover::Discovery;
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

    use super::Cli;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }
}
