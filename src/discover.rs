//! Finding Jujutsu workspaces within directories.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// A repository to fetch, identified by one of its workspaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// The root of the workspace through which the repository was found.
    pub workspace: PathBuf,
    /// The canonical path of the repository's store (`.jj/repo` of its main workspace).
    pub store: PathBuf,
}

/// The result of searching directories for repositories.
#[derive(Debug, Default)]
pub struct Discovery {
    /// Each repository found, once, in search order.
    pub repos: Vec<Repo>,
    /// Paths that couldn't be searched, and why.
    pub errors: Vec<(PathBuf, io::Error)>,
}

/// Searches each of `dirs`, and their subdirectories up to `max_depth` levels below them, for
/// Jujutsu workspaces.
///
/// The search doesn't descend into workspaces, `.git` or `.jj` directories, or symlinks.
/// Repositories with several workspaces are reported once, through the first workspace found.
pub fn discover(dirs: &[PathBuf], max_depth: usize) -> Discovery {
    let mut discovery = Discovery::default();
    let mut seen_stores = HashSet::new();
    for dir in dirs {
        match dir.canonicalize() {
            Ok(dir) => search(&dir, max_depth, &mut seen_stores, &mut discovery),
            Err(error) => discovery.errors.push((dir.clone(), error)),
        }
    }
    discovery
}

fn search(
    dir: &Path,
    depth_remaining: usize,
    seen_stores: &mut HashSet<PathBuf>,
    discovery: &mut Discovery,
) {
    match workspace_store(dir) {
        Ok(Some(store)) => {
            if seen_stores.insert(store.clone()) {
                discovery.repos.push(Repo {
                    workspace: dir.to_owned(),
                    store,
                });
            }
            return;
        }
        Ok(None) => {}
        Err(error) => {
            discovery.errors.push((dir.join(".jj/repo"), error));
            return;
        }
    }

    let Some(depth_remaining) = depth_remaining.checked_sub(1) else {
        return;
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            discovery.errors.push((dir.to_owned(), error));
            return;
        }
    };
    let mut subdirs = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                discovery.errors.push((dir.to_owned(), error));
                continue;
            }
        };
        // `DirEntry::file_type` doesn't follow symlinks.
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_dir && entry.file_name() != ".git" && entry.file_name() != ".jj" {
            subdirs.push(entry.path());
        }
    }
    subdirs.sort();
    for subdir in subdirs {
        search(&subdir, depth_remaining, seen_stores, discovery);
    }
}

/// Returns the canonical path of the store of the repository whose workspace is rooted at `dir`,
/// or `None` if `dir` isn't the root of a workspace.
fn workspace_store(dir: &Path) -> io::Result<Option<PathBuf>> {
    let jj_dir = dir.join(".jj");
    let repo = jj_dir.join("repo");
    let metadata = match fs::metadata(&repo) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let store = if metadata.is_dir() {
        repo
    } else {
        // A secondary workspace's `.jj/repo` file holds the path of the main workspace's
        // `.jj/repo` directory, relative to its own `.jj` directory unless absolute.
        let contents = fs::read(&repo)?;
        let path = Path::new(OsStr::from_bytes(&contents));
        jj_dir.join(path)
    };
    store.canonicalize().map(Some)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::slice;

    use tempfile::TempDir;

    use super::*;

    /// Creates the skeleton of a main workspace, whose `.jj/repo` is a directory.
    fn main_workspace(path: &Path) {
        fs::create_dir_all(path.join(".jj/repo/store")).unwrap();
    }

    /// Creates the skeleton of a secondary workspace, whose `.jj/repo` file points at the store.
    fn secondary_workspace(path: &Path, repo_path: &Path) {
        fs::create_dir_all(path.join(".jj")).unwrap();
        fs::write(
            path.join(".jj/repo"),
            repo_path.as_os_str().as_encoded_bytes(),
        )
        .unwrap();
    }

    fn workspaces(discovery: &Discovery) -> Vec<PathBuf> {
        discovery
            .repos
            .iter()
            .map(|r| r.workspace.clone())
            .collect()
    }

    fn tmp() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        // Discovered paths are canonical, and the temporary directory may be behind a symlink.
        let root = dir.path().canonicalize().unwrap();
        (dir, root)
    }

    #[test]
    fn finds_workspaces_within_max_depth() {
        let (_dir, root) = tmp();
        main_workspace(&root.join("a"));
        main_workspace(&root.join("group/b"));
        main_workspace(&root.join("x/y/z/too-deep"));
        fs::create_dir_all(root.join("not-a-repo/empty")).unwrap();

        let discovery = discover(slice::from_ref(&root), 3);

        assert!(discovery.errors.is_empty(), "{:?}", discovery.errors);
        assert_eq!(
            discovery.repos,
            vec![
                Repo {
                    workspace: root.join("a"),
                    store: root.join("a/.jj/repo"),
                },
                Repo {
                    workspace: root.join("group/b"),
                    store: root.join("group/b/.jj/repo"),
                },
            ]
        );
        assert_eq!(discover(slice::from_ref(&root), 1).repos.len(), 1);
        assert_eq!(discover(slice::from_ref(&root), 4).repos.len(), 3);
    }

    #[test]
    fn finds_a_directory_that_is_itself_a_workspace() {
        let (_dir, root) = tmp();
        main_workspace(&root);

        let discovery = discover(slice::from_ref(&root), 0);

        assert_eq!(workspaces(&discovery), vec![root]);
    }

    #[test]
    fn reports_each_repository_once() {
        let (_dir, root) = tmp();
        main_workspace(&root.join("b-main"));
        // Relative to the secondary workspace's `.jj` directory, as jj writes it.
        secondary_workspace(&root.join("c-relative"), Path::new("../../b-main/.jj/repo"));
        secondary_workspace(&root.join("a-absolute"), &root.join("b-main/.jj/repo"));

        let discovery = discover(&[root.clone(), root.join("b-main")], 3);

        assert!(discovery.errors.is_empty(), "{:?}", discovery.errors);
        assert_eq!(
            discovery.repos,
            vec![Repo {
                workspace: root.join("a-absolute"),
                store: root.join("b-main/.jj/repo"),
            }]
        );
    }

    #[test]
    fn does_not_search_inside_workspaces() {
        let (_dir, root) = tmp();
        main_workspace(&root.join("outer"));
        main_workspace(&root.join("outer/vendor/inner"));

        let discovery = discover(slice::from_ref(&root), 5);

        assert_eq!(workspaces(&discovery), vec![root.join("outer")]);
    }

    #[test]
    fn does_not_follow_symlinks() {
        let (_dir, root) = tmp();
        main_workspace(&root.join("elsewhere/repo"));
        fs::create_dir(root.join("search")).unwrap();
        symlink(root.join("elsewhere"), root.join("search/link")).unwrap();

        let discovery = discover(&[root.join("search")], 3);

        assert!(discovery.repos.is_empty(), "{:?}", discovery.repos);
    }

    #[test]
    fn follows_a_symlinked_search_directory() {
        let (_dir, root) = tmp();
        main_workspace(&root.join("real/repo"));
        symlink(root.join("real"), root.join("link")).unwrap();

        let discovery = discover(&[root.join("link")], 1);

        assert_eq!(workspaces(&discovery), vec![root.join("real/repo")]);
    }

    #[test]
    fn reports_unsearchable_paths_and_continues() {
        let (_dir, root) = tmp();
        main_workspace(&root.join("good"));
        secondary_workspace(&root.join("dangling"), &root.join("gone/.jj/repo"));
        let missing = root.join("missing");

        let discovery = discover(&[missing.clone(), root.clone()], 1);

        assert_eq!(workspaces(&discovery), vec![root.join("good")]);
        let error_paths: Vec<_> = discovery.errors.iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(error_paths, vec![missing, root.join("dangling/.jj/repo")]);
    }
}
