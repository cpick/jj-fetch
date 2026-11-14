use std::process::ExitCode;

use clap::Parser;

/// Fetch all remotes of every Jujutsu repository in a directory.
#[derive(Parser)]
#[command(version, about)]
struct Cli {}

fn main() -> ExitCode {
    let Cli {} = Cli::parse();
    ExitCode::SUCCESS
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
