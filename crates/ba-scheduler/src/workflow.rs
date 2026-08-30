use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use ba_core::{AgentProfile, EntityId, Milestone, Project, Task, TaskStatus, Workstream, now};
use ba_git::RepoPath;
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};

use crate::SchedulerError;

const WRITE_SCOPE_PLACEHOLDER: &str = "$WRITE_SCOPE";
const MAX_PROJECT_TEMPLATES: usize = 64;
const MAX_TEMPLATE_BYTES: u64 = 256 * 1024;
const BUILTIN_TEMPLATE_SOURCES: [(&str, &str); 6] = [
    (
        "feature-delivery.toml",
        include_str!("../templates/feature-delivery.toml"),
    ),
    (
        "defect-investigation.toml",
        include_str!("../templates/defect-investigation.toml"),
    ),
    (
        "security-review.toml",
        include_str!("../templates/security-review.toml"),
    ),
    (
        "migration.toml",
        include_str!("../templates/migration.toml"),
    ),
    (
        "release-preparation.toml",
        include_str!("../templates/release-preparation.toml"),
    ),
    (
        "documentation-verification.toml",
        include_str!("../templates/documentation-verification.toml"),
    ),
];

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTemplateOrigin {
    #[default]
    Builtin,
    Project,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowTemplateTask {
    pub key: String,
    pub title_zh: String,
    pub title_en: String,
    #[serde(default)]
    pub description_zh: String,
    #[serde(default)]
    pub description_en: String,
    pub role: String,
    #[serde(default)]
    pub write: bool,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub path_scope: Vec<String>,
    pub acceptance_zh: Vec<String>,
    pub acceptance_en: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowTemplate {
    pub version: u32,
    pub id: String,
    pub name_zh: String,
    pub name_en: String,
    pub description_zh: String,
    pub description_en: String,
    pub tasks: Vec<WorkflowTemplateTask>,
    #[serde(default, skip_deserializing)]
    pub origin: WorkflowTemplateOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTemplateDiagnostic {
    pub path: String,
    pub error: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTemplateCatalog {
    pub templates: Vec<WorkflowTemplate>,
    pub diagnostics: Vec<WorkflowTemplateDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowInstantiationRequest {
    pub project_id: EntityId,
    pub template_id: String,
    pub workstream_name: Option<String>,
    pub milestone_id: Option<EntityId>,
    pub write_scope: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInstance {
    pub template_id: String,
    pub workstream: Workstream,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug)]
pub struct WorkflowTemplateService {
    store: SqliteStore,
}

impl WorkflowTemplateService {
    #[must_use]
    pub const fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// List built-in templates plus valid project overrides.
    ///
    /// Invalid project files become diagnostics so one local editing mistake does
    /// not hide the usable built-in catalog.
    ///
    /// # Errors
    ///
    /// Returns an error when the project or workflow directory is unavailable.
    pub fn catalog(
        &self,
        project_id: &EntityId,
    ) -> Result<WorkflowTemplateCatalog, SchedulerError> {
        let project = self.project(project_id)?;
        let mut templates = builtin_templates()?
            .into_iter()
            .map(|template| (template.id.clone(), template))
            .collect::<BTreeMap<_, _>>();
        let mut diagnostics = Vec::new();
        let root = PathBuf::from(&project.root_path).canonicalize()?;
        let workflow_root = root.join(".ba-agent/workflows");
        if workflow_root.exists() {
            let workflow_root = workflow_root.canonicalize()?;
            if !workflow_root.starts_with(&root) {
                return Err(SchedulerError::InvalidWorkflowTemplate(
                    "project workflow directory resolves outside the repository".into(),
                ));
            }
            let mut entries = fs::read_dir(&workflow_root)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(fs::DirEntry::file_name);
            let mut project_template_count = 0;
            for entry in entries {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                    continue;
                }
                project_template_count += 1;
                if project_template_count > MAX_PROJECT_TEMPLATES {
                    diagnostics.push(WorkflowTemplateDiagnostic {
                        path: workflow_root.to_string_lossy().into_owned(),
                        error: format!(
                            "project template limit of {MAX_PROJECT_TEMPLATES} was exceeded"
                        ),
                    });
                    break;
                }
                match load_project_template(&workflow_root, &path) {
                    Ok(template) => {
                        templates.insert(template.id.clone(), template);
                    }
                    Err(error) => diagnostics.push(WorkflowTemplateDiagnostic {
                        path: path.to_string_lossy().into_owned(),
                        error: error.to_string(),
                    }),
                }
            }
        }
        Ok(WorkflowTemplateCatalog {
            templates: templates.into_values().collect(),
            diagnostics,
        })
    }

    /// Copy a built-in template into the project for explicit user editing.
    /// Existing files are never replaced.
    ///
    /// # Errors
    ///
    /// Returns an error when the project or template is missing or the destination exists.
    pub fn materialize(
        &self,
        project_id: &EntityId,
        template_id: &str,
    ) -> Result<PathBuf, SchedulerError> {
        let project = self.project(project_id)?;
        let (file_name, source) = BUILTIN_TEMPLATE_SOURCES
            .iter()
            .find(|(_, source)| template_id_from_source(source).as_deref() == Some(template_id))
            .ok_or_else(|| SchedulerError::WorkflowTemplateNotFound(template_id.into()))?;
        let root = PathBuf::from(project.root_path).canonicalize()?;
        let workflow_root = root.join(".ba-agent/workflows");
        fs::create_dir_all(&workflow_root)?;
        let workflow_root = workflow_root.canonicalize()?;
        if !workflow_root.starts_with(&root) {
            return Err(SchedulerError::InvalidWorkflowTemplate(
                "project workflow directory resolves outside the repository".into(),
            ));
        }
        let destination = workflow_root.join(file_name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        Ok(destination)
    }

    /// Instantiate one validated template as an atomic workstream/task graph.
    ///
    /// # Errors
    ///
    /// Returns an error when template validation, role resolution, or the atomic insert fails.
    pub fn instantiate(
        &self,
        request: &WorkflowInstantiationRequest,
    ) -> Result<WorkflowInstance, SchedulerError> {
        let project = self.project(&request.project_id)?;
        let catalog = self.catalog(&request.project_id)?;
        let template = catalog
            .templates
            .into_iter()
            .find(|template| template.id == request.template_id)
            .ok_or_else(|| SchedulerError::WorkflowTemplateNotFound(request.template_id.clone()))?;
        let milestone = request
            .milestone_id
            .as_ref()
            .map(|id| self.store.get_milestone(id))
            .transpose()?
            .flatten();
        validate_instance_milestone(&project, milestone.as_ref())?;
        let profile_records = self.store.list_agent_profiles()?;
        let profiles = profiles_by_role(&profile_records);
        let default_scope = validated_scope(&request.write_scope)?;
        let stamp = now();
        let workstream = Workstream {
            id: EntityId::new(),
            project_id: project.id.clone(),
            name: request
                .workstream_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map_or_else(
                    || localized(&project, &template.name_zh, &template.name_en).to_owned(),
                    str::to_owned,
                ),
            description: localized(&project, &template.description_zh, &template.description_en)
                .to_owned(),
            status: "active".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        let ids = template
            .tasks
            .iter()
            .map(|task| (task.key.clone(), EntityId::new()))
            .collect::<BTreeMap<_, _>>();
        let dependents = template_dependents(&template);
        let tasks = template
            .tasks
            .iter()
            .map(|definition| {
                instantiate_task(
                    &project,
                    &workstream,
                    milestone.as_ref(),
                    definition,
                    &ids,
                    dependents.get(&definition.key),
                    &profiles,
                    &default_scope,
                    stamp,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.store.create_workflow_instance(&workstream, &tasks)?;
        Ok(WorkflowInstance {
            template_id: template.id,
            workstream,
            tasks,
        })
    }

    fn project(&self, id: &EntityId) -> Result<Project, SchedulerError> {
        self.store.get_project(id)?.ok_or_else(|| {
            SchedulerError::InvalidWorkflowTemplate(format!("project {id} not found"))
        })
    }
}

fn builtin_templates() -> Result<Vec<WorkflowTemplate>, SchedulerError> {
    BUILTIN_TEMPLATE_SOURCES
        .iter()
        .map(|(_, source)| {
            let mut template: WorkflowTemplate = toml::from_str(source)?;
            template.origin = WorkflowTemplateOrigin::Builtin;
            validate_template(&template)?;
            Ok(template)
        })
        .collect()
}

fn template_id_from_source(source: &str) -> Option<String> {
    toml::from_str::<WorkflowTemplate>(source)
        .ok()
        .map(|template| template.id)
}

fn load_project_template(
    workflow_root: &Path,
    path: &Path,
) -> Result<WorkflowTemplate, SchedulerError> {
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(workflow_root) || !canonical.is_file() {
        return Err(SchedulerError::InvalidWorkflowTemplate(
            "template path resolves outside the project workflow directory".into(),
        ));
    }
    if fs::metadata(&canonical)?.len() > MAX_TEMPLATE_BYTES {
        return Err(SchedulerError::InvalidWorkflowTemplate(format!(
            "template exceeds {MAX_TEMPLATE_BYTES} bytes"
        )));
    }
    let mut template: WorkflowTemplate = toml::from_str(&fs::read_to_string(canonical)?)?;
    template.origin = WorkflowTemplateOrigin::Project;
    validate_template(&template)?;
    Ok(template)
}

fn validate_template(template: &WorkflowTemplate) -> Result<(), SchedulerError> {
    if template.version != 1 {
        return Err(invalid_template("version must be 1"));
    }
    if !valid_slug(&template.id) {
        return Err(invalid_template(
            "id must use lowercase ASCII letters, digits, and single hyphens",
        ));
    }
    if template.name_zh.trim().is_empty()
        || template.name_en.trim().is_empty()
        || template.tasks.is_empty()
        || template.tasks.len() > 32
    {
        return Err(invalid_template(
            "localized names and between 1 and 32 tasks are required",
        ));
    }
    let keys = template
        .tasks
        .iter()
        .map(|task| task.key.as_str())
        .collect::<BTreeSet<_>>();
    if keys.len() != template.tasks.len() {
        return Err(invalid_template("task keys must be unique"));
    }
    for task in &template.tasks {
        if !valid_slug(&task.key)
            || task.title_zh.trim().is_empty()
            || task.title_en.trim().is_empty()
            || task.role.trim().is_empty()
            || task.acceptance_zh.is_empty()
            || task.acceptance_en.is_empty()
            || task
                .depends_on
                .iter()
                .any(|key| !keys.contains(key.as_str()))
            || task.depends_on.iter().any(|key| key == &task.key)
        {
            return Err(invalid_template(format!(
                "task {} has invalid identity, role, acceptance, or dependencies",
                task.key
            )));
        }
        if task.write == task.path_scope.is_empty() {
            return Err(invalid_template(format!(
                "task {} must declare scope exactly when it writes",
                task.key
            )));
        }
        for scope in &task.path_scope {
            if scope != WRITE_SCOPE_PLACEHOLDER {
                RepoPath::new(scope).map_err(|error| invalid_template(error.to_string()))?;
            }
        }
    }
    ensure_acyclic(template, &keys)
}

fn ensure_acyclic(
    template: &WorkflowTemplate,
    keys: &BTreeSet<&str>,
) -> Result<(), SchedulerError> {
    let mut completed = BTreeSet::new();
    while completed.len() < keys.len() {
        let before = completed.len();
        for task in &template.tasks {
            if !completed.contains(task.key.as_str())
                && task
                    .depends_on
                    .iter()
                    .all(|dependency| completed.contains(dependency.as_str()))
            {
                completed.insert(task.key.as_str());
            }
        }
        if completed.len() == before {
            return Err(invalid_template("task dependency graph contains a cycle"));
        }
    }
    Ok(())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn invalid_template(message: impl Into<String>) -> SchedulerError {
    SchedulerError::InvalidWorkflowTemplate(message.into())
}

fn validated_scope(scope: &[String]) -> Result<Vec<String>, SchedulerError> {
    let mut normalized = BTreeSet::new();
    for path in scope {
        normalized.insert(
            RepoPath::new(path)
                .map_err(|error| invalid_template(format!("invalid write scope: {error}")))?
                .as_path()
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    Ok(normalized.into_iter().collect())
}

fn profiles_by_role(profiles: &[AgentProfile]) -> BTreeMap<&str, &AgentProfile> {
    profiles
        .iter()
        .map(|profile| (profile.role.as_str(), profile))
        .collect()
}

fn validate_instance_milestone(
    project: &Project,
    milestone: Option<&Milestone>,
) -> Result<(), SchedulerError> {
    if let Some(milestone) = milestone {
        if milestone.project_id != project.id || milestone.workstream_id.is_some() {
            return Err(invalid_template(
                "workflow templates can attach only to a project-level milestone in the same project",
            ));
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "template instantiation makes every validated linkage explicit"
)]
fn instantiate_task(
    project: &Project,
    workstream: &Workstream,
    milestone: Option<&Milestone>,
    definition: &WorkflowTemplateTask,
    ids: &BTreeMap<String, EntityId>,
    dependents: Option<&Vec<String>>,
    profiles: &BTreeMap<&str, &AgentProfile>,
    default_scope: &[String],
    stamp: ba_core::Timestamp,
) -> Result<Task, SchedulerError> {
    let profile = profiles.get(definition.role.as_str()).ok_or_else(|| {
        invalid_template(format!(
            "template role {} has no durable Agent profile",
            definition.role
        ))
    })?;
    if definition.write && profile.config["sandboxMode"] != "workspace-write" {
        return Err(invalid_template(format!(
            "role {} cannot execute write task {}",
            definition.role, definition.key
        )));
    }
    let mut scopes = Vec::new();
    for scope in &definition.path_scope {
        if scope == WRITE_SCOPE_PLACEHOLDER {
            if default_scope.is_empty() {
                return Err(invalid_template(format!(
                    "template task {} requires a write scope",
                    definition.key
                )));
            }
            scopes.extend(default_scope.iter().cloned());
        } else {
            scopes.push(scope.clone());
        }
    }
    scopes.sort();
    scopes.dedup();
    let depends_on = definition
        .depends_on
        .iter()
        .map(|key| {
            ids.get(key)
                .cloned()
                .ok_or_else(|| invalid_template(format!("dependency task {key} is missing")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let downstream_ids = dependents
        .into_iter()
        .flatten()
        .filter_map(|key| ids.get(key))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let zh = project.language != "en";
    let description = task_description(definition, zh, &downstream_ids);
    Ok(Task {
        id: ids
            .get(&definition.key)
            .cloned()
            .ok_or_else(|| invalid_template("template task id is missing"))?,
        project_id: project.id.clone(),
        workstream_id: Some(workstream.id.clone()),
        milestone_id: milestone.map(|item| item.id.clone()),
        title: if zh {
            definition.title_zh.clone()
        } else {
            definition.title_en.clone()
        },
        description,
        status: TaskStatus::Draft,
        write: definition.write,
        path_scope: scopes,
        depends_on,
        agent_profile_id: Some(profile.id.clone()),
        created_at: stamp,
        updated_at: stamp,
    })
}

fn task_description(task: &WorkflowTemplateTask, zh: bool, downstream_ids: &[String]) -> String {
    let description = if zh {
        task.description_zh.trim()
    } else {
        task.description_en.trim()
    };
    let acceptance = if zh {
        &task.acceptance_zh
    } else {
        &task.acceptance_en
    };
    let mut sections = Vec::new();
    if !description.is_empty() {
        sections.push(description.to_owned());
    }
    sections.push(format!(
        "{}\n{}",
        if zh {
            "验收条件："
        } else {
            "Acceptance criteria:"
        },
        acceptance
            .iter()
            .map(|item| format!("- {}", item.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    ));
    if !downstream_ids.is_empty() {
        sections.push(if zh {
            format!(
                "完成后通过 BA Agent 的 handoff.create 提交结构化交接，目标任务：{}。不得只把关键信息留在聊天文本中。",
                downstream_ids.join(", ")
            )
        } else {
            format!(
                "When finished, submit a structured handoff through BA Agent handoff.create for downstream tasks: {}. Do not leave critical state only in chat text.",
                downstream_ids.join(", ")
            )
        });
    }
    sections.join("\n\n")
}

fn template_dependents(template: &WorkflowTemplate) -> BTreeMap<String, Vec<String>> {
    let mut dependents = BTreeMap::<String, Vec<String>>::new();
    for task in &template.tasks {
        for dependency in &task.depends_on {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(task.key.clone());
        }
    }
    dependents
}

fn localized<'a>(project: &Project, zh: &'a str, en: &'a str) -> &'a str {
    if project.language == "en" { en } else { zh }
}
