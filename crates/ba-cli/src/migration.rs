use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, bail};
use ba_config::{MergeOutcome, ProjectConfig, Runtime, merge_artifact};
use ba_core::{Artifact, EntityId, RunStatus, TaskStatus, canonical_json, now};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{MigrateCommand, app};

const PROJECT_INSTRUCTIONS: &str = "BA Agent manages this repository as a local-first development control plane. Preserve user-owned files, respect declared write scopes, run configured quality gates, and require human approval for push, merge, deploy, deletion, and privilege elevation.";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MigrationReport {
    mode: &'static str,
    source: String,
    target: String,
    files: Vec<FileReport>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileReport {
    category: String,
    source: String,
    target: String,
    outcome: String,
}

#[derive(Debug)]
struct PreparedWrite {
    report_index: usize,
    destination: PathBuf,
    contents: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeStateSnapshot {
    schema_version: u32,
    runtime: String,
    captured_at: String,
    #[serde(default)]
    tasks: Vec<ExternalTaskState>,
    #[serde(default)]
    runs: Vec<ExternalRunState>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExternalTaskState {
    external_id: String,
    title: String,
    status: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExternalRunState {
    external_id: String,
    #[serde(default)]
    task_external_id: Option<String>,
    status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStateImportReport {
    mode: &'static str,
    runtime: String,
    source: String,
    target: String,
    digest: String,
    task_count: usize,
    run_count: usize,
    outcome: String,
    artifact_id: Option<EntityId>,
    receipt_id: Option<EntityId>,
    warnings: Vec<String>,
}

pub fn run(paths: &app::DataPaths, command: MigrateCommand, json: bool) -> anyhow::Result<()> {
    match command {
        MigrateCommand::TheAgency {
            source,
            apply,
            project_root,
        } => migrate_the_agency(&source, apply, project_root.as_deref(), json),
        MigrateCommand::Config {
            plugin_root,
            project_root,
            apply,
        } => generate_config(paths, &plugin_root, &project_root, apply, json),
        MigrateCommand::RuntimeState {
            source,
            project_id,
            apply,
        } => import_runtime_state(paths, &source, &project_id, apply, json),
    }
}

fn migrate_the_agency(
    source: &Path,
    apply: bool,
    project_root: Option<&Path>,
    json: bool,
) -> anyhow::Result<()> {
    let source = source
        .canonicalize()
        .with_context(|| format!("TheAgency source does not exist: {}", source.display()))?;
    if !source.is_dir() {
        bail!("TheAgency source must be a directory");
    }
    let target_root = match project_root {
        Some(root) => root
            .canonicalize()
            .with_context(|| format!("project root does not exist: {}", root.display()))?,
        None if apply => bail!("--project-root is required with --apply"),
        None => PathBuf::from("<PROJECT_ROOT>"),
    };

    let mut report = MigrationReport {
        mode: if apply { "apply" } else { "dry-run" },
        source: source.to_string_lossy().into_owned(),
        target: target_root.to_string_lossy().into_owned(),
        files: Vec::new(),
        warnings: vec![
            "Application source code and upstream branding are intentionally excluded.".into(),
            "Review role mappings and imported receipts before relying on them.".into(),
        ],
    };

    let roots = [
        ("workflow", "workflows"),
        ("workflow", ".agency/workflows"),
        ("workflow", ".the-agency/workflows"),
        ("workflow", "docs/workflows"),
        ("role", "roles"),
        ("role", "agents"),
        ("role", ".codex/agents"),
        ("role", ".claude/agents"),
        ("receipt", "receipts"),
        ("receipt", ".agency/receipts"),
        ("receipt", ".the-agency/receipts"),
    ];
    for (category, relative_root) in roots {
        let candidate = source.join(relative_root);
        if !candidate.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&candidate).follow_links(false) {
            let entry = entry?;
            if !entry.file_type().is_file() || entry.path_is_symlink() {
                continue;
            }
            let extension = entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "md" | "json" | "yaml" | "yml" | "toml") {
                continue;
            }
            let relative = entry.path().strip_prefix(&candidate)?;
            ensure_safe_relative(relative)?;
            let destination = target_root
                .join(".ba-agent/imports/the-agency")
                .join(category)
                .join(relative);
            let outcome = if apply {
                if destination.exists() {
                    "conflict-existing"
                } else {
                    let parent = destination
                        .parent()
                        .context("migration target has no parent")?;
                    fs::create_dir_all(parent)?;
                    fs::copy(entry.path(), &destination)?;
                    "created"
                }
            } else if destination.exists() {
                "would-conflict"
            } else {
                "would-create"
            };
            report.files.push(FileReport {
                category: category.into(),
                source: entry.path().to_string_lossy().into_owned(),
                target: destination.to_string_lossy().into_owned(),
                outcome: outcome.into(),
            });
        }
    }
    report
        .files
        .sort_by(|left, right| left.target.cmp(&right.target));
    app::emit(&report, json)
}

fn generate_config(
    _paths: &app::DataPaths,
    plugin_root: &Path,
    project_root: &Path,
    apply: bool,
    json: bool,
) -> anyhow::Result<()> {
    let plugin_root = plugin_root
        .canonicalize()
        .with_context(|| format!("plugin root does not exist: {}", plugin_root.display()))?;
    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("project root does not exist: {}", project_root.display()))?;
    let config = ProjectConfig::from_plugin(&plugin_root, PROJECT_INSTRUCTIONS)?;
    let mut report = MigrationReport {
        mode: if apply { "apply" } else { "dry-run" },
        source: plugin_root.to_string_lossy().into_owned(),
        target: project_root.to_string_lossy().into_owned(),
        files: Vec::new(),
        warnings: Vec::new(),
    };
    let mut writes = Vec::new();

    for runtime in [Runtime::Codex, Runtime::Claude] {
        let category = match runtime {
            Runtime::Codex => "codex-config",
            Runtime::Claude => "claude-config",
        };
        for artifact in config.generate(runtime)?.artifacts {
            ensure_safe_relative(&artifact.relative_path)?;
            let destination = project_root.join(&artifact.relative_path);
            let existing = if destination.exists() {
                Some(fs::read_to_string(&destination).with_context(|| {
                    format!(
                        "generated configuration must be UTF-8: {}",
                        destination.display()
                    )
                })?)
            } else {
                None
            };
            match merge_artifact(existing.as_deref(), &artifact) {
                Ok(merged) => {
                    let outcome = format!("{:?}", merged.outcome).to_ascii_lowercase();
                    let report_index = report.files.len();
                    report.files.push(FileReport {
                        category: category.into(),
                        source: artifact.relative_path.to_string_lossy().into_owned(),
                        target: destination.to_string_lossy().into_owned(),
                        outcome,
                    });
                    if merged.outcome != MergeOutcome::Unchanged {
                        writes.push(PreparedWrite {
                            report_index,
                            destination,
                            contents: merged.contents,
                        });
                    }
                }
                Err(error) => {
                    report.files.push(FileReport {
                        category: category.into(),
                        source: artifact.relative_path.to_string_lossy().into_owned(),
                        target: destination.to_string_lossy().into_owned(),
                        outcome: format!("conflict: {error}"),
                    });
                }
            }
        }
    }
    let has_conflict = report
        .files
        .iter()
        .any(|item| item.outcome.starts_with("conflict:"));
    if apply {
        if has_conflict {
            for prepared in &writes {
                report.files[prepared.report_index].outcome = "not-applied-due-to-conflict".into();
            }
            report.warnings.push(
                "Configuration apply was aborted before writing because at least one user-owned file conflicted."
                    .into(),
            );
        } else {
            for prepared in writes {
                let parent = prepared
                    .destination
                    .parent()
                    .context("configuration target has no parent")?;
                fs::create_dir_all(parent)?;
                fs::write(prepared.destination, prepared.contents)?;
            }
        }
    }
    report
        .files
        .sort_by(|left, right| left.target.cmp(&right.target));
    app::emit(&report, json)
}

fn import_runtime_state(
    paths: &app::DataPaths,
    source: &Path,
    project_id: &str,
    apply: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let (source, snapshot, canonical, digest) = load_runtime_snapshot(source)?;
    let project_id = EntityId::from_str(project_id)?;
    let store = paths.store()?;
    let project = store
        .get_project(&project_id)?
        .context("project not found")?;
    let project_root = PathBuf::from(&project.root_path)
        .canonicalize()
        .with_context(|| format!("project root does not exist: {}", project.root_path))?;
    let relative_path = PathBuf::from(".ba-agent")
        .join("imports")
        .join("runtime-state")
        .join(&snapshot.runtime)
        .join(format!("{digest}.json"));
    ensure_safe_relative(&relative_path)?;
    let destination = guarded_project_destination(&project_root, &relative_path, false)?;
    let warnings = vec![
        "Imported status is immutable evidence and does not mutate BA Agent task or run state."
            .into(),
        "Live Claude dispatch remains unsupported in the personal Alpha.".into(),
    ];

    if let Some(existing) = store
        .list_artifacts(&project_id)?
        .into_iter()
        .find(|artifact| {
            artifact.kind == "runtime_state_import" && artifact.digest.as_deref() == Some(&digest)
        })
    {
        let (outcome, receipt_id) = reconcile_existing_runtime_snapshot(
            &store,
            &project_root,
            &relative_path,
            &snapshot,
            &canonical,
            &existing,
            apply,
        )?;
        return app::emit(
            &RuntimeStateImportReport {
                mode: if apply { "apply" } else { "dry-run" },
                runtime: snapshot.runtime,
                source: source.to_string_lossy().into_owned(),
                target: destination.to_string_lossy().into_owned(),
                digest,
                task_count: snapshot.tasks.len(),
                run_count: snapshot.runs.len(),
                outcome: outcome.into(),
                artifact_id: Some(existing.id),
                receipt_id,
                warnings,
            },
            json_output,
        );
    }

    let mut report = RuntimeStateImportReport {
        mode: if apply { "apply" } else { "dry-run" },
        runtime: snapshot.runtime.clone(),
        source: source.to_string_lossy().into_owned(),
        target: destination.to_string_lossy().into_owned(),
        digest: digest.clone(),
        task_count: snapshot.tasks.len(),
        run_count: snapshot.runs.len(),
        outcome: "would-create".into(),
        artifact_id: None,
        receipt_id: None,
        warnings,
    };
    if !apply {
        return app::emit(&report, json_output);
    }

    let (artifact_id, receipt_id) = persist_runtime_snapshot(
        &store,
        &project_id,
        &project_root,
        &relative_path,
        &snapshot,
        &canonical,
        &digest,
    )?;
    store.verify_receipt_chain(&project_id)?;
    report.outcome = "created".into();
    report.artifact_id = Some(artifact_id);
    report.receipt_id = Some(receipt_id);
    app::emit(&report, json_output)
}

fn reconcile_existing_runtime_snapshot(
    store: &ba_store::SqliteStore,
    project_root: &Path,
    relative_path: &Path,
    snapshot: &RuntimeStateSnapshot,
    canonical: &str,
    existing: &Artifact,
    apply: bool,
) -> anyhow::Result<(&'static str, Option<EntityId>)> {
    let expected_path = relative_path.to_string_lossy().replace('\\', "/");
    if existing.path != expected_path {
        bail!(
            "existing runtime state artifact path does not match its digest target: {}",
            existing.path
        );
    }
    let destination = guarded_project_destination(project_root, relative_path, false)?;
    let rendered = format!("{canonical}\n");
    if destination.exists() {
        if fs::read_to_string(&destination)? != rendered {
            bail!(
                "runtime state destination exists with different content: {}",
                destination.display()
            );
        }
        return Ok(("unchanged", None));
    }
    if !apply {
        return Ok(("would-restore", None));
    }

    let destination = guarded_project_destination(project_root, relative_path, true)?;
    write_new_file(&destination, &rendered)?;
    let receipt = store.append_receipt(
        &existing.project_id,
        None,
        None,
        "runtime_state_import_restored",
        json!({
            "artifactId": existing.id,
            "runtime": snapshot.runtime,
            "digest": existing.digest,
            "path": expected_path,
        }),
    );
    let receipt = match receipt {
        Ok(receipt) => receipt,
        Err(error) => {
            let _ignored = fs::remove_file(&destination);
            return Err(error.into());
        }
    };
    store.verify_receipt_chain(&existing.project_id)?;
    Ok(("restored", Some(receipt.id)))
}

fn load_runtime_snapshot(
    source: &Path,
) -> anyhow::Result<(PathBuf, RuntimeStateSnapshot, String, String)> {
    let source = source.canonicalize().with_context(|| {
        format!(
            "runtime state snapshot does not exist: {}",
            source.display()
        )
    })?;
    let metadata = fs::metadata(&source)?;
    if !metadata.is_file() {
        bail!("runtime state snapshot must be a file");
    }
    if metadata.len() > 10 * 1_024 * 1_024 {
        bail!("runtime state snapshot exceeds the 10 MiB import limit");
    }
    let snapshot: RuntimeStateSnapshot = serde_json::from_str(&fs::read_to_string(&source)?)?;
    validate_runtime_snapshot(&snapshot)?;
    let canonical = canonical_json(&snapshot)?;
    let digest = hex::encode(Sha256::digest(canonical.as_bytes()));
    Ok((source, snapshot, canonical, digest))
}

fn persist_runtime_snapshot(
    store: &ba_store::SqliteStore,
    project_id: &EntityId,
    project_root: &Path,
    relative_path: &Path,
    snapshot: &RuntimeStateSnapshot,
    canonical: &str,
    digest: &str,
) -> anyhow::Result<(EntityId, EntityId)> {
    let rendered = format!("{canonical}\n");
    let destination = guarded_project_destination(project_root, relative_path, true)?;
    let mut created_file = false;
    if destination.exists() {
        if fs::read_to_string(&destination)? != rendered {
            bail!(
                "runtime state destination exists with different content: {}",
                destination.display()
            );
        }
    } else {
        write_new_file(&destination, &rendered)?;
        created_file = true;
    }
    let artifact = Artifact {
        id: EntityId::new(),
        project_id: project_id.clone(),
        task_id: None,
        run_id: None,
        kind: "runtime_state_import".into(),
        path: relative_path.to_string_lossy().replace('\\', "/"),
        digest: Some(digest.to_owned()),
        metadata: serde_json::Value::Null,
        created_at: now(),
    };
    let recorded = store.record_artifact_with_receipt(
        &artifact,
        "runtime_state_import",
        json!({
            "artifactId": artifact.id,
            "runtime": snapshot.runtime,
            "capturedAt": snapshot.captured_at,
            "digest": digest,
            "taskCount": snapshot.tasks.len(),
            "runCount": snapshot.runs.len()
        }),
    );
    match recorded {
        Ok(receipt) => Ok((artifact.id, receipt.id)),
        Err(error) => {
            if created_file {
                let _ignored = fs::remove_file(&destination);
            }
            Err(error.into())
        }
    }
}

fn write_new_file(destination: &Path, contents: &str) -> anyhow::Result<()> {
    let parent = destination
        .parent()
        .context("runtime state destination has no parent")?;
    let temporary = parent.join(format!(".{}.tmp", EntityId::new()));
    fs::write(&temporary, contents)?;
    if let Err(error) = fs::rename(&temporary, destination) {
        let _ignored = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn guarded_project_destination(
    project_root: &Path,
    relative_path: &Path,
    create_parents: bool,
) -> anyhow::Result<PathBuf> {
    ensure_safe_relative(relative_path)?;
    let file_name = relative_path
        .file_name()
        .context("runtime state destination has no file name")?;
    let parent = relative_path
        .parent()
        .context("runtime state destination has no parent")?;
    let mut current = project_root.to_path_buf();
    for component in parent.components() {
        let Component::Normal(name) = component else {
            bail!("unsafe generated path: {}", relative_path.display());
        };
        let candidate = current.join(name);
        if candidate.exists() {
            let metadata = fs::symlink_metadata(&candidate)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                bail!(
                    "runtime state parent must be a real directory inside the project: {}",
                    candidate.display()
                );
            }
            let resolved = candidate.canonicalize()?;
            if !resolved.starts_with(project_root) {
                bail!(
                    "runtime state path escapes the project root: {}",
                    candidate.display()
                );
            }
            current = resolved;
        } else {
            if create_parents {
                fs::create_dir(&candidate)?;
            }
            current = candidate;
        }
    }
    let destination = current.join(file_name);
    if destination.exists() {
        let metadata = fs::symlink_metadata(&destination)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!(
                "runtime state destination must be a regular file: {}",
                destination.display()
            );
        }
        let resolved = destination.canonicalize()?;
        if !resolved.starts_with(project_root) {
            bail!(
                "runtime state path escapes the project root: {}",
                destination.display()
            );
        }
    }
    Ok(destination)
}

fn validate_runtime_snapshot(snapshot: &RuntimeStateSnapshot) -> anyhow::Result<()> {
    if snapshot.schema_version != 1 {
        bail!(
            "unsupported runtime state schema version: {}",
            snapshot.schema_version
        );
    }
    if !matches!(snapshot.runtime.as_str(), "codex" | "claude") {
        bail!("unsupported runtime state adapter: {}", snapshot.runtime);
    }
    chrono::DateTime::parse_from_rfc3339(&snapshot.captured_at)
        .context("capturedAt must be an RFC3339 timestamp")?;
    let mut task_ids = BTreeSet::new();
    for task in &snapshot.tasks {
        validate_external_id(&task.external_id)?;
        if task.title.trim().is_empty() {
            bail!("external task title must not be empty");
        }
        if !task_ids.insert(task.external_id.as_str()) {
            bail!("duplicate external task id: {}", task.external_id);
        }
        TaskStatus::from_str(&task.status)?;
    }
    let mut run_ids = BTreeSet::new();
    for run in &snapshot.runs {
        validate_external_id(&run.external_id)?;
        if !run_ids.insert(run.external_id.as_str()) {
            bail!("duplicate external run id: {}", run.external_id);
        }
        if let Some(task_external_id) = &run.task_external_id
            && !task_ids.contains(task_external_id.as_str())
        {
            bail!(
                "external run {} references unknown task {}",
                run.external_id,
                task_external_id
            );
        }
        RunStatus::from_str(&run.status)?;
    }
    Ok(())
}

fn validate_external_id(value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        bail!("invalid external state identifier");
    }
    Ok(())
}

fn ensure_safe_relative(path: &Path) -> anyhow::Result<()> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("unsafe generated path: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agency_import_excludes_application_source() -> anyhow::Result<()> {
        let source = tempfile::tempdir()?;
        let target = tempfile::tempdir()?;
        fs::create_dir_all(source.path().join("workflows"))?;
        fs::create_dir_all(source.path().join("src"))?;
        fs::write(source.path().join("workflows/flow.md"), "# Workflow\n")?;
        fs::write(
            source.path().join("src/app.ts"),
            "export const copied = false;\n",
        )?;

        migrate_the_agency(source.path(), true, Some(target.path()), true)?;

        assert!(
            target
                .path()
                .join(".ba-agent/imports/the-agency/workflow/flow.md")
                .is_file()
        );
        assert!(
            !target
                .path()
                .join(".ba-agent/imports/the-agency/src/app.ts")
                .exists()
        );
        Ok(())
    }

    #[test]
    fn config_apply_aborts_all_writes_on_managed_content_conflict() -> anyhow::Result<()> {
        let project = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let paths = app::DataPaths::resolve(Some(data.path().to_path_buf()))?;
        let plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("plugins/personal-marketplace/plugins/ba-agent-control");
        fs::write(project.path().join("AGENTS.md"), "user-owned preface\n")?;
        generate_config(&paths, &plugin, project.path(), true, true)?;

        let agents = project.path().join("AGENTS.md");
        let tampered = fs::read_to_string(&agents)?.replace(
            "BA Agent manages this repository",
            "user changed the managed body",
        );
        fs::write(&agents, &tampered)?;
        let generated_role = project.path().join(".codex/agents/coordinator.toml");
        fs::remove_file(&generated_role)?;

        generate_config(&paths, &plugin, project.path(), true, true)?;

        assert_eq!(fs::read_to_string(agents)?, tampered);
        assert!(!generated_role.exists());
        Ok(())
    }

    #[test]
    fn generated_paths_reject_parent_traversal() {
        assert!(ensure_safe_relative(Path::new("../outside")).is_err());
        assert!(ensure_safe_relative(Path::new(".codex/agents/reviewer.toml")).is_ok());
    }

    #[test]
    fn codex_claude_config_and_imported_state_keep_one_receipt_chain() -> anyhow::Result<()> {
        let project_root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let source_root = tempfile::tempdir()?;
        let paths = app::DataPaths::resolve(Some(data.path().to_path_buf()))?;
        let store = paths.store()?;
        let stamp = now();
        let project = ba_core::Project {
            id: EntityId::new(),
            name: "Compatibility demo".into(),
            root_path: project_root.path().to_string_lossy().into_owned(),
            description: String::new(),
            language: "zh-CN".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        let plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("plugins/personal-marketplace/plugins/ba-agent-control");
        fs::write(project_root.path().join("AGENTS.md"), "user Codex note\n")?;
        fs::write(project_root.path().join("CLAUDE.md"), "user Claude note\n")?;
        generate_config(&paths, &plugin, project_root.path(), true, true)?;

        assert!(
            fs::read_to_string(project_root.path().join("AGENTS.md"))?
                .starts_with("user Codex note")
        );
        assert!(
            fs::read_to_string(project_root.path().join("CLAUDE.md"))?
                .starts_with("user Claude note")
        );
        assert_eq!(
            WalkDir::new(project_root.path().join(".codex/agents"))
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .count(),
            8
        );
        assert_eq!(
            WalkDir::new(project_root.path().join(".claude/skills"))
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name() == "SKILL.md")
                .count(),
            11
        );

        let snapshot_path = source_root.path().join("claude-state.json");
        fs::write(
            &snapshot_path,
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "runtime": "claude",
                "capturedAt": "2026-08-24T09:00:00Z",
                "tasks": [{"externalId":"task-1","title":"Review compatibility","status":"completed"}],
                "runs": [{"externalId":"run-1","taskExternalId":"task-1","status":"succeeded"}]
            }))?,
        )?;
        import_runtime_state(&paths, &snapshot_path, project.id.as_str(), false, true)?;
        assert!(store.list_artifacts(&project.id)?.is_empty());
        assert!(store.list_receipts(&project.id)?.is_empty());

        import_runtime_state(&paths, &snapshot_path, project.id.as_str(), true, true)?;
        let artifacts = store.list_artifacts(&project.id)?;
        let receipts = store.list_receipts(&project.id)?;
        assert_eq!(artifacts.len(), 1);
        assert_eq!(receipts.len(), 1);
        let receipt_hash = receipts[0].hash.clone();
        store.verify_receipt_chain(&project.id)?;

        generate_config(&paths, &plugin, project_root.path(), true, true)?;
        import_runtime_state(&paths, &snapshot_path, project.id.as_str(), true, true)?;
        assert_eq!(store.list_artifacts(&project.id)?.len(), 1);
        let receipts = store.list_receipts(&project.id)?;
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].hash, receipt_hash);
        store.verify_receipt_chain(&project.id)?;

        let artifact = store.list_artifacts(&project.id)?.remove(0);
        let imported_path = project_root.path().join(&artifact.path);
        assert!(imported_path.starts_with(project_root.path()));
        fs::remove_file(&imported_path)?;
        assert!(!imported_path.exists());

        import_runtime_state(&paths, &snapshot_path, project.id.as_str(), false, true)?;
        assert!(!imported_path.exists());
        assert_eq!(store.list_receipts(&project.id)?.len(), 1);

        import_runtime_state(&paths, &snapshot_path, project.id.as_str(), true, true)?;
        assert!(imported_path.is_file());
        assert_eq!(store.list_artifacts(&project.id)?.len(), 1);
        let receipts = store.list_receipts(&project.id)?;
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[1].kind, "runtime_state_import_restored");
        assert_eq!(receipts[1].payload["artifactId"], artifact.id.as_str());
        store.verify_receipt_chain(&project.id)?;
        Ok(())
    }

    #[test]
    fn runtime_state_destination_rejects_non_directory_parent() -> anyhow::Result<()> {
        let project_root = tempfile::tempdir()?;
        fs::write(project_root.path().join(".ba-agent"), "not a directory")?;
        let result = guarded_project_destination(
            project_root.path(),
            Path::new(".ba-agent/imports/runtime-state/claude/state.json"),
            true,
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn runtime_state_restore_rejects_mismatched_artifact_path() -> anyhow::Result<()> {
        let project_root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let paths = app::DataPaths::resolve(Some(data.path().to_path_buf()))?;
        let snapshot = RuntimeStateSnapshot {
            schema_version: 1,
            runtime: "claude".into(),
            captured_at: "2026-08-24T09:00:00Z".into(),
            tasks: Vec::new(),
            runs: Vec::new(),
        };
        let artifact = Artifact {
            id: EntityId::new(),
            project_id: EntityId::new(),
            task_id: None,
            run_id: None,
            kind: "runtime_state_import".into(),
            path: ".ba-agent/imports/runtime-state/claude/other.json".into(),
            digest: Some("digest".into()),
            metadata: serde_json::Value::Null,
            created_at: now(),
        };
        let result = reconcile_existing_runtime_snapshot(
            &paths.store()?,
            project_root.path(),
            Path::new(".ba-agent/imports/runtime-state/claude/state.json"),
            &snapshot,
            "{}",
            &artifact,
            false,
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn runtime_state_rejects_unknown_task_reference() {
        let snapshot = RuntimeStateSnapshot {
            schema_version: 1,
            runtime: "claude".into(),
            captured_at: "2026-08-24T09:00:00Z".into(),
            tasks: Vec::new(),
            runs: vec![ExternalRunState {
                external_id: "run-1".into(),
                task_external_id: Some("missing-task".into()),
                status: "succeeded".into(),
            }],
        };
        assert!(validate_runtime_snapshot(&snapshot).is_err());
    }
}
