//! Recursive directory walking (`walkdir`'s role).

use std::fs::{self, FileType, Metadata};
use std::io;
use std::path::{Path, PathBuf};

/// One file, directory or link met while walking.
#[derive(Debug, Clone)]
pub struct Entry {
    path: PathBuf,
    file_type: FileType,
    depth: usize,
}

impl Entry {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn into_path(self) -> PathBuf {
        self.path
    }

    /// The last path component (the root itself reports the path it was given).
    #[must_use]
    pub fn file_name(&self) -> &std::ffi::OsStr {
        self.path.file_name().unwrap_or(self.path.as_os_str())
    }

    /// What the entry is. A symbolic link reports as a link unless the walk follows links.
    #[must_use]
    pub const fn file_type(&self) -> FileType {
        self.file_type
    }

    /// 0 for the root, 1 for its children, and so on.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// # Errors
    ///
    /// When the entry vanished or cannot be read.
    pub fn metadata(&self) -> io::Result<Metadata> {
        fs::symlink_metadata(&self.path)
    }
}

type Filter = Box<dyn FnMut(&Entry) -> bool>;

/// A recursive walk, configured and then iterated: directories come before their contents.
pub struct Walk {
    root: PathBuf,
    follow_links: bool,
    sort: bool,
    max_depth: usize,
    filter: Option<Filter>,
}

impl std::fmt::Debug for Walk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Walk")
            .field("root", &self.root)
            .field("follow_links", &self.follow_links)
            .field("sort", &self.sort)
            .field("max_depth", &self.max_depth)
            .finish_non_exhaustive()
    }
}

impl Walk {
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            follow_links: false,
            sort: false,
            max_depth: usize::MAX,
            filter: None,
        }
    }

    /// Descend into symbolic links to directories (a link that loops back to an ancestor is an error entry).
    #[must_use]
    pub const fn follow_links(mut self, follow: bool) -> Self {
        self.follow_links = follow;
        self
    }

    /// Visit each directory's children in file-name order instead of the OS's.
    #[must_use]
    pub const fn sort_by_file_name(mut self) -> Self {
        self.sort = true;
        self
    }

    /// Do not descend below this depth (0 is the root alone).
    #[must_use]
    pub const fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Skips what a search tool such as `grep -r` skips by default: hidden files and directories, and
    /// whatever the `.gitignore`, `.ignore`, `.git/info/exclude` and global git ignore files exclude (see
    /// [`ignore`](crate::ignore)). The root itself is never skipped.
    #[must_use]
    pub fn standard_filters(mut self) -> Self {
        let mut ignorer = crate::ignore::Ignorer::new(&self.root, true, true);
        let mut previous = self.filter.take();
        self.filter = Some(Box::new(move |entry| {
            !ignorer.is_ignored(entry.path(), entry.file_type().is_dir())
                && previous.as_mut().is_none_or(|keep| keep(entry))
        }));
        self
    }

    /// Entries for which `keep` is false are skipped, and a skipped directory is not entered.
    #[must_use]
    pub fn filter_entry(mut self, keep: impl FnMut(&Entry) -> bool + 'static) -> Self {
        self.filter = Some(Box::new(keep));
        self
    }
}

impl IntoIterator for Walk {
    type Item = io::Result<Entry>;
    type IntoIter = WalkIter;

    fn into_iter(self) -> WalkIter {
        WalkIter {
            walk: self,
            started: false,
            stack: Vec::new(),
            ancestors: Vec::new(),
            deferred: None,
        }
    }
}

/// The iterator behind [`Walk`].
#[derive(Debug)]
pub struct WalkIter {
    walk: Walk,
    started: bool,
    /// Unvisited children of each open directory, last to be popped first.
    stack: Vec<Vec<PathBuf>>,
    /// Canonical paths of the open directories, to catch link loops.
    ancestors: Vec<PathBuf>,
    /// A directory that could not be opened: reported right after its own entry.
    deferred: Option<io::Error>,
}

impl WalkIter {
    fn entry(&self, path: PathBuf, depth: usize) -> io::Result<Entry> {
        let mut file_type = fs::symlink_metadata(&path)?.file_type();
        if self.walk.follow_links && file_type.is_symlink() {
            file_type = fs::metadata(&path)?.file_type();
        }
        Ok(Entry {
            path,
            file_type,
            depth,
        })
    }

    /// Opens `entry` as the next level, if it is a directory that may be entered.
    fn descend(&mut self, entry: &Entry) -> io::Result<()> {
        if !entry.file_type.is_dir() || entry.depth >= self.walk.max_depth {
            return Ok(());
        }
        if self.walk.follow_links {
            let canonical = fs::canonicalize(&entry.path)?;
            if self.ancestors.contains(&canonical) {
                return Err(io::Error::other(format!(
                    "file system loop found: {} points to an ancestor",
                    entry.path.display()
                )));
            }
            self.ancestors.push(canonical);
        } else {
            self.ancestors.push(PathBuf::new());
        }
        let mut children = Vec::new();
        let listing = fs::read_dir(&entry.path).and_then(|dir| {
            for child in dir {
                children.push(child?.path());
            }
            Ok(())
        });
        if let Err(error) = listing {
            self.ancestors.pop();
            return Err(error);
        }
        if self.walk.sort {
            children.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        } else {
            children.reverse();
        }
        self.stack.push(children);
        Ok(())
    }
}

impl Iterator for WalkIter {
    type Item = io::Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(error) = self.deferred.take() {
            return Some(Err(error));
        }
        loop {
            let (path, depth) = if self.started {
                let children = self.stack.last_mut()?;
                if let Some(path) = children.pop() {
                    (path, self.stack.len())
                } else {
                    self.stack.pop();
                    self.ancestors.pop();
                    continue;
                }
            } else {
                self.started = true;
                (self.walk.root.clone(), 0)
            };
            let entry = match self.entry(path, depth) {
                Ok(entry) => entry,
                Err(error) => return Some(Err(error)),
            };
            if let Some(filter) = self.walk.filter.as_mut()
                && !filter(&entry)
            {
                continue;
            }
            self.deferred = self.descend(&entry).err();
            return Some(Ok(entry));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    fn tree() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join("b/inner")).unwrap();
        fs::create_dir(p.join("a")).unwrap();
        fs::write(p.join("a/one.txt"), "1").unwrap();
        fs::write(p.join("b/two.txt"), "2").unwrap();
        fs::write(p.join("b/inner/three.txt"), "3").unwrap();
        fs::write(p.join("root.txt"), "r").unwrap();
        dir
    }

    fn names(walk: Walk, root: &Path) -> Vec<String> {
        walk.into_iter()
            .map(|entry| {
                let entry = entry.unwrap();
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn walks_directories_before_their_contents_in_name_order() {
        let dir = tree();
        let got = names(Walk::new(dir.path()).sort_by_file_name(), dir.path());
        assert_eq!(
            got,
            [
                "",
                "a",
                "a/one.txt",
                "b",
                "b/inner",
                "b/inner/three.txt",
                "b/two.txt",
                "root.txt"
            ]
        );
    }

    #[test]
    fn entries_know_their_depth_name_and_type() {
        let dir = tree();
        let entries: Vec<Entry> = Walk::new(dir.path())
            .sort_by_file_name()
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(entries[0].depth(), 0);
        assert!(entries[0].file_type().is_dir());
        let three = entries
            .iter()
            .find(|e| e.file_name() == "three.txt")
            .unwrap();
        assert_eq!(three.depth(), 3);
        assert!(three.file_type().is_file());
        assert_eq!(three.metadata().unwrap().len(), 1);
        assert_eq!(
            three.clone().into_path(),
            dir.path().join("b/inner/three.txt")
        );
    }

    #[test]
    fn max_depth_limits_the_descent() {
        let dir = tree();
        assert_eq!(names(Walk::new(dir.path()).max_depth(0), dir.path()), [""]);
        let one = names(
            Walk::new(dir.path()).max_depth(1).sort_by_file_name(),
            dir.path(),
        );
        assert_eq!(one, ["", "a", "b", "root.txt"]);
    }

    #[test]
    fn filter_entry_prunes_whole_directories() {
        let dir = tree();
        let got = names(
            Walk::new(dir.path())
                .sort_by_file_name()
                .filter_entry(|entry| entry.file_name() != "b"),
            dir.path(),
        );
        assert_eq!(got, ["", "a", "a/one.txt", "root.txt"]);
    }

    #[test]
    fn unsorted_walks_still_visit_everything() {
        let dir = tree();
        let mut got = names(Walk::new(dir.path()), dir.path());
        got.sort();
        assert_eq!(got.len(), 8);
        assert!(format!("{:?}", Walk::new(dir.path())).contains("Walk"));
    }

    #[test]
    fn a_missing_root_is_an_error_entry() {
        let dir = TempDir::new().unwrap();
        let mut walk = Walk::new(dir.path().join("missing")).into_iter();
        assert!(walk.next().unwrap().is_err());
        assert!(walk.next().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_is_an_entry_followed_by_an_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tree();
        let locked = dir.path().join("a");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let results: Vec<_> = Walk::new(dir.path())
            .sort_by_file_name()
            .into_iter()
            .collect();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        // Running as root reads it anyway; otherwise the error follows the entry for `a`.
        let position = results
            .iter()
            .position(|r| r.as_ref().is_ok_and(|e| e.file_name() == "a"))
            .unwrap();
        if results.iter().any(Result::is_err) {
            assert!(results[position + 1].is_err());
        }
        assert!(
            results
                .iter()
                .any(|r| r.as_ref().is_ok_and(|e| e.file_name() == "root.txt"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_are_reported_as_links_unless_followed() {
        use std::os::unix::fs::symlink;
        let dir = tree();
        symlink(dir.path().join("a"), dir.path().join("link")).unwrap();

        let plain = Walk::new(dir.path()).sort_by_file_name();
        let link = plain
            .into_iter()
            .map(Result::unwrap)
            .find(|e| e.file_name() == "link")
            .unwrap();
        assert!(link.file_type().is_symlink());

        let followed = names(
            Walk::new(dir.path()).follow_links(true).sort_by_file_name(),
            dir.path(),
        );
        assert!(followed.contains(&"link/one.txt".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_loop_is_an_error_not_an_endless_walk() {
        use std::os::unix::fs::symlink;
        let dir = tree();
        symlink(dir.path(), dir.path().join("a/up")).unwrap();
        let results: Vec<_> = Walk::new(dir.path())
            .follow_links(true)
            .into_iter()
            .collect();
        assert!(results.iter().any(Result::is_err));
        assert!(results.len() < 100);
    }

    #[test]
    fn standard_filters_skip_hidden_and_ignored_files() {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join(".git/info")).unwrap();
        fs::create_dir_all(p.join("src/gen")).unwrap();
        fs::create_dir_all(p.join("target/debug")).unwrap();
        fs::create_dir_all(p.join(".hidden")).unwrap();
        fs::write(
            p.join(".gitignore"),
            "target/
*.tmp
!keep.tmp
",
        )
        .unwrap();
        fs::write(p.join(".git/info/exclude"), "secret.txt\n").unwrap();
        fs::write(p.join("src/.ignore"), "gen/\n").unwrap();
        for file in [
            "a.rs",
            "x.tmp",
            "keep.tmp",
            "secret.txt",
            ".dot",
            "src/b.rs",
            "src/gen/g.rs",
            "target/debug/t",
            ".hidden/h",
        ] {
            fs::write(p.join(file), "x").unwrap();
        }
        let got = names(Walk::new(p).sort_by_file_name().standard_filters(), p);
        assert_eq!(got, ["", "a.rs", "keep.tmp", "src", "src/b.rs"]);
        // A walk starting below the repository root still honours the parents' files.
        let got = names(
            Walk::new(p.join("src"))
                .sort_by_file_name()
                .standard_filters(),
            &p.join("src"),
        );
        assert_eq!(got, ["", "b.rs"]);
        // Composed with a filter of its own.
        let got = names(
            Walk::new(p)
                .sort_by_file_name()
                .filter_entry(|e| e.depth() == 0 || e.file_name() != "a.rs")
                .standard_filters(),
            p,
        );
        assert!(!got.contains(&"a.rs".to_string()) && got.contains(&"keep.tmp".to_string()));
        // Outside a repository `.gitignore` is not honoured, `.ignore` is.
        let plain = TempDir::new().unwrap();
        fs::write(plain.path().join(".gitignore"), "a.txt\n").unwrap();
        fs::write(plain.path().join(".ignore"), "b.txt\n").unwrap();
        for file in ["a.txt", "b.txt", "c.txt"] {
            fs::write(plain.path().join(file), "x").unwrap();
        }
        let got = names(
            Walk::new(plain.path())
                .sort_by_file_name()
                .standard_filters(),
            plain.path(),
        );
        assert!(
            got.contains(&"a.txt".to_string())
                && !got.contains(&"b.txt".to_string())
                && got.contains(&"c.txt".to_string())
        );
    }
}
