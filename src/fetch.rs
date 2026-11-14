//! Running `jj git fetch` in repositories.

use std::fmt;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

/// How often to check whether a fetch has finished.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// How long a timed-out fetch gets to clean up after `SIGTERM` before it's killed.
const TERMINATION_GRACE: Duration = Duration::from_secs(5);
/// How long to wait for output after a fetch exits, in case it left a process holding the pipe.
const OUTPUT_GRACE: Duration = Duration::from_secs(2);

/// Runs `jj git fetch` for all remotes of a repository.
pub struct Fetcher {
    /// The `jj` executable.
    pub jj: PathBuf,
    /// How long a fetch may take before it's stopped.
    pub timeout: Duration,
}

/// Why a fetch failed.
#[derive(Debug)]
pub enum FetchError {
    Spawn(io::Error),
    Wait(io::Error),
    Exit { status: ExitStatus, output: String },
    Timeout { output: String },
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Spawn(error) => write!(f, "couldn't run jj: {error}"),
            FetchError::Wait(error) => write!(f, "couldn't wait for jj: {error}"),
            FetchError::Exit { status, output } => write!(f, "jj {status}{}", indent(output)),
            FetchError::Timeout { output } => write!(f, "timed out{}", indent(output)),
        }
    }
}

impl std::error::Error for FetchError {}

/// Formats command output to follow a log message, one indented line per line of output.
pub fn indent(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("\n    {line}"))
        .collect()
}

impl Fetcher {
    /// Fetches the bookmarks of all remotes of the repository with a workspace at `workspace`,
    /// returning jj's output.
    ///
    /// The working copy is neither snapshotted nor updated, so the fetch can't interfere with
    /// edits in progress.
    pub fn fetch(&self, workspace: &Path) -> Result<String, FetchError> {
        let (reader, writer) = io::pipe().map_err(FetchError::Spawn)?;
        let mut command = Command::new(&self.jj);
        command
            .arg("--repository")
            .arg(workspace)
            .args([
                "--ignore-working-copy",
                "--no-pager",
                "--color=never",
                "git",
                "fetch",
                "--all-remotes",
            ])
            // Fail rather than wait for credentials that nobody is there to type.
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(writer.try_clone().map_err(FetchError::Spawn)?)
            .stderr(writer)
            // Its own process group, so a timeout can stop the `git` and `ssh` it runs, too.
            .process_group(0);
        let mut child = command.spawn().map_err(FetchError::Spawn)?;
        // Close our copies of the pipe's write end, so reading sees EOF once jj exits.
        drop(command);
        let output = read_in_background(reader);

        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(status) = child.try_wait().map_err(FetchError::Wait)? {
                let output = output_from(&output);
                return if status.success() {
                    Ok(output)
                } else {
                    Err(FetchError::Exit { status, output })
                };
            }
            if Instant::now() >= deadline {
                stop(&mut child).map_err(FetchError::Wait)?;
                return Err(FetchError::Timeout {
                    output: output_from(&output),
                });
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// Fetches each of `workspaces`, up to `jobs` at a time, calling `on_done` as each finishes.
    pub fn fetch_all<F>(&self, workspaces: &[PathBuf], jobs: usize, on_done: F)
    where
        F: Fn(&Path, Result<String, FetchError>) + Sync,
    {
        let next = AtomicUsize::new(0);
        thread::scope(|scope| {
            for _ in 0..jobs.clamp(1, workspaces.len().max(1)) {
                scope.spawn(|| {
                    while let Some(workspace) = workspaces.get(next.fetch_add(1, Ordering::Relaxed))
                    {
                        on_done(workspace, self.fetch(workspace));
                    }
                });
            }
        });
    }
}

fn read_in_background(mut reader: io::PipeReader) -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        let _ = reader.read_to_end(&mut output);
        let _ = sender.send(output);
    });
    receiver
}

fn output_from(receiver: &Receiver<Vec<u8>>) -> String {
    receiver
        .recv_timeout(OUTPUT_GRACE)
        .map(|output| String::from_utf8_lossy(&output).into_owned())
        .unwrap_or_default()
}

/// Stops `child`'s process group: politely at first, so `git` can remove its lock files, and then
/// forcefully.
fn stop(child: &mut Child) -> io::Result<()> {
    signal_group(child, libc::SIGTERM);
    let deadline = Instant::now() + TERMINATION_GRACE;
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
    signal_group(child, libc::SIGKILL);
    child.wait().map(drop)
}

fn signal_group(child: &Child, signal: libc::c_int) {
    // The child leads its own process group, so the group's ID is the child's ID.
    let group = libc::pid_t::try_from(child.id()).expect("process IDs fit in pid_t");
    // SAFETY: `kill` has no memory-safety preconditions. The group can't have been reused, since
    // the child hasn't been reaped.
    unsafe {
        libc::kill(-group, signal);
    }
}
