use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use ba_core::{EntityId, Project, now};
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};

use crate::SchedulerError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOnboardingMode {
    Import,
    Create,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectOnboardingRequest {
    pub mode: ProjectOnboardingMode,
    pub path: PathBuf,
    pub name: Option<String>,
    pub description: String,
    pub language: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOnboardingResult {
    pub project: Project,
    pub mode: ProjectOnboardingMode,
    pub created_directory: bool,
    pub initialized_git: bool,
    pub created_project_config: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectOnboardingService {
    store: SqliteStore,
}

impl ProjectOnboardingService {
    #[must_use]
    pub const fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Import an existing Git root or create and register a new local repository.
    ///
    /// Existing user configuration is preserved. Create mode only accepts a missing
    /// path or an empty directory and cleans up artifacts it created if registration
    /// fails.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe/invalid paths, non-root Git paths, duplicate
    /// registration, non-empty create targets, Git failures, or persistence failures.
    pub fn onboard(
        &self,
        request: ProjectOnboardingRequest,
    ) -> Result<ProjectOnboardingResult, SchedulerError> {
        validate_language(&request.language)?;
        let supplied_name = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        if request.name.is_some() && supplied_name.is_none() {
            return Err(SchedulerError::InvalidProjectOnboarding(
                "project name must not be empty".into(),
            ));
        }

        let prepared = match request.mode {
            ProjectOnboardingMode::Import => PreparedRoot::import(&request.path)?,
            ProjectOnboardingMode::Create => PreparedRoot::create(&request.path)?,
        };
        let root = prepared.root.clone();
        if let Err(error) = self.reject_duplicate(&root) {
            prepared.rollback();
            return Err(error);
        }

        let initialized_git = if request.mode == ProjectOnboardingMode::Create {
            if let Err(error) = initialize_git(&root) {
                prepared.rollback();
                return Err(error);
            }
            true
        } else {
            ensure_exact_git_root(&root)?;
            false
        };

        let name = supplied_name.map_or_else(
            || {
                root.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("project")
                    .to_owned()
            },
            str::to_owned,
        );
        let created_project_config =
            match initialize_project_layout(&root, &name, &request.language) {
                Ok(created) => created,
                Err(error) => {
                    prepared.rollback();
                    return Err(error);
                }
            };
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name,
            root_path: root.to_string_lossy().into_owned(),
            description: request.description,
            language: request.language,
            created_at: stamp,
            updated_at: stamp,
        };
        if let Err(error) = self.store.upsert_project(&project) {
            prepared.rollback();
            return Err(error.into());
        }
        Ok(ProjectOnboardingResult {
            project,
            mode: request.mode,
            created_directory: prepared.created_directory,
            initialized_git,
            created_project_config,
        })
    }

    fn reject_duplicate(&self, root: &Path) -> Result<(), SchedulerError> {
        for project in self.store.list_projects()? {
            let existing = PathBuf::from(project.root_path);
            let normalized = existing.canonicalize().unwrap_or(existing);
            if normalized == root {
                return Err(SchedulerError::ProjectAlreadyRegistered(root.to_path_buf()));
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct PreparedRoot {
    root: PathBuf,
    created_directory: bool,
    cleanup_create_artifacts: bool,
}

impl PreparedRoot {
    fn import(path: &Path) -> Result<Self, SchedulerError> {
        let root = path.canonicalize().map_err(|error| {
            SchedulerError::InvalidProjectOnboarding(format!(
                "project root does not exist or is inaccessible: {error}"
            ))
        })?;
        if !root.is_dir() {
            return Err(SchedulerError::InvalidProjectOnboarding(
                "project root must be a directory".into(),
            ));
        }
        Ok(Self {
            root,
            created_directory: false,
            cleanup_create_artifacts: false,
        })
    }

    fn create(path: &Path) -> Result<Self, SchedulerError> {
        if path.exists() {
            let root = path.canonicalize()?;
            if !root.is_dir() {
                return Err(SchedulerError::InvalidProjectOnboarding(
                    "new project path must be a directory".into(),
                ));
            }
            if fs::read_dir(&root)?.next().transpose()?.is_some() {
                return Err(SchedulerError::ProjectDirectoryNotEmpty(root));
            }
            return Ok(Self {
                root,
                created_directory: false,
                cleanup_create_artifacts: true,
            });
        }
        let name = path.file_name().ok_or_else(|| {
            SchedulerError::InvalidProjectOnboarding(
                "new project path must name a child directory".into(),
            )
        })?;
        let parent = path.parent().ok_or_else(|| {
            SchedulerError::InvalidProjectOnboarding(
                "new project path must have an existing parent".into(),
            )
        })?;
        let parent = parent.canonicalize().map_err(|error| {
            SchedulerError::InvalidProjectOnboarding(format!(
                "new project parent does not exist or is inaccessible: {error}"
            ))
        })?;
        if !parent.is_dir() {
            return Err(SchedulerError::InvalidProjectOnboarding(
                "new project parent must be a directory".into(),
            ));
        }
        let target = parent.join(name);
        fs::create_dir(&target)?;
        let root = target.canonicalize()?;
        Ok(Self {
            root,
            created_directory: true,
            cleanup_create_artifacts: true,
        })
    }

    fn rollback(&self) {
        if !self.cleanup_create_artifacts {
            return;
        }
        let result = if self.created_directory {
            fs::remove_dir_all(&self.root)
        } else {
            remove_create_artifacts(&self.root)
        };
        if let Err(error) = result {
            tracing::warn!(path = %self.root.display(), %error, "unable to roll back project onboarding artifacts");
        }
    }
}

fn remove_create_artifacts(root: &Path) -> std::io::Result<()> {
    for child in [root.join(".git"), root.join(".ba-agent")] {
        if child.is_dir() {
            fs::remove_dir_all(child)?;
        } else if child.is_file() {
            fs::remove_file(child)?;
        }
    }
    Ok(())
}

fn validate_language(language: &str) -> Result<(), SchedulerError> {
    if matches!(language, "zh-CN" | "en") {
        Ok(())
    } else {
        Err(SchedulerError::InvalidProjectOnboarding(
            "language must be zh-CN or en".into(),
        ))
    }
}

fn initialize_git(root: &Path) -> Result<(), SchedulerError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["init", "--initial-branch=main"])
        .output()?;
    if !output.status.success() {
        return Err(SchedulerError::GitInitialization(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    ensure_exact_git_root(root)
}

fn ensure_exact_git_root(root: &Path) -> Result<(), SchedulerError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        return Err(SchedulerError::InvalidProjectOnboarding(format!(
            "project root is not a Git repository: {}",
            root.display()
        )));
    }
    let actual = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()).canonicalize()?;
    if actual != root {
        return Err(SchedulerError::InvalidProjectOnboarding(format!(
            "project path must be the repository root: {}",
            actual.display()
        )));
    }
    Ok(())
}

fn initialize_project_layout(
    root: &Path,
    name: &str,
    language: &str,
) -> Result<bool, SchedulerError> {
    let config_root = root.join(".ba-agent");
    for directory in ["workflows", "plans", "knowledge", "receipts"] {
        fs::create_dir_all(config_root.join(directory))?;
    }
    let project_file = config_root.join("project.toml");
    if project_file.exists() {
        return Ok(false);
    }
    fs::write(
        project_file,
        format!(
            "version = 1\nname = {name:?}\nlanguage = {language:?}\nlog_retention_days = 90\nmax_concurrency = 4\n\n[gates]\nformat = []\nlint = []\ntypecheck = []\nchanged_tests = []\nsecret_scan = []\ndiff_review = []\n"
        ),
    )?;
    Ok(true)
}
