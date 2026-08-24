use std::collections::BTreeMap;

use ba_core::{AgentProfile, EntityId, now};
use ba_store::SqliteStore;
use serde_json::json;

use crate::SchedulerError;

const BUILTIN_PROFILES: [(&str, &str, &str, &str); 8] = [
    (
        "Coordinator",
        "coordinator",
        "read-only",
        "Keep the work graph coherent. Confirm prerequisites, assign narrow roles, and surface user decisions. Do not implement changes or bypass approval and gate policies.",
    ),
    (
        "Planner",
        "planner",
        "read-only",
        "Produce executable tasks with acceptance criteria, dependencies, declared paths, and verification. Do not edit production files or invent missing product decisions.",
    ),
    (
        "Explorer",
        "explorer",
        "read-only",
        "Trace real execution paths and return concise evidence with file references. Stay read-only and separate facts from inferences.",
    ),
    (
        "Implementer",
        "implementer",
        "workspace-write",
        "Make the smallest complete change that satisfies the task. Touch only declared paths, keep unrelated work intact, and run targeted verification. Never push, merge, deploy, or bypass gates.",
    ),
    (
        "Reviewer",
        "reviewer",
        "read-only",
        "Review like an owner. Lead with concrete findings and reproduction evidence, prioritize behavior and risk, and avoid modifying files.",
    ),
    (
        "Test Engineer",
        "test_engineer",
        "workspace-write",
        "Test observable behavior and failure paths. Keep test changes inside the declared scope, report exact commands and results, and do not weaken assertions to make a run pass.",
    ),
    (
        "Security Reviewer",
        "security_reviewer",
        "read-only",
        "Threat-model the changed paths and inspect authorization, input validation, filesystem safety, and credential handling. Report exploitable evidence first and do not edit files.",
    ),
    (
        "Docs Reviewer",
        "docs_reviewer",
        "read-only",
        "Verify every documented command and claim against the repository. Flag missing setup, recovery, and safety guidance; do not invent examples or edit code.",
    ),
];

/// Ensure every narrow built-in role has one durable profile without overwriting users.
///
/// # Errors
///
/// Returns an error when profile state cannot be read or persisted.
pub fn ensure_builtin_agent_profiles(
    store: &SqliteStore,
) -> Result<Vec<AgentProfile>, SchedulerError> {
    let mut profiles = store
        .list_agent_profiles()?
        .into_iter()
        .map(|profile| (profile.role.clone(), profile))
        .collect::<BTreeMap<_, _>>();
    for (name, role, sandbox_mode, instructions) in BUILTIN_PROFILES {
        if profiles.contains_key(role) {
            continue;
        }
        let stamp = now();
        let profile = AgentProfile {
            id: EntityId::new(),
            name: name.into(),
            role: role.into(),
            instructions: instructions.into(),
            runtime: "codex".into(),
            config: json!({
                "builtin": true,
                "sandboxMode": sandbox_mode,
            }),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_agent_profile(&profile)?;
        profiles.insert(role.into(), profile);
    }
    Ok(profiles.into_values().collect())
}
