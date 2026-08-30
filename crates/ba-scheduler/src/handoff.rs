use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use ba_core::{Artifact, EntityId, Timestamp, canonical_json, now};
use ba_git::RepoPath;
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::SchedulerError;

const HANDOFF_KIND: &str = "agent_handoff";
const MAX_HANDOFF_BYTES: usize = 128 * 1024;
const MAX_LIST_ITEMS: usize = 64;
const MAX_ITEM_CHARS: usize = 2_000;
const MAX_OUTCOME_CHARS: usize = 8_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffCommandEvidence {
    pub command: String,
    pub result: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffDraft {
    pub source_task_id: EntityId,
    pub source_run_id: Option<EntityId>,
    pub destination_task_ids: Vec<EntityId>,
    pub outcome: String,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub changed_paths: Vec<String>,
    #[serde(default)]
    pub commands: Vec<HandoffCommandEvidence>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub next_steps: Vec<String>,
    #[serde(default)]
    pub artifact_ids: Vec<EntityId>,
    #[serde(default)]
    pub receipt_ids: Vec<EntityId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffRecord {
    pub id: EntityId,
    pub project_id: EntityId,
    pub source_task_id: EntityId,
    pub source_run_id: Option<EntityId>,
    pub destination_task_ids: Vec<EntityId>,
    pub outcome: String,
    pub decisions: Vec<String>,
    pub evidence: Vec<String>,
    pub changed_paths: Vec<String>,
    pub commands: Vec<HandoffCommandEvidence>,
    pub risks: Vec<String>,
    pub next_steps: Vec<String>,
    pub artifact_ids: Vec<EntityId>,
    pub receipt_ids: Vec<EntityId>,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffArtifact {
    pub handoff: HandoffRecord,
    pub artifact: Artifact,
}

#[derive(Clone, Debug)]
pub struct HandoffService {
    store: SqliteStore,
}

impl HandoffService {
    #[must_use]
    pub const fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Persist one canonical structured handoff and its Artifact metadata.
    ///
    /// The digest is required because a handoff may be consumed before the
    /// repository file is committed; Git cannot then detect a substituted result.
    ///
    /// # Errors
    ///
    /// Returns an error when links or paths are invalid, persistence fails, or the file exists.
    pub fn create(&self, draft: HandoffDraft) -> Result<HandoffArtifact, SchedulerError> {
        validate_draft_shape(&draft)?;
        let source_task = self
            .store
            .get_task(&draft.source_task_id)?
            .ok_or_else(|| invalid_handoff("source task not found"))?;
        if let Some(run_id) = &draft.source_run_id {
            let run = self
                .store
                .get_run(run_id)?
                .ok_or_else(|| invalid_handoff("source run not found"))?;
            if run.task_id != source_task.id {
                return Err(invalid_handoff("source run belongs to a different task"));
            }
        }
        for destination_id in &draft.destination_task_ids {
            let destination = self
                .store
                .get_task(destination_id)?
                .ok_or_else(|| invalid_handoff("destination task not found"))?;
            if destination.project_id != source_task.project_id
                || !destination.depends_on.contains(&source_task.id)
            {
                return Err(invalid_handoff(
                    "each destination must belong to the project and directly depend on the source task",
                ));
            }
        }
        validate_evidence_links(&self.store, &source_task.project_id, &draft)?;
        for path in &draft.changed_paths {
            RepoPath::new(path)
                .map_err(|error| invalid_handoff(format!("invalid changed path: {error}")))?;
        }

        let record = HandoffRecord {
            id: EntityId::new(),
            project_id: source_task.project_id.clone(),
            source_task_id: source_task.id,
            source_run_id: draft.source_run_id,
            destination_task_ids: deduplicate_ids(draft.destination_task_ids),
            outcome: draft.outcome.trim().to_owned(),
            decisions: normalized_items(draft.decisions),
            evidence: normalized_items(draft.evidence),
            changed_paths: normalized_items(draft.changed_paths),
            commands: draft
                .commands
                .into_iter()
                .map(|item| HandoffCommandEvidence {
                    command: item.command.trim().to_owned(),
                    result: item.result.trim().to_owned(),
                })
                .collect(),
            risks: normalized_items(draft.risks),
            next_steps: normalized_items(draft.next_steps),
            artifact_ids: deduplicate_ids(draft.artifact_ids),
            receipt_ids: deduplicate_ids(draft.receipt_ids),
            created_at: now(),
        };
        let canonical = canonical_json(&record)?;
        if canonical.len() > MAX_HANDOFF_BYTES {
            return Err(invalid_handoff(format!(
                "canonical handoff exceeds {MAX_HANDOFF_BYTES} bytes"
            )));
        }
        let digest = hex::encode(Sha256::digest(canonical.as_bytes()));
        let project = self
            .store
            .get_project(&record.project_id)?
            .ok_or_else(|| invalid_handoff("project not found"))?;
        let (path, relative) = handoff_destination(&project.root_path, &record.id)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(canonical.as_bytes())?;
        file.sync_all()?;
        let artifact = Artifact {
            id: record.id.clone(),
            project_id: record.project_id.clone(),
            task_id: Some(record.source_task_id.clone()),
            run_id: record.source_run_id.clone(),
            kind: HANDOFF_KIND.into(),
            path: relative,
            digest: Some(digest),
            metadata: serde_json::to_value(&record)?,
            created_at: record.created_at,
        };
        if let Err(error) = self.store.upsert_artifact(&artifact) {
            if let Err(cleanup_error) = fs::remove_file(&path) {
                tracing::warn!(path = %path.display(), %cleanup_error, "unable to roll back handoff file");
            }
            return Err(error.into());
        }
        Ok(HandoffArtifact {
            handoff: record,
            artifact,
        })
    }

    /// Read and verify one handoff by its shared Handoff/Artifact identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the repository file or persisted metadata fails verification.
    pub fn get(&self, id: &EntityId) -> Result<Option<HandoffArtifact>, SchedulerError> {
        let Some(artifact) = self.store.get_artifact(id)? else {
            return Ok(None);
        };
        if artifact.kind != HANDOFF_KIND {
            return Ok(None);
        }
        Ok(Some(self.read_artifact(artifact)?))
    }

    /// List verified handoffs received by a task.
    ///
    /// # Errors
    ///
    /// Returns an error when the task is missing or a matching handoff fails verification.
    pub fn list_for_task(
        &self,
        task_id: &EntityId,
    ) -> Result<Vec<HandoffArtifact>, SchedulerError> {
        let task = self
            .store
            .get_task(task_id)?
            .ok_or_else(|| invalid_handoff("task not found"))?;
        let mut records = self
            .store
            .list_artifacts(&task.project_id)?
            .into_iter()
            .filter(|artifact| artifact.kind == HANDOFF_KIND)
            .map(|artifact| self.read_artifact(artifact))
            .collect::<Result<Vec<_>, _>>()?;
        records.retain(|item| item.handoff.destination_task_ids.contains(task_id));
        records.sort_by_key(|item| item.handoff.created_at);
        Ok(records)
    }

    /// List verified handoffs created by a task.
    ///
    /// # Errors
    ///
    /// Returns an error when the task is missing or a matching handoff fails verification.
    pub fn list_from_task(
        &self,
        task_id: &EntityId,
    ) -> Result<Vec<HandoffArtifact>, SchedulerError> {
        let task = self
            .store
            .get_task(task_id)?
            .ok_or_else(|| invalid_handoff("task not found"))?;
        let mut records = self
            .store
            .list_artifacts(&task.project_id)?
            .into_iter()
            .filter(|artifact| {
                artifact.kind == HANDOFF_KIND && artifact.task_id.as_ref() == Some(task_id)
            })
            .map(|artifact| self.read_artifact(artifact))
            .collect::<Result<Vec<_>, _>>()?;
        records.sort_by_key(|item| item.handoff.created_at);
        Ok(records)
    }

    fn read_artifact(&self, artifact: Artifact) -> Result<HandoffArtifact, SchedulerError> {
        let project = self
            .store
            .get_project(&artifact.project_id)?
            .ok_or_else(|| invalid_handoff("handoff project not found"))?;
        let root = PathBuf::from(project.root_path).canonicalize()?;
        let relative = RepoPath::new(&artifact.path)
            .map_err(|error| invalid_handoff(format!("unsafe artifact path: {error}")))?;
        let path = root
            .join(relative.as_path())
            .canonicalize()
            .map_err(|error| {
                SchedulerError::HandoffIntegrity(format!(
                    "handoff artifact {} is unavailable: {error}",
                    artifact.id
                ))
            })?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err(SchedulerError::HandoffIntegrity(
                "handoff artifact resolves outside the repository".into(),
            ));
        }
        let contents = fs::read_to_string(path).map_err(|error| {
            SchedulerError::HandoffIntegrity(format!(
                "handoff artifact {} cannot be read: {error}",
                artifact.id
            ))
        })?;
        let handoff: HandoffRecord = serde_json::from_str(&contents).map_err(|error| {
            SchedulerError::HandoffIntegrity(format!(
                "handoff artifact {} is not valid structured JSON: {error}",
                artifact.id
            ))
        })?;
        let canonical = canonical_json(&handoff)?;
        let actual = hex::encode(Sha256::digest(canonical.as_bytes()));
        if artifact.id != handoff.id
            || artifact.digest.as_deref() != Some(actual.as_str())
            || artifact.project_id != handoff.project_id
            || artifact.task_id.as_ref() != Some(&handoff.source_task_id)
            || artifact.run_id != handoff.source_run_id
            || artifact.metadata != serde_json::to_value(&handoff)?
        {
            return Err(SchedulerError::HandoffIntegrity(format!(
                "handoff artifact {} does not match its canonical record",
                artifact.id
            )));
        }
        Ok(HandoffArtifact { handoff, artifact })
    }
}

fn validate_draft_shape(draft: &HandoffDraft) -> Result<(), SchedulerError> {
    if draft.outcome.trim().is_empty()
        || draft.outcome.chars().count() > MAX_OUTCOME_CHARS
        || draft.destination_task_ids.is_empty()
        || draft.destination_task_ids.len() > MAX_LIST_ITEMS
        || draft.artifact_ids.len() > MAX_LIST_ITEMS
        || draft.receipt_ids.len() > MAX_LIST_ITEMS
    {
        return Err(invalid_handoff(
            "outcome and at least one destination task are required",
        ));
    }
    for items in [
        &draft.decisions,
        &draft.evidence,
        &draft.changed_paths,
        &draft.risks,
        &draft.next_steps,
    ] {
        validate_items(items)?;
    }
    if draft.commands.len() > MAX_LIST_ITEMS
        || draft.commands.iter().any(|item| {
            item.command.trim().is_empty()
                || item.command.chars().count() > MAX_ITEM_CHARS
                || item.result.chars().count() > MAX_ITEM_CHARS
        })
    {
        return Err(invalid_handoff("command evidence is empty or too large"));
    }
    Ok(())
}

fn validate_items(items: &[String]) -> Result<(), SchedulerError> {
    if items.len() > MAX_LIST_ITEMS
        || items
            .iter()
            .any(|item| item.trim().is_empty() || item.chars().count() > MAX_ITEM_CHARS)
    {
        return Err(invalid_handoff("handoff list item is empty or too large"));
    }
    Ok(())
}

fn validate_evidence_links(
    store: &SqliteStore,
    project_id: &EntityId,
    draft: &HandoffDraft,
) -> Result<(), SchedulerError> {
    for id in &draft.artifact_ids {
        let artifact = store
            .get_artifact(id)?
            .ok_or_else(|| invalid_handoff(format!("artifact {id} not found")))?;
        if artifact.project_id != *project_id {
            return Err(invalid_handoff("artifact belongs to a different project"));
        }
    }
    for id in &draft.receipt_ids {
        let receipt = store
            .get_receipt(id)?
            .ok_or_else(|| invalid_handoff(format!("receipt {id} not found")))?;
        if receipt.project_id != *project_id {
            return Err(invalid_handoff("receipt belongs to a different project"));
        }
    }
    Ok(())
}

fn handoff_destination(
    project_root: &str,
    id: &EntityId,
) -> Result<(PathBuf, String), SchedulerError> {
    let root = PathBuf::from(project_root).canonicalize()?;
    let directory = root.join(".ba-agent/knowledge/handoffs");
    fs::create_dir_all(&directory)?;
    let directory = directory.canonicalize()?;
    if !directory.starts_with(&root) {
        return Err(invalid_handoff(
            "handoff directory resolves outside the repository",
        ));
    }
    let relative = format!(".ba-agent/knowledge/handoffs/{id}.json");
    Ok((directory.join(format!("{id}.json")), relative))
}

fn normalized_items(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .map(|item| item.trim().to_owned())
        .collect()
}

fn deduplicate_ids(ids: Vec<EntityId>) -> Vec<EntityId> {
    ids.into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn invalid_handoff(message: impl Into<String>) -> SchedulerError {
    SchedulerError::InvalidHandoff(message.into())
}

pub(crate) fn handoff_prompt_section(handoffs: &[HandoffArtifact]) -> Option<String> {
    if handoffs.is_empty() {
        return None;
    }
    let entries = handoffs
        .iter()
        .map(|item| {
            let record = &item.handoff;
            format!(
                "交接 {}（来源任务 {}）：\n结果：{}\n决策：{}\n证据：{}\n风险：{}\n下一步：{}",
                record.id,
                record.source_task_id,
                record.outcome,
                list_or_none(&record.decisions),
                list_or_none(&record.evidence),
                list_or_none(&record.risks),
                list_or_none(&record.next_steps),
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(format!(
        "以下是已校验且持久化的上游结构化交接。先核对这些证据，不要假设聊天中未记录的状态：\n\n{entries}"
    ))
}

fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "无".into()
    } else {
        items.join("；")
    }
}
