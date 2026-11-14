//! End-to-end tests of `jj-fetch fetch`, using real `jj` and `git` binaries from `PATH`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// A scratch directory with `jj` and `git` isolated from the user's configuration.
struct Env {
    _dir: TempDir,
    root: PathBuf,
}

impl Env {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir(root.join("home")).unwrap();
        fs::write(
            root.join("jj-config.toml"),
            "user.name = \"Test\"\nuser.email = \"test@example.com\"\n",
        )
        .unwrap();
        Env { _dir: dir, root }
    }

    fn command(&self, program: impl AsRef<std::ffi::OsStr>, cwd: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .env("HOME", self.root.join("home"))
            .env("JJ_CONFIG", self.root.join("jj-config.toml"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com");
        command
    }

    fn run(&self, program: &str, cwd: &Path, args: &[&str]) -> String {
        let output = self.command(program, cwd).args(args).output().unwrap();
        assert_success(&output, &format!("{program} {args:?}"));
        String::from_utf8(output.stdout).unwrap()
    }

    /// Creates a bare Git repository with a commit on each of `branches`.
    fn remote(&self, name: &str, branches: &[&str]) -> PathBuf {
        let remote = self.root.join(name);
        let source = self.root.join(format!("{name}-source"));
        self.run("git", &self.root, &["init", "--quiet", "--bare", name]);
        self.run(
            "git",
            &self.root,
            &["init", "--quiet", source.to_str().unwrap()],
        );
        for branch in branches {
            fs::write(source.join("file"), branch).unwrap();
            self.run("git", &source, &["add", "file"]);
            self.run("git", &source, &["commit", "--quiet", "--message", branch]);
            let refspec = format!("HEAD:refs/heads/{branch}");
            self.run(
                "git",
                &source,
                &["push", "--quiet", remote.to_str().unwrap(), &refspec],
            );
        }
        remote
    }

    /// Creates a jj repository at `path` with the given remotes.
    fn repo(&self, path: &Path, remotes: &[(&str, &Path)]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        self.run("jj", &self.root, &["git", "init", path.to_str().unwrap()]);
        for (name, url) in remotes {
            self.run(
                "jj",
                path,
                &["git", "remote", "add", name, url.to_str().unwrap()],
            );
        }
    }

    fn remote_bookmarks(&self, repo: &Path) -> Vec<String> {
        let template = r#"if(remote, name ++ "@" ++ remote ++ "\n")"#;
        let output = self.run(
            "jj",
            repo,
            &[
                "--ignore-working-copy",
                "bookmark",
                "list",
                "--all-remotes",
                "--template",
                template,
            ],
        );
        let mut bookmarks: Vec<String> = output
            .lines()
            .filter(|line| !line.ends_with("@git"))
            .map(str::to_owned)
            .collect();
        bookmarks.sort();
        bookmarks
    }

    fn jj_fetch(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_jj-fetch"), &self.root)
            .args(args)
            .output()
            .unwrap()
    }
}

fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn fetches_bookmarks_from_all_remotes_of_every_repository() {
    let env = Env::new();
    let origin = env.remote("origin.git", &["main", "release"]);
    let upstream = env.remote("upstream.git", &["feature"]);
    let search = env.root.join("search");
    let one = search.join("one");
    let two = search.join("group/two");
    env.repo(&one, &[("origin", &origin), ("upstream", &upstream)]);
    env.repo(&two, &[("upstream", &upstream)]);
    // A second workspace of `two` shouldn't cause a second fetch, or any harm.
    env.run(
        "jj",
        &two,
        &["workspace", "add", "--quiet", "../two-workspace"],
    );

    let output = env.jj_fetch(&["fetch", search.to_str().unwrap()]);

    assert_success(&output, "jj-fetch fetch");
    assert_eq!(
        env.remote_bookmarks(&one),
        ["feature@upstream", "main@origin", "release@origin"]
    );
    assert_eq!(env.remote_bookmarks(&two), ["feature@upstream"]);
}

#[test]
fn leaves_the_working_copy_alone() {
    let env = Env::new();
    let origin = env.remote("origin.git", &["main"]);
    let repo = env.root.join("search/repo");
    env.repo(&repo, &[("origin", &origin)]);
    fs::write(repo.join("unsnapshotted"), "edit in progress").unwrap();

    let output = env.jj_fetch(&["fetch", repo.to_str().unwrap()]);

    assert_success(&output, "jj-fetch fetch");
    assert_eq!(env.remote_bookmarks(&repo), ["main@origin"]);
    let diff = env.run("jj", &repo, &["--ignore-working-copy", "diff", "--summary"]);
    assert_eq!(diff, "", "the working copy was snapshotted");
}

#[test]
fn reports_failures_but_fetches_other_repositories() {
    let env = Env::new();
    let origin = env.remote("origin.git", &["main"]);
    let search = env.root.join("search");
    let broken = search.join("a-broken");
    let good = search.join("b-good");
    env.repo(&broken, &[("origin", &env.root.join("missing.git"))]);
    env.repo(&good, &[("origin", &origin)]);

    let output = env.jj_fetch(&["fetch", search.to_str().unwrap()]);

    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(broken.to_str().unwrap()),
        "the failure wasn't reported:\n{stderr}"
    );
    assert_eq!(env.remote_bookmarks(&good), ["main@origin"]);
}

#[test]
fn gives_up_on_fetches_that_take_too_long() {
    let env = Env::new();
    let repo = env.root.join("search/repo");
    env.repo(&repo, &[]);
    // Stands in for `jj`, hanging as a fetch might on an unresponsive network.
    let slow_jj = env.root.join("slow-jj");
    fs::write(&slow_jj, "#!/bin/sh\nexec sleep 60\n").unwrap();
    env.run("chmod", &env.root, &["+x", slow_jj.to_str().unwrap()]);

    let started = std::time::Instant::now();
    let output = env.jj_fetch(&[
        "fetch",
        "--jj",
        slow_jj.to_str().unwrap(),
        "--timeout",
        "1s",
        repo.to_str().unwrap(),
    ]);

    assert!(!output.status.success(), "{output:?}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "took {:?}",
        started.elapsed()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("timed out"), "{stderr}");
}
