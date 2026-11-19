use std::collections::{BTreeMap, BTreeSet};

use ba_core::{EntityId, Task, TaskStatus};
use ba_git::{RepoPath, WriteScope};
use serde::{Deserialize, Serialize};

use crate::SchedulerError;

pub const DEFAULT_MAX_CONCURRENCY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub max_concurrency: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
        }
    }
}

impl SchedulerConfig {
    /// Validate scheduler limits.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::InvalidConcurrency`] for a zero limit.
    pub fn validate(self) -> Result<Self, SchedulerError> {
        if self.max_concurrency == 0 {
            Err(SchedulerError::InvalidConcurrency)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleBlocker {
    NotReady {
        status: TaskStatus,
    },
    MissingDependency {
        task_id: EntityId,
    },
    DependencyIncomplete {
        task_id: EntityId,
        status: TaskStatus,
    },
    DependencyCycle,
    EmptyWriteScope,
    InvalidPathScope {
        path: String,
        reason: String,
    },
    ConcurrencyLimit {
        limit: usize,
    },
    WriteConflict {
        running_task_id: EntityId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DispatchPlan {
    pub accepted: Vec<EntityId>,
    pub blocked: BTreeMap<EntityId, Vec<ScheduleBlocker>>,
}

/// Validate the parts of a queued task that cannot become true later.
#[must_use]
pub fn queue_blockers_for(task: &Task, tasks: &[Task]) -> Vec<ScheduleBlocker> {
    let tasks_by_id = tasks
        .iter()
        .map(|candidate| (candidate.id.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut blockers = Vec::new();
    if task.status != TaskStatus::Ready {
        blockers.push(ScheduleBlocker::NotReady {
            status: task.status,
        });
        return blockers;
    }
    if dependency_cycle(task, &tasks_by_id) {
        blockers.push(ScheduleBlocker::DependencyCycle);
    }
    for dependency_id in &task.depends_on {
        if !tasks_by_id.contains_key(dependency_id) {
            blockers.push(ScheduleBlocker::MissingDependency {
                task_id: dependency_id.clone(),
            });
        }
    }
    if let Err(blocker) = Reservation::from_task(task) {
        blockers.push(blocker);
    }
    blockers
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reservation {
    pub task_id: EntityId,
    pub project_id: EntityId,
    pub write_scope: Option<WriteScope>,
}

impl Reservation {
    fn from_task(task: &Task) -> Result<Self, ScheduleBlocker> {
        let write_scope = if task.write {
            if task.path_scope.is_empty() {
                return Err(ScheduleBlocker::EmptyWriteScope);
            }
            let paths = task
                .path_scope
                .iter()
                .map(|path| {
                    RepoPath::new(path).map_err(|error| ScheduleBlocker::InvalidPathScope {
                        path: path.clone(),
                        reason: error.to_string(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Some(
                WriteScope::new(paths).map_err(|error| ScheduleBlocker::InvalidPathScope {
                    path: task.path_scope.join(", "),
                    reason: error.to_string(),
                })?,
            )
        } else {
            None
        };
        Ok(Self {
            task_id: task.id.clone(),
            project_id: task.project_id.clone(),
            write_scope,
        })
    }

    fn conflicts_with(&self, other: &Self) -> bool {
        self.project_id == other.project_id
            && self
                .write_scope
                .as_ref()
                .zip(other.write_scope.as_ref())
                .is_some_and(|(left, right)| left.overlaps(right))
    }
}

#[derive(Debug)]
pub struct Scheduler {
    config: SchedulerConfig,
    active: BTreeMap<EntityId, Reservation>,
}

impl Scheduler {
    /// Create an empty scheduler with validated policy.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::InvalidConcurrency`] for a zero limit.
    pub fn new(config: SchedulerConfig) -> Result<Self, SchedulerError> {
        Ok(Self {
            config: config.validate()?,
            active: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn active(&self) -> &BTreeMap<EntityId, Reservation> {
        &self.active
    }

    /// Produce a deterministic dispatch plan without mutating scheduler state.
    #[must_use]
    pub fn plan(&self, tasks: &[Task]) -> DispatchPlan {
        let tasks_by_id = tasks
            .iter()
            .map(|task| (task.id.clone(), task))
            .collect::<BTreeMap<_, _>>();
        let mut ordered = tasks.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut reservations = self.active.clone();
        let mut accepted = Vec::new();
        let mut blocked = BTreeMap::new();

        for task in ordered {
            let blockers = blockers_for(task, &tasks_by_id, &reservations, self.config);
            if blockers.is_empty() {
                if let Ok(reservation) = Reservation::from_task(task) {
                    reservations.insert(task.id.clone(), reservation);
                    accepted.push(task.id.clone());
                }
            } else if task.status == TaskStatus::Ready {
                blocked.insert(task.id.clone(), blockers);
            }
        }
        DispatchPlan { accepted, blocked }
    }

    /// Atomically validate and reserve capacity for a ready task.
    ///
    /// # Errors
    ///
    /// Returns an error when the task is already active or any dependency, scope,
    /// capacity, or path-conflict policy blocks admission.
    pub fn admit(&mut self, task: &Task, tasks: &[Task]) -> Result<Reservation, SchedulerError> {
        if self.active.contains_key(&task.id) {
            return Err(SchedulerError::AlreadyActive(task.id.to_string()));
        }
        let tasks_by_id = tasks
            .iter()
            .map(|candidate| (candidate.id.clone(), candidate))
            .collect::<BTreeMap<_, _>>();
        let blockers = blockers_for(task, &tasks_by_id, &self.active, self.config);
        if !blockers.is_empty() {
            return Err(SchedulerError::NotSchedulable(serde_json::to_string(
                &blockers,
            )?));
        }
        let reservation = Reservation::from_task(task)
            .map_err(|blocker| SchedulerError::NotSchedulable(format!("{blocker:?}")))?;
        self.active.insert(task.id.clone(), reservation.clone());
        Ok(reservation)
    }

    /// Restore a reconciled active run after restart. Status/dependency checks are
    /// skipped because already-running work must remain represented even if policy
    /// limits were lowered or legacy runs conflict. New admissions still observe
    /// every restored reservation and remain blocked safely.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate tasks or invalid scopes.
    pub fn restore(&mut self, task: &Task) -> Result<Reservation, SchedulerError> {
        if self.active.contains_key(&task.id) {
            return Err(SchedulerError::AlreadyActive(task.id.to_string()));
        }
        let reservation = Reservation::from_task(task)
            .map_err(|blocker| SchedulerError::NotSchedulable(format!("{blocker:?}")))?;
        self.active.insert(task.id.clone(), reservation.clone());
        Ok(reservation)
    }

    /// Release a completed, cancelled, or otherwise reconciled reservation.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::NotActive`] when no reservation exists.
    pub fn release(&mut self, task_id: &EntityId) -> Result<Reservation, SchedulerError> {
        self.active
            .remove(task_id)
            .ok_or_else(|| SchedulerError::NotActive(task_id.to_string()))
    }
}

fn blockers_for(
    task: &Task,
    tasks: &BTreeMap<EntityId, &Task>,
    reservations: &BTreeMap<EntityId, Reservation>,
    config: SchedulerConfig,
) -> Vec<ScheduleBlocker> {
    let mut blockers = Vec::new();
    if task.status != TaskStatus::Ready {
        blockers.push(ScheduleBlocker::NotReady {
            status: task.status,
        });
        return blockers;
    }
    if dependency_cycle(task, tasks) {
        blockers.push(ScheduleBlocker::DependencyCycle);
    }
    for dependency_id in &task.depends_on {
        match tasks.get(dependency_id) {
            None => blockers.push(ScheduleBlocker::MissingDependency {
                task_id: dependency_id.clone(),
            }),
            Some(dependency) if dependency.status != TaskStatus::Completed => {
                blockers.push(ScheduleBlocker::DependencyIncomplete {
                    task_id: dependency_id.clone(),
                    status: dependency.status,
                });
            }
            Some(_) => {}
        }
    }
    let reservation = match Reservation::from_task(task) {
        Ok(reservation) => Some(reservation),
        Err(blocker) => {
            blockers.push(blocker);
            None
        }
    };
    if reservations.len() >= config.max_concurrency {
        blockers.push(ScheduleBlocker::ConcurrencyLimit {
            limit: config.max_concurrency,
        });
    } else if let Some(reservation) = reservation
        && let Some(conflict) = reservations
            .values()
            .find(|running| reservation.conflicts_with(running))
    {
        blockers.push(ScheduleBlocker::WriteConflict {
            running_task_id: conflict.task_id.clone(),
        });
    }
    blockers
}

fn dependency_cycle(task: &Task, tasks: &BTreeMap<EntityId, &Task>) -> bool {
    fn visit(
        current: &EntityId,
        tasks: &BTreeMap<EntityId, &Task>,
        visiting: &mut BTreeSet<EntityId>,
        complete: &mut BTreeSet<EntityId>,
    ) -> bool {
        if complete.contains(current) {
            return false;
        }
        if !visiting.insert(current.clone()) {
            return true;
        }
        let Some(task) = tasks.get(current) else {
            visiting.remove(current);
            complete.insert(current.clone());
            return false;
        };
        if task
            .depends_on
            .iter()
            .any(|dependency| visit(dependency, tasks, visiting, complete))
        {
            return true;
        }
        visiting.remove(current);
        complete.insert(current.clone());
        false
    }

    visit(&task.id, tasks, &mut BTreeSet::new(), &mut BTreeSet::new())
}
