//! Safe Git and detached-worktree operations for BA Agent.
//!
//! The crate never invokes a shell. Every path is validated as repository-relative
//! before it is passed to Git, and operations that mutate remotes or remove data are
//! classified for an external approval queue rather than executed here.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Operations that may be requested by a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperation {
    LocalBranch,
    LocalCommit,
    Push,
    Merge,
    Deploy,
    Delete,
    ElevatePermissions,
}

impl GitOperation {
    /// Whether the operation must be resolved by the human approval queue.
    #[must_use]
    pub const fn requires_approval(self) -> bool {
        matches!(
            self,
            Self::Push | Self::Merge | Self::Deploy | Self::Delete | Self::ElevatePermissions
        )
    }
}

/// A normalized, repository-relative path used in task write declarations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepoPath(PathBuf);

impl RepoPath {
    /// Validate and normalize a path without touching the filesystem.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::UnsafePath`] for absolute, escaping, empty, or
    /// repository-metadata paths.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err(GitError::UnsafePath(path.to_path_buf()));
        }

        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(segment) => normalized.push(segment),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(GitError::UnsafePath(path.to_path_buf()));
                }
            }
        }
        if normalized.as_os_str().is_empty()
            || normalized.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(".git")
            })
        {
            return Err(GitError::UnsafePath(path.to_path_buf()));
        }
        Ok(Self(normalized))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Returns true when `candidate` is inside this declared path.
    #[must_use]
    pub fn contains(&self, candidate: &RepoPath) -> bool {
        candidate.0.starts_with(&self.0)
    }
}

/// True when two normalized repository paths are equal or one is an ancestor.
#[must_use]
pub fn paths_overlap(left: &RepoPath, right: &RepoPath) -> bool {
    left.contains(right) || right.contains(left)
}

/// A non-empty set of paths a task is allowed to write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteScope {
    paths: BTreeSet<RepoPath>,
}

impl WriteScope {
    /// Build a non-empty normalized write scope.
    ///
    /// # Errors
    ///
    /// Returns [`GitError::EmptyWriteScope`] when no paths are supplied.
    pub fn new(paths: impl IntoIterator<Item = RepoPath>) -> Result<Self, GitError> {
        let paths = paths.into_iter().collect::<BTreeSet<_>>();
        if paths.is_empty() {
            return Err(GitError::EmptyWriteScope);
        }
        Ok(Self { paths })
    }

    #[must_use]
    pub fn paths(&self) -> &BTreeSet<RepoPath> {
        &self.paths
    }

    #[must_use]
    pub fn contains(&self, candidate: &RepoPath) -> bool {
        self.paths
            .iter()
            .any(|declared| declared.contains(candidate))
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.paths
            .iter()
            .any(|left| other.paths.iter().any(|right| paths_overlap(left, right)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeHandle {
    pub task_id: String,
    pub path: PathBuf,
    pub base_revision: String,
    pub write_scope: WriteScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalizedWorktree {
    pub branch: String,
    pub commit: String,
    pub changed_paths: Vec<RepoPath>,
}

/// Repository and global worktree-root pair with validated canonical roots.
#[derive(Debug, Clone)]
pub struct WorktreeManager {
    repository: PathBuf,
    worktree_root: PathBuf,
}

impl WorktreeManager {
    /// Open a repository root and a separate worktree storage root.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot inspect the repository, the requested
    /// repository is not its top level, or the worktree root is inside it.
    pub fn open(
        repository: impl AsRef<Path>,
        worktree_root: impl AsRef<Path>,
    ) -> Result<Self, GitError> {
        let repository = repository.as_ref().canonicalize()?;
        let root_output = run_git(&repository, ["rev-parse", "--show-toplevel"])?;
        let actual_root = output_text(&root_output)?.trim().to_owned();
        let actual_root = PathBuf::from(actual_root).canonicalize()?;
        if repository != actual_root {
            return Err(GitError::RepositoryRootMismatch {
                requested: repository,
                actual: actual_root,
            });
        }

        let unresolved_root = resolve_without_creating(worktree_root.as_ref())?;
        if unresolved_root == repository || unresolved_root.starts_with(&repository) {
            return Err(GitError::UnsafeWorktreeRoot(unresolved_root));
        }
        std::fs::create_dir_all(&unresolved_root)?;
        let worktree_root = unresolved_root.canonicalize()?;
        if worktree_root == repository || worktree_root.starts_with(&repository) {
            return Err(GitError::UnsafeWorktreeRoot(worktree_root));
        }
        Ok(Self {
            repository,
            worktree_root,
        })
    }

    #[must_use]
    pub fn repository(&self) -> &Path {
        &self.repository
    }

    /// Create a detached worktree for a write task.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe task identifiers, existing destinations, or
    /// failed Git worktree creation.
    pub fn create_detached(
        &self,
        task_id: &str,
        write_scope: WriteScope,
    ) -> Result<WorktreeHandle, GitError> {
        let task_segment = safe_ref_segment(task_id)?;
        let path = self.worktree_root.join(task_segment);
        if path.exists() {
            return Err(GitError::WorktreeExists(path));
        }
        ensure_direct_child(&self.worktree_root, &path)?;

        let revision_output = run_git(&self.repository, ["rev-parse", "HEAD"])?;
        let base_revision = output_text(&revision_output)?.trim().to_owned();
        let args = vec![
            "worktree".to_owned(),
            "add".to_owned(),
            "--detach".to_owned(),
            git_argument_path(&path),
            base_revision.clone(),
        ];
        run_git_owned(&self.repository, &args)?;
        let canonical_path = path.canonicalize()?;
        if !canonical_path.starts_with(&self.worktree_root) {
            let _ignored = run_git_owned(
                &self.repository,
                &[
                    "worktree".into(),
                    "remove".into(),
                    "--force".into(),
                    git_argument_path(&canonical_path),
                ],
            );
            return Err(GitError::UnsafePath(canonical_path));
        }

        Ok(WorktreeHandle {
            task_id: task_id.to_owned(),
            path: canonical_path,
            base_revision,
            write_scope,
        })
    }

    /// Verify changed paths, create the task branch, and commit only after gates pass.
    ///
    /// # Errors
    ///
    /// Returns an error when gates have not passed, the handle is stale or foreign,
    /// a changed path escapes the declared scope, or Git cannot branch/commit.
    pub fn finalize(
        &self,
        handle: &WorktreeHandle,
        workstream: &str,
        commit_message: &str,
        gates_passed: bool,
    ) -> Result<FinalizedWorktree, GitError> {
        if !gates_passed {
            return Err(GitError::QualityGateRequired);
        }
        self.verify_handle_at_base(handle)?;

        let detected_paths = changed_paths(&handle.path)?;
        if detected_paths.is_empty() {
            return Err(GitError::NoChanges);
        }
        if let Some(path) = detected_paths
            .iter()
            .find(|path| !handle.write_scope.contains(path))
        {
            return Err(GitError::WriteScopeViolation(path.as_path().to_path_buf()));
        }

        let branch = format!(
            "ba/{}/{}",
            safe_ref_segment(workstream)?,
            safe_ref_segment(&handle.task_id)?
        );
        run_git_owned(
            &handle.path,
            &["switch".into(), "-c".into(), branch.clone()],
        )?;
        run_git(&handle.path, ["add", "--all", "--", "."])?;
        let staged_paths = changed_paths(&handle.path)?;
        if let Some(path) = staged_paths
            .iter()
            .find(|path| !handle.write_scope.contains(path))
        {
            return Err(GitError::WriteScopeViolation(path.as_path().to_path_buf()));
        }
        run_git_owned(
            &handle.path,
            &["commit".into(), "-m".into(), commit_message.to_owned()],
        )?;
        let commit_output = run_git(&handle.path, ["rev-parse", "HEAD"])?;
        let commit = output_text(&commit_output)?.trim().to_owned();
        Ok(FinalizedWorktree {
            branch,
            commit,
            changed_paths: detected_paths,
        })
    }

    /// Remove a clean worktree after its changes have been committed elsewhere.
    ///
    /// Dirty worktrees are retained. Destructive/forced removal is deliberately not
    /// exposed here and must be handled by an approval-gated outer layer.
    ///
    /// # Errors
    ///
    /// Returns an error when the handle is foreign, the worktree is dirty, or Git
    /// cannot remove it safely.
    pub fn remove_clean(&self, handle: &WorktreeHandle) -> Result<(), GitError> {
        self.verify_ownership(handle)?;
        if !changed_paths(&handle.path)?.is_empty() {
            return Err(GitError::DirtyWorktree(handle.path.clone()));
        }
        run_git_owned(
            &self.repository,
            &[
                "worktree".into(),
                "remove".into(),
                git_argument_path(&handle.path),
            ],
        )?;
        Ok(())
    }

    fn verify_handle_at_base(&self, handle: &WorktreeHandle) -> Result<(), GitError> {
        self.verify_ownership(handle)?;
        let head_output = run_git(&handle.path, ["rev-parse", "HEAD"])?;
        let head = output_text(&head_output)?;
        if head.trim() != handle.base_revision {
            return Err(GitError::BaseRevisionChanged {
                expected: handle.base_revision.clone(),
                actual: head.trim().to_owned(),
            });
        }
        Ok(())
    }

    fn verify_ownership(&self, handle: &WorktreeHandle) -> Result<(), GitError> {
        let canonical = handle.path.canonicalize()?;
        ensure_direct_child(&self.worktree_root, &canonical)?;
        if canonical != handle.path {
            return Err(GitError::UnsafePath(handle.path.clone()));
        }

        let root_output = run_git(&canonical, ["rev-parse", "--show-toplevel"])?;
        let root = output_text(&root_output)?;
        if PathBuf::from(root.trim()).canonicalize()? != canonical {
            return Err(GitError::ForeignWorktree(canonical));
        }
        let common_dir_output = run_git(&canonical, ["rev-parse", "--git-common-dir"])?;
        let common_dir = output_text(&common_dir_output)?;
        let common_dir = absolute_from(&canonical, Path::new(common_dir.trim())).canonicalize()?;
        let repository_git_dir_output =
            run_git(&self.repository, ["rev-parse", "--git-common-dir"])?;
        let repository_git_dir = output_text(&repository_git_dir_output)?;
        let repository_git_dir =
            absolute_from(&self.repository, Path::new(repository_git_dir.trim())).canonicalize()?;
        if common_dir != repository_git_dir {
            return Err(GitError::ForeignWorktree(canonical));
        }
        Ok(())
    }
}

fn absolute_from(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn git_argument_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(network) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{network}");
        }
        if let Some(local) = value.strip_prefix(r"\\?\") {
            return local.to_owned();
        }
    }
    value.into_owned()
}

fn resolve_without_creating(path: &Path) -> Result<PathBuf, GitError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut cursor = absolute.as_path();
    let mut missing = Vec::new();
    while !cursor.exists() {
        let segment = cursor
            .file_name()
            .ok_or_else(|| GitError::UnsafeWorktreeRoot(absolute.clone()))?;
        if segment == OsStr::new(".") || segment == OsStr::new("..") {
            return Err(GitError::UnsafeWorktreeRoot(absolute));
        }
        missing.push(segment.to_os_string());
        cursor = cursor
            .parent()
            .ok_or_else(|| GitError::UnsafeWorktreeRoot(absolute.clone()))?;
    }
    let mut resolved = cursor.canonicalize()?;
    for segment in missing.into_iter().rev() {
        resolved.push(segment);
    }
    Ok(resolved)
}

fn ensure_direct_child(root: &Path, candidate: &Path) -> Result<(), GitError> {
    let parent = candidate
        .parent()
        .ok_or_else(|| GitError::UnsafePath(candidate.to_path_buf()))?;
    if parent != root || candidate.file_name().is_none() {
        return Err(GitError::UnsafePath(candidate.to_path_buf()));
    }
    Ok(())
}

fn safe_ref_segment(value: &str) -> Result<String, GitError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.starts_with('.')
        || trimmed.ends_with('.')
        || trimmed.to_ascii_lowercase().ends_with(".lock")
        || trimmed.contains("..")
        || trimmed.contains("@{")
        || trimmed.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '/'
                )
        })
    {
        return Err(GitError::UnsafeRefSegment(value.to_owned()));
    }
    Ok(trimmed.to_owned())
}

fn changed_paths(worktree: &Path) -> Result<Vec<RepoPath>, GitError> {
    let output = run_git(
        worktree,
        ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut result = BTreeSet::new();
    let mut entries = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            return Err(GitError::InvalidGitOutput("short porcelain record".into()));
        }
        let status = &entry[..2];
        let path = bytes_to_repo_path(&entry[3..])?;
        result.insert(path);
        if matches!(status[0], b'R' | b'C') || matches!(status[1], b'R' | b'C') {
            let original = entries
                .next()
                .ok_or_else(|| GitError::InvalidGitOutput("rename source missing".into()))?;
            result.insert(bytes_to_repo_path(original)?);
        }
    }
    Ok(result.into_iter().collect())
}

fn bytes_to_repo_path(bytes: &[u8]) -> Result<RepoPath, GitError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|error| GitError::InvalidGitOutput(error.to_string()))?;
    RepoPath::new(value)
}

fn run_git<'a>(cwd: &Path, args: impl IntoIterator<Item = &'a str>) -> Result<Output, GitError> {
    let args = args.into_iter().map(ToOwned::to_owned).collect::<Vec<_>>();
    run_git_owned(cwd, &args)
}

fn run_git_owned(cwd: &Path, args: &[String]) -> Result<Output, GitError> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(GitError::CommandFailed {
            args: args.to_vec(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

fn output_text(output: &Output) -> Result<&str, GitError> {
    std::str::from_utf8(&output.stdout)
        .map_err(|error| GitError::InvalidGitOutput(error.to_string()))
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("unsafe repository-relative path: {0}")]
    UnsafePath(PathBuf),
    #[error("write scope must contain at least one path")]
    EmptyWriteScope,
    #[error("worktree root must be outside the repository: {0}")]
    UnsafeWorktreeRoot(PathBuf),
    #[error("requested path {requested:?} is not the repository root {actual:?}")]
    RepositoryRootMismatch { requested: PathBuf, actual: PathBuf },
    #[error("worktree already exists: {0}")]
    WorktreeExists(PathBuf),
    #[error("path is not a worktree owned by this repository: {0}")]
    ForeignWorktree(PathBuf),
    #[error("worktree HEAD changed from {expected} to {actual}")]
    BaseRevisionChanged { expected: String, actual: String },
    #[error("worktree has uncommitted changes and was retained: {0}")]
    DirtyWorktree(PathBuf),
    #[error("unsafe Git ref segment: {0}")]
    UnsafeRefSegment(String),
    #[error("quality gates must pass before finalizing a worktree")]
    QualityGateRequired,
    #[error("worktree has no changes to commit")]
    NoChanges,
    #[error("changed path is outside the declared write scope: {0}")]
    WriteScopeViolation(PathBuf),
    #[error("Git output was invalid: {0}")]
    InvalidGitOutput(String),
    #[error("git {args:?} failed: {stderr}")]
    CommandFailed { args: Vec<String>, stderr: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(cwd: &Path, args: &[&str]) -> Result<(), GitError> {
        run_git(cwd, args.iter().copied()).map(|_| ())
    }

    #[test]
    fn path_scope_rejects_escape_and_detects_overlap() -> Result<(), Box<dyn std::error::Error>> {
        assert!(RepoPath::new("../secrets").is_err());
        assert!(RepoPath::new(".git/config").is_err());
        assert!(RepoPath::new("src/.GIT/config").is_err());
        let source = RepoPath::new("src")?;
        let nested = RepoPath::new("src/server/main.rs")?;
        let docs = RepoPath::new("docs")?;
        assert!(paths_overlap(&source, &nested));
        assert!(!paths_overlap(&source, &docs));
        Ok(())
    }

    #[test]
    fn detached_worktree_finalizes_to_local_branch() -> Result<(), Box<dyn std::error::Error>> {
        let repository_dir = tempfile::tempdir()?;
        let worktrees_dir = tempfile::tempdir()?;
        git(repository_dir.path(), &["init", "-b", "main"])?;
        git(
            repository_dir.path(),
            &["config", "user.email", "test@example.invalid"],
        )?;
        git(
            repository_dir.path(),
            &["config", "user.name", "BA Agent Test"],
        )?;
        std::fs::create_dir(repository_dir.path().join("src"))?;
        std::fs::write(repository_dir.path().join("src/base.txt"), "base\n")?;
        git(repository_dir.path(), &["add", "."])?;
        git(repository_dir.path(), &["commit", "-m", "base"])?;

        let manager = WorktreeManager::open(repository_dir.path(), worktrees_dir.path())?;
        let scope = WriteScope::new([RepoPath::new("src")?])?;
        let handle = manager.create_detached("01TESTTASK", scope)?;
        std::fs::write(handle.path.join("src/change.txt"), "change\n")?;
        let finalized = manager.finalize(&handle, "alpha", "feat: task change", true)?;

        assert_eq!(finalized.branch, "ba/alpha/01TESTTASK");
        assert_eq!(
            finalized.changed_paths,
            vec![RepoPath::new("src/change.txt")?]
        );
        let branch = output_text(&run_git(&handle.path, ["branch", "--show-current"])?)?
            .trim()
            .to_owned();
        assert_eq!(branch, finalized.branch);
        manager.remove_clean(&handle)?;
        assert!(!handle.path.exists());
        Ok(())
    }

    #[test]
    fn opening_rejects_uncreated_root_inside_repository_without_creating_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let repository_dir = tempfile::tempdir()?;
        git(repository_dir.path(), &["init", "-b", "main"])?;
        let unsafe_root = repository_dir.path().join("state/worktrees");
        assert!(matches!(
            WorktreeManager::open(repository_dir.path(), &unsafe_root),
            Err(GitError::UnsafeWorktreeRoot(_))
        ));
        assert!(!unsafe_root.exists());
        Ok(())
    }

    #[test]
    fn finalization_rejects_forged_or_stale_worktree_handles()
    -> Result<(), Box<dyn std::error::Error>> {
        let repository_dir = tempfile::tempdir()?;
        let worktrees_dir = tempfile::tempdir()?;
        git(repository_dir.path(), &["init", "-b", "main"])?;
        git(
            repository_dir.path(),
            &["config", "user.email", "test@example.invalid"],
        )?;
        git(
            repository_dir.path(),
            &["config", "user.name", "BA Agent Test"],
        )?;
        std::fs::write(repository_dir.path().join("base.txt"), "base\n")?;
        git(repository_dir.path(), &["add", "."])?;
        git(repository_dir.path(), &["commit", "-m", "base"])?;
        let manager = WorktreeManager::open(repository_dir.path(), worktrees_dir.path())?;

        let foreign = worktrees_dir.path().join("foreign");
        std::fs::create_dir(&foreign)?;
        git(&foreign, &["init", "-b", "main"])?;
        git(&foreign, &["config", "user.email", "test@example.invalid"])?;
        git(&foreign, &["config", "user.name", "BA Agent Test"])?;
        std::fs::write(foreign.join("file.txt"), "foreign\n")?;
        git(&foreign, &["add", "."])?;
        git(&foreign, &["commit", "-m", "foreign"])?;
        let foreign_head = output_text(&run_git(&foreign, ["rev-parse", "HEAD"])?)?
            .trim()
            .to_owned();
        let forged = WorktreeHandle {
            task_id: "FORGED".to_owned(),
            path: foreign.canonicalize()?,
            base_revision: foreign_head,
            write_scope: WriteScope::new([RepoPath::new("file.txt")?])?,
        };
        std::fs::write(foreign.join("file.txt"), "changed\n")?;
        assert!(matches!(
            manager.finalize(&forged, "alpha", "forged", true),
            Err(GitError::ForeignWorktree(_))
        ));

        let stale =
            manager.create_detached("STALE", WriteScope::new([RepoPath::new("base.txt")?])?)?;
        std::fs::write(stale.path.join("base.txt"), "manual commit\n")?;
        git(&stale.path, &["add", "."])?;
        git(&stale.path, &["commit", "-m", "manual"])?;
        assert!(matches!(
            manager.finalize(&stale, "alpha", "stale", true),
            Err(GitError::BaseRevisionChanged { .. })
        ));
        Ok(())
    }

    #[test]
    fn finalization_refuses_scope_violation_and_unpassed_gate()
    -> Result<(), Box<dyn std::error::Error>> {
        let repository_dir = tempfile::tempdir()?;
        let worktrees_dir = tempfile::tempdir()?;
        git(repository_dir.path(), &["init", "-b", "main"])?;
        git(
            repository_dir.path(),
            &["config", "user.email", "test@example.invalid"],
        )?;
        git(
            repository_dir.path(),
            &["config", "user.name", "BA Agent Test"],
        )?;
        std::fs::write(repository_dir.path().join("README.md"), "base\n")?;
        git(repository_dir.path(), &["add", "."])?;
        git(repository_dir.path(), &["commit", "-m", "base"])?;
        let manager = WorktreeManager::open(repository_dir.path(), worktrees_dir.path())?;
        let handle =
            manager.create_detached("01SCOPE", WriteScope::new([RepoPath::new("src")?])?)?;
        std::fs::write(handle.path.join("README.md"), "outside\n")?;
        assert!(matches!(
            manager.finalize(&handle, "alpha", "change", false),
            Err(GitError::QualityGateRequired)
        ));
        assert!(matches!(
            manager.finalize(&handle, "alpha", "change", true),
            Err(GitError::WriteScopeViolation(_))
        ));
        Ok(())
    }

    #[test]
    fn remote_and_destructive_operations_require_approval() {
        assert!(!GitOperation::LocalCommit.requires_approval());
        assert!(GitOperation::Push.requires_approval());
        assert!(GitOperation::Merge.requires_approval());
        assert!(GitOperation::Deploy.requires_approval());
        assert!(GitOperation::Delete.requires_approval());
        assert!(GitOperation::ElevatePermissions.requires_approval());
        assert!(safe_ref_segment("feature..escape").is_err());
        assert!(safe_ref_segment(".hidden").is_err());
        assert!(safe_ref_segment("branch.LOCK").is_err());
    }
}
