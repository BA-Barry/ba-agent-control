use std::collections::{BTreeMap, BTreeSet};

use ba_core::{
    AgentProfile, EntityId, Project, ReviewRepairDecision, ReviewRepairLoop,
    ReviewRepairLoopStatus, Task, TaskStatus, Workstream, now,
};
use ba_git::RepoPath;
use ba_store::SqliteStore;

use crate::SchedulerError;

const MAX_REPAIR_LIMIT: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewRepairAction {
    Accept,
    Repair,
    Block,
}

#[derive(Clone, Debug)]
pub struct ReviewRepairLoopStartRequest {
    pub project_id: EntityId,
    pub title: String,
    pub objective: String,
    pub write_scope: Vec<String>,
    pub max_repairs: u8,
}

#[derive(Clone, Debug)]
pub struct ReviewRepairLoopDecisionRequest {
    pub action: ReviewRepairAction,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct ReviewRepairLoopService {
    store: SqliteStore,
}

impl ReviewRepairLoopService {
    #[must_use]
    pub const fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Start a bounded implementation, test, and review sequence.
    ///
    /// # Errors
    ///
    /// Returns an error when the project, write scope, role profiles, or durable
    /// transaction is invalid.
    pub fn start(
        &self,
        request: &ReviewRepairLoopStartRequest,
    ) -> Result<ReviewRepairLoop, SchedulerError> {
        let project = self.project(&request.project_id)?;
        let title = required_text(&request.title, "loop title")?;
        let objective = required_objective(&request.objective)?;
        let write_scope = validated_scope(&request.write_scope)?;
        if write_scope.is_empty() {
            return Err(SchedulerError::InvalidReviewRepairLoop(
                "a review/repair loop requires at least one write scope".into(),
            ));
        }
        if request.max_repairs == 0 || request.max_repairs > MAX_REPAIR_LIMIT {
            return Err(SchedulerError::InvalidReviewRepairLoop(format!(
                "max repairs must be between 1 and {MAX_REPAIR_LIMIT}"
            )));
        }
        let profile_records = self.store.list_agent_profiles()?;
        let profiles = profiles_by_role(&profile_records);
        let implementer = writable_profile(&profiles, "implementer")?;
        let tester = writable_profile(&profiles, "test_engineer")?;
        let reviewer = profile(&profiles, "reviewer")?;
        let stamp = now();
        let workstream = Workstream {
            id: EntityId::new(),
            project_id: project.id.clone(),
            name: title.clone(),
            description: objective.clone(),
            status: "active".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        let implement_id = EntityId::new();
        let test_id = EntityId::new();
        let review_id = EntityId::new();
        let english = project.language == "en";
        let tasks = vec![
            task(
                &project,
                &workstream,
                implement_id.clone(),
                "实现已批准变更",
                "Implement the approved change",
                implementation_description(&objective, &test_id, english),
                true,
                write_scope.clone(),
                implementer,
                Vec::new(),
                stamp,
            ),
            task(
                &project,
                &workstream,
                test_id.clone(),
                "验证行为与失败路径",
                "Verify behavior and failure paths",
                testing_description(&objective, &review_id, english),
                true,
                write_scope.clone(),
                tester,
                vec![implement_id],
                stamp,
            ),
            task(
                &project,
                &workstream,
                review_id.clone(),
                "独立审查并等待修复决定",
                "Independently review and await repair decision",
                review_description(&objective, english),
                false,
                Vec::new(),
                reviewer,
                vec![test_id],
                stamp,
            ),
        ];
        let loop_record = ReviewRepairLoop {
            id: EntityId::new(),
            project_id: project.id,
            workstream_id: workstream.id.clone(),
            title,
            objective,
            write_scope,
            max_repairs: request.max_repairs,
            repairs_started: 0,
            status: ReviewRepairLoopStatus::Running,
            active_review_task_id: review_id,
            confirmation_task_id: None,
            decisions: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        self.store
            .create_review_repair_loop(&loop_record, &workstream, &tasks)?;
        Ok(loop_record)
    }

    /// List loops after synchronizing status from their durable task state.
    ///
    /// # Errors
    ///
    /// Returns an error when a stored loop has missing task links or persistence fails.
    pub fn list(&self, project_id: &EntityId) -> Result<Vec<ReviewRepairLoop>, SchedulerError> {
        self.project(project_id)?;
        self.store
            .list_review_repair_loops(project_id)?
            .into_iter()
            .map(|loop_record| self.refresh(loop_record))
            .collect()
    }

    /// Read one loop after synchronizing status from its durable task state.
    ///
    /// # Errors
    ///
    /// Returns an error when the loop is missing, has invalid task links, or persistence fails.
    pub fn get(&self, id: &EntityId) -> Result<ReviewRepairLoop, SchedulerError> {
        let loop_record = self
            .store
            .get_review_repair_loop(id)?
            .ok_or_else(|| SchedulerError::ReviewRepairLoopNotFound(id.to_string()))?;
        self.refresh(loop_record)
    }

    /// Persist an explicit local-user review decision and generate only the next bounded stage.
    ///
    /// # Errors
    ///
    /// Returns an error when the review is not complete, the loop is not awaiting a
    /// decision, or generated tasks cannot be persisted atomically.
    pub fn decide(
        &self,
        id: &EntityId,
        request: &ReviewRepairLoopDecisionRequest,
    ) -> Result<ReviewRepairLoop, SchedulerError> {
        let loop_record = self
            .store
            .get_review_repair_loop(id)?
            .ok_or_else(|| SchedulerError::ReviewRepairLoopNotFound(id.to_string()))?;
        self.decide_review(loop_record, request)
    }

    fn decide_review(
        &self,
        loop_record: ReviewRepairLoop,
        request: &ReviewRepairLoopDecisionRequest,
    ) -> Result<ReviewRepairLoop, SchedulerError> {
        let review_task_id = loop_record.active_review_task_id.clone();
        let mut loop_record = self.refresh(loop_record)?;
        // Refresh may reread after a race, but this decision belongs only to the
        // review observed when the request began.
        if loop_record.active_review_task_id != review_task_id {
            return Err(ba_store::StoreError::ConcurrentReviewRepairLoop(loop_record.id).into());
        }
        if loop_record.status != ReviewRepairLoopStatus::AwaitingDecision {
            return Err(SchedulerError::InvalidReviewRepairLoop(format!(
                "loop {} is {}, not awaiting a review decision",
                loop_record.id,
                loop_record.status.as_str()
            )));
        }
        let expected = loop_record.clone();
        let note = required_decision_note(&request.note)?;
        let stamp = now();
        let decision = match request.action {
            ReviewRepairAction::Accept => "accept",
            ReviewRepairAction::Repair => "repair",
            ReviewRepairAction::Block => "block",
        };
        loop_record.decisions.push(ReviewRepairDecision {
            decision: decision.into(),
            review_task_id: loop_record.active_review_task_id.clone(),
            note: note.clone(),
            decided_at: stamp,
        });
        loop_record.updated_at = stamp;

        let tasks = match request.action {
            ReviewRepairAction::Accept => self.accept_review(&mut loop_record, &note, stamp)?,
            ReviewRepairAction::Repair => self.request_repair(&mut loop_record, &note, stamp)?,
            ReviewRepairAction::Block => {
                loop_record.status = ReviewRepairLoopStatus::Blocked;
                Vec::new()
            }
        };
        // Commit this decision only for the observed review. Never retry a
        // conflict: a later round needs a new explicit user decision.
        self.store
            .advance_review_repair_loop(&expected, &loop_record, &tasks)?;
        Ok(loop_record)
    }

    fn accept_review(
        &self,
        loop_record: &mut ReviewRepairLoop,
        note: &str,
        stamp: ba_core::Timestamp,
    ) -> Result<Vec<Task>, SchedulerError> {
        let project = self.project(&loop_record.project_id)?;
        let profile_records = self.store.list_agent_profiles()?;
        let profiles = profiles_by_role(&profile_records);
        let reviewer = profile(&profiles, "reviewer")?;
        let confirmation = task(
            &project,
            &self.workstream(&loop_record.workstream_id)?,
            EntityId::new(),
            "确认审查结论与交付边界",
            "Confirm the review conclusion and delivery boundary",
            confirmation_description(&loop_record.objective, note, project.language == "en"),
            false,
            Vec::new(),
            reviewer,
            vec![loop_record.active_review_task_id.clone()],
            stamp,
        );
        loop_record.status = ReviewRepairLoopStatus::Confirming;
        loop_record.confirmation_task_id = Some(confirmation.id.clone());
        Ok(vec![confirmation])
    }

    fn request_repair(
        &self,
        loop_record: &mut ReviewRepairLoop,
        note: &str,
        stamp: ba_core::Timestamp,
    ) -> Result<Vec<Task>, SchedulerError> {
        if loop_record.repairs_started >= loop_record.max_repairs {
            loop_record.status = ReviewRepairLoopStatus::Exhausted;
            return Ok(Vec::new());
        }
        let project = self.project(&loop_record.project_id)?;
        let workstream = self.workstream(&loop_record.workstream_id)?;
        let profile_records = self.store.list_agent_profiles()?;
        let profiles = profiles_by_role(&profile_records);
        let implementer = writable_profile(&profiles, "implementer")?;
        let tester = writable_profile(&profiles, "test_engineer")?;
        let reviewer = profile(&profiles, "reviewer")?;
        let iteration = loop_record.repairs_started + 1;
        let repair_id = EntityId::new();
        let test_id = EntityId::new();
        let review_id = EntityId::new();
        let english = project.language == "en";
        let tasks = vec![
            task(
                &project,
                &workstream,
                repair_id.clone(),
                &format!("修复第 {iteration} 轮审查发现"),
                &format!("Repair review findings, iteration {iteration}"),
                repair_description(&loop_record.objective, note, &test_id, english),
                true,
                loop_record.write_scope.clone(),
                implementer,
                vec![loop_record.active_review_task_id.clone()],
                stamp,
            ),
            task(
                &project,
                &workstream,
                test_id.clone(),
                &format!("验证第 {iteration} 轮修复"),
                &format!("Verify repair iteration {iteration}"),
                testing_description(&loop_record.objective, &review_id, english),
                true,
                loop_record.write_scope.clone(),
                tester,
                vec![repair_id],
                stamp,
            ),
            task(
                &project,
                &workstream,
                review_id.clone(),
                &format!("复审第 {iteration} 轮修复"),
                &format!("Re-review repair iteration {iteration}"),
                review_description(&loop_record.objective, english),
                false,
                Vec::new(),
                reviewer,
                vec![test_id],
                stamp,
            ),
        ];
        loop_record.repairs_started = iteration;
        loop_record.status = ReviewRepairLoopStatus::Running;
        loop_record.active_review_task_id = review_id;
        Ok(tasks)
    }

    fn refresh(
        &self,
        mut loop_record: ReviewRepairLoop,
    ) -> Result<ReviewRepairLoop, SchedulerError> {
        let next_status = match loop_record.status {
            ReviewRepairLoopStatus::Running => {
                let [writer, test, review] = self.active_stage(&loop_record)?;
                if [writer.status, test.status, review.status]
                    .iter()
                    .any(|status| {
                        matches!(
                            status,
                            TaskStatus::Blocked | TaskStatus::Failed | TaskStatus::Cancelled
                        )
                    })
                {
                    Some(ReviewRepairLoopStatus::Blocked)
                } else if review.status == TaskStatus::Completed {
                    Some(ReviewRepairLoopStatus::AwaitingDecision)
                } else {
                    None
                }
            }
            ReviewRepairLoopStatus::Confirming => {
                let confirmation = loop_record.confirmation_task_id.as_ref().ok_or_else(|| {
                    SchedulerError::InvalidReviewRepairLoop(
                        "confirming loop has no confirmation task".into(),
                    )
                })?;
                match self.task(confirmation)?.status {
                    TaskStatus::Completed => Some(ReviewRepairLoopStatus::Completed),
                    TaskStatus::Blocked | TaskStatus::Failed | TaskStatus::Cancelled => {
                        Some(ReviewRepairLoopStatus::Blocked)
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(status) = next_status {
            let expected = loop_record.clone();
            loop_record.status = status;
            loop_record.updated_at = now();
            match self
                .store
                .advance_review_repair_loop(&expected, &loop_record, &[])
            {
                Ok(()) => {}
                Err(ba_store::StoreError::ConcurrentReviewRepairLoop(_)) => {
                    return self.get(&loop_record.id);
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(loop_record)
    }

    fn active_stage(&self, loop_record: &ReviewRepairLoop) -> Result<[Task; 3], SchedulerError> {
        let review = self.task(&loop_record.active_review_task_id)?;
        let test_id = single_dependency(&review, "review")?;
        let test = self.task(test_id)?;
        let writer_id = single_dependency(&test, "test")?;
        let writer = self.task(writer_id)?;
        Ok([writer, test, review])
    }

    fn project(&self, id: &EntityId) -> Result<Project, SchedulerError> {
        self.store.get_project(id)?.ok_or_else(|| {
            SchedulerError::InvalidReviewRepairLoop(format!("project {id} not found"))
        })
    }

    fn workstream(&self, id: &EntityId) -> Result<Workstream, SchedulerError> {
        self.store.get_workstream(id)?.ok_or_else(|| {
            SchedulerError::InvalidReviewRepairLoop(format!("workstream {id} not found"))
        })
    }

    fn task(&self, id: &EntityId) -> Result<Task, SchedulerError> {
        self.store
            .get_task(id)?
            .ok_or_else(|| SchedulerError::InvalidReviewRepairLoop(format!("task {id} not found")))
    }
}

fn required_text(value: &str, label: &str) -> Result<String, SchedulerError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 240 {
        return Err(SchedulerError::InvalidReviewRepairLoop(format!(
            "{label} must be between 1 and 240 characters"
        )));
    }
    Ok(value.to_owned())
}

fn required_objective(value: &str) -> Result<String, SchedulerError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 4_000 {
        return Err(SchedulerError::InvalidReviewRepairLoop(
            "loop objective must be between 1 and 4000 characters".into(),
        ));
    }
    Ok(value.to_owned())
}

fn required_decision_note(value: &str) -> Result<String, SchedulerError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 4_000 {
        return Err(SchedulerError::InvalidReviewRepairLoop(
            "decision evidence must be between 1 and 4000 characters".into(),
        ));
    }
    Ok(value.to_owned())
}

fn validated_scope(scope: &[String]) -> Result<Vec<String>, SchedulerError> {
    let mut normalized = BTreeSet::new();
    for path in scope {
        normalized.insert(
            RepoPath::new(path)
                .map_err(|error| SchedulerError::InvalidReviewRepairLoop(error.to_string()))?
                .as_path()
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    Ok(normalized.into_iter().collect())
}

fn single_dependency<'a>(task: &'a Task, stage: &str) -> Result<&'a EntityId, SchedulerError> {
    let [dependency] = task.depends_on.as_slice() else {
        return Err(SchedulerError::InvalidReviewRepairLoop(format!(
            "{stage} task {} must have exactly one stage dependency",
            task.id
        )));
    };
    Ok(dependency)
}

fn profiles_by_role(profiles: &[AgentProfile]) -> BTreeMap<&str, &AgentProfile> {
    profiles
        .iter()
        .map(|profile| (profile.role.as_str(), profile))
        .collect()
}

fn profile<'a>(
    profiles: &'a BTreeMap<&str, &'a AgentProfile>,
    role: &str,
) -> Result<&'a AgentProfile, SchedulerError> {
    profiles.get(role).copied().ok_or_else(|| {
        SchedulerError::InvalidReviewRepairLoop(format!("missing durable Agent profile for {role}"))
    })
}

fn writable_profile<'a>(
    profiles: &'a BTreeMap<&str, &'a AgentProfile>,
    role: &str,
) -> Result<&'a AgentProfile, SchedulerError> {
    let profile = profile(profiles, role)?;
    if profile.config["sandboxMode"] != "workspace-write" {
        return Err(SchedulerError::InvalidReviewRepairLoop(format!(
            "Agent profile {role} is not write-capable"
        )));
    }
    Ok(profile)
}

#[allow(
    clippy::too_many_arguments,
    reason = "task creation keeps loop links explicit"
)]
fn task(
    project: &Project,
    workstream: &Workstream,
    id: EntityId,
    title_zh: &str,
    title_en: &str,
    description: String,
    write: bool,
    path_scope: Vec<String>,
    profile: &AgentProfile,
    depends_on: Vec<EntityId>,
    stamp: ba_core::Timestamp,
) -> Task {
    Task {
        id,
        project_id: project.id.clone(),
        workstream_id: Some(workstream.id.clone()),
        milestone_id: None,
        title: if project.language == "en" {
            title_en.into()
        } else {
            title_zh.into()
        },
        description,
        status: TaskStatus::Ready,
        write,
        path_scope,
        depends_on,
        agent_profile_id: Some(profile.id.clone()),
        created_at: stamp,
        updated_at: stamp,
    }
}

fn implementation_description(objective: &str, downstream: &EntityId, english: bool) -> String {
    if english {
        format!(
            "Objective: {objective}\n\nWhen complete, submit a structured BA Agent handoff to downstream task {downstream}."
        )
    } else {
        format!(
            "实现目标：{objective}\n\n完成后通过 BA Agent 的 handoff.create 提交结构化交接，目标任务：{downstream}。"
        )
    }
}

fn testing_description(objective: &str, downstream: &EntityId, english: bool) -> String {
    if english {
        format!(
            "Verification objective: {objective}\nCover the main success and failure paths and record exact commands and results.\n\nWhen complete, submit a structured BA Agent handoff to downstream task {downstream}."
        )
    } else {
        format!(
            "验证目标：{objective}\n覆盖主要成功与失败路径，并记录精确命令与结果。\n\n完成后通过 BA Agent 的 handoff.create 提交结构化交接，目标任务：{downstream}。"
        )
    }
}

fn review_description(objective: &str, english: bool) -> String {
    if english {
        format!(
            "Independent review objective: {objective}\nReport reproducible findings and severity. Do not modify files or create repair tasks; wait for the user to accept, repair, or block in BA Agent."
        )
    } else {
        format!(
            "独立审查目标：{objective}\n报告可复现发现及其严重度。不要修改文件，也不要自行创建修复任务；等待用户在 BA Agent 中决定通过、修复或阻塞。"
        )
    }
}

fn repair_description(objective: &str, note: &str, downstream: &EntityId, english: bool) -> String {
    if english {
        format!(
            "Repair objective: {objective}\n\nReview decision evidence: {note}\n\nWhen complete, submit a structured BA Agent handoff to downstream task {downstream}."
        )
    } else {
        format!(
            "修复目标：{objective}\n\n审查决定与说明：{note}\n\n完成后通过 BA Agent 的 handoff.create 提交结构化交接，目标任务：{downstream}。"
        )
    }
}

fn confirmation_description(objective: &str, note: &str, english: bool) -> String {
    if english {
        format!(
            "Confirmation objective: {objective}\n\nUser acceptance evidence: {note}\nVerify the review conclusion, quality evidence, and declared scope. Do not create another repair."
        )
    } else {
        format!(
            "确认目标：{objective}\n\n用户接受说明：{note}\n核对审查结论、质量证据与声明范围；无需创建后续修复。"
        )
    }
}

#[cfg(test)]
#[path = "review_repair_tests.rs"]
mod concurrency_tests;

#[cfg(test)]
mod tests {
    use ba_core::{Project, TaskStatus, now};
    use ba_store::SqliteStore;

    use super::{
        ReviewRepairAction, ReviewRepairLoopDecisionRequest, ReviewRepairLoopService,
        ReviewRepairLoopStartRequest,
    };
    use crate::ensure_builtin_agent_profiles;

    pub(super) fn complete_task(
        store: &SqliteStore,
        id: &ba_core::EntityId,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for status in [
            TaskStatus::Queued,
            TaskStatus::Running,
            TaskStatus::Review,
            TaskStatus::Completed,
        ] {
            store.transition_task(id, status)?;
        }
        Ok(())
    }

    pub(super) fn fixture()
    -> Result<(tempfile::TempDir, SqliteStore, Project), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = SqliteStore::open(directory.path().join("state.sqlite3"))?;
        ensure_builtin_agent_profiles(&store)?;
        let stamp = now();
        let project = Project {
            id: ba_core::EntityId::new(),
            name: "loop fixture".into(),
            root_path: directory.path().to_string_lossy().into_owned(),
            description: String::new(),
            language: "zh-CN".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        Ok((directory, store, project))
    }

    #[test]
    fn bounded_repair_loop_preserves_review_decisions_and_stops_at_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_directory, store, project) = fixture()?;
        let service = ReviewRepairLoopService::new(store.clone());
        let loop_record = service.start(&ReviewRepairLoopStartRequest {
            project_id: project.id.clone(),
            title: "bounded repair".into(),
            objective: "repair the observed behavior".into(),
            write_scope: vec!["src".into()],
            max_repairs: 1,
        })?;
        assert_eq!(store.list_tasks(&project.id)?.len(), 3);

        complete_task(&store, &loop_record.active_review_task_id)?;
        let awaiting = service.get(&loop_record.id)?;
        assert_eq!(awaiting.status.as_str(), "awaiting_decision");

        let repairing = service.decide(
            &loop_record.id,
            &ReviewRepairLoopDecisionRequest {
                action: ReviewRepairAction::Repair,
                note: "one reproducible defect remains".into(),
            },
        )?;
        assert_eq!(repairing.repairs_started, 1);
        assert_eq!(repairing.status.as_str(), "running");
        assert_eq!(store.list_tasks(&project.id)?.len(), 6);

        complete_task(&store, &repairing.active_review_task_id)?;
        let exhausted = service.decide(
            &loop_record.id,
            &ReviewRepairLoopDecisionRequest {
                action: ReviewRepairAction::Repair,
                note: "a second issue remains".into(),
            },
        )?;
        assert_eq!(exhausted.status.as_str(), "exhausted");
        assert_eq!(exhausted.decisions.len(), 2);
        assert_eq!(store.list_tasks(&project.id)?.len(), 6);
        Ok(())
    }

    #[test]
    fn accepted_review_requires_a_separate_confirmation_task()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_directory, store, project) = fixture()?;
        let service = ReviewRepairLoopService::new(store.clone());
        let loop_record = service.start(&ReviewRepairLoopStartRequest {
            project_id: project.id,
            title: "accepted review".into(),
            objective: "verify the finished change".into(),
            write_scope: vec!["src".into()],
            max_repairs: 2,
        })?;
        complete_task(&store, &loop_record.active_review_task_id)?;
        let confirming = service.decide(
            &loop_record.id,
            &ReviewRepairLoopDecisionRequest {
                action: ReviewRepairAction::Accept,
                note: "findings accepted".into(),
            },
        )?;
        let confirmation = confirming
            .confirmation_task_id
            .as_ref()
            .ok_or_else(|| std::io::Error::other("confirmation task missing"))?;
        assert_eq!(confirming.status.as_str(), "confirming");
        complete_task(&store, confirmation)?;
        assert_eq!(service.get(&loop_record.id)?.status.as_str(), "completed");
        Ok(())
    }

    #[test]
    fn stage_failure_blocks_the_loop_and_blank_evidence_is_rejected()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_directory, store, project) = fixture()?;
        let service = ReviewRepairLoopService::new(store.clone());
        let invalid = service.start(&ReviewRepairLoopStartRequest {
            project_id: project.id.clone(),
            title: "missing objective".into(),
            objective: "  ".into(),
            write_scope: vec!["src".into()],
            max_repairs: 1,
        });
        assert!(invalid.is_err());

        let loop_record = service.start(&ReviewRepairLoopStartRequest {
            project_id: project.id,
            title: "failed verification".into(),
            objective: "stop when the test stage fails".into(),
            write_scope: vec!["src".into()],
            max_repairs: 1,
        })?;
        let review = store
            .get_task(&loop_record.active_review_task_id)?
            .ok_or_else(|| std::io::Error::other("review task missing"))?;
        let test_id = review
            .depends_on
            .first()
            .ok_or_else(|| std::io::Error::other("test task missing"))?;
        for status in [TaskStatus::Queued, TaskStatus::Running, TaskStatus::Failed] {
            store.transition_task(test_id, status)?;
        }
        assert_eq!(service.get(&loop_record.id)?.status.as_str(), "blocked");

        let decision_loop = service.start(&ReviewRepairLoopStartRequest {
            project_id: loop_record.project_id,
            title: "blank decision evidence".into(),
            objective: "require a durable decision rationale".into(),
            write_scope: vec!["src".into()],
            max_repairs: 1,
        })?;
        complete_task(&store, &decision_loop.active_review_task_id)?;
        assert!(
            service
                .decide(
                    &decision_loop.id,
                    &ReviewRepairLoopDecisionRequest {
                        action: ReviewRepairAction::Block,
                        note: " ".into(),
                    },
                )
                .is_err()
        );
        Ok(())
    }
}
