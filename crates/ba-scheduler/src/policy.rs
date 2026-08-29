use std::{fs, path::Path};

use ba_core::{EntityId, Project};
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};

use crate::{DEFAULT_MAX_CONCURRENCY, SchedulerError};

const GLOBAL_POLICY_KEY: &str = "scheduler.policy";
pub const MAX_CONCURRENCY_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerPolicy {
    pub global_max_concurrency: usize,
}

impl Default for SchedulerPolicy {
    fn default() -> Self {
        Self {
            global_max_concurrency: DEFAULT_MAX_CONCURRENCY,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConcurrency {
    pub project_id: EntityId,
    pub requested_max_concurrency: usize,
    pub effective_max_concurrency: usize,
    pub capped_by_global: bool,
}

#[derive(Clone, Debug)]
pub struct SchedulerPolicyService {
    store: SqliteStore,
}

impl SchedulerPolicyService {
    #[must_use]
    pub const fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Read and validate the global shared concurrency policy.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid persisted values or SQLite failures.
    pub fn global(&self) -> Result<SchedulerPolicy, SchedulerError> {
        let policy: SchedulerPolicy = self
            .store
            .get_scheduler_setting(GLOBAL_POLICY_KEY)?
            .unwrap_or_default();
        validate_limit(policy.global_max_concurrency)?;
        Ok(policy)
    }

    /// Persist the global shared concurrency maximum.
    ///
    /// # Errors
    ///
    /// Returns an error when the limit is outside `1..=64` or persistence fails.
    pub fn set_global(&self, maximum: usize) -> Result<SchedulerPolicy, SchedulerError> {
        validate_limit(maximum)?;
        let policy = SchedulerPolicy {
            global_max_concurrency: maximum,
        };
        self.store
            .set_scheduler_setting(GLOBAL_POLICY_KEY, &policy)?;
        Ok(policy)
    }

    /// Resolve requested and effective concurrency for one project.
    ///
    /// # Errors
    ///
    /// Returns an error when the global policy or project TOML is invalid.
    pub fn project(&self, project: &Project) -> Result<ProjectConcurrency, SchedulerError> {
        let global = self.global()?.global_max_concurrency;
        let requested = read_project_limit(Path::new(&project.root_path))?;
        Ok(ProjectConcurrency {
            project_id: project.id.clone(),
            requested_max_concurrency: requested,
            effective_max_concurrency: requested.min(global),
            capped_by_global: requested > global,
        })
    }

    /// Persist one project's concurrency maximum without exceeding the global cap.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid limit or project configuration write.
    pub fn set_project(
        &self,
        project: &Project,
        maximum: usize,
    ) -> Result<ProjectConcurrency, SchedulerError> {
        validate_limit(maximum)?;
        let global = self.global()?.global_max_concurrency;
        if maximum > global {
            return Err(SchedulerError::InvalidProjectConcurrency {
                requested: maximum,
                global,
            });
        }
        write_project_limit(Path::new(&project.root_path), maximum)?;
        self.project(project)
    }
}

fn validate_limit(maximum: usize) -> Result<(), SchedulerError> {
    if maximum == 0 || maximum > MAX_CONCURRENCY_LIMIT {
        return Err(SchedulerError::ConcurrencyOutOfRange {
            maximum: MAX_CONCURRENCY_LIMIT,
        });
    }
    Ok(())
}

fn read_project_limit(project_root: &Path) -> Result<usize, SchedulerError> {
    let path = project_root.join(".ba-agent/project.toml");
    if !path.exists() {
        return Ok(DEFAULT_MAX_CONCURRENCY);
    }
    let source = fs::read_to_string(&path)?;
    let document = source
        .parse::<toml::Table>()
        .map_err(|error| SchedulerError::InvalidProjectConfig(error.to_string()))?;
    let value = document
        .get("max_concurrency")
        .and_then(toml::Value::as_integer)
        .map(usize::try_from)
        .transpose()
        .map_err(|_| {
            SchedulerError::InvalidProjectConfig("max_concurrency must be positive".into())
        })?
        .unwrap_or(DEFAULT_MAX_CONCURRENCY);
    validate_limit(value)?;
    Ok(value)
}

fn write_project_limit(project_root: &Path, maximum: usize) -> Result<(), SchedulerError> {
    let path = project_root.join(".ba-agent/project.toml");
    let mut source = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        fs::create_dir_all(path.parent().ok_or_else(|| {
            SchedulerError::InvalidProjectConfig("project config has no parent".into())
        })?)?;
        "version = 1\n".into()
    };
    let replacement = format!("max_concurrency = {maximum}");
    let mut replaced = false;
    let mut lines = Vec::new();
    for line in source.lines() {
        if !replaced && line.trim_start().starts_with("max_concurrency") {
            lines.push(replacement.clone());
            replaced = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !replaced {
        let section = lines
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .unwrap_or(lines.len());
        lines.insert(section, replacement);
    }
    source = lines.join("\n");
    source.push('\n');
    fs::write(path, source)?;
    Ok(())
}
