use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

/// A runtime whose project configuration can be generated from the shared model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    Codex,
    Claude,
}

/// One explicitly reported runtime compatibility capability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityFeature {
    pub supported: bool,
    pub note: String,
}

/// Static compatibility of one renderer with the shared canonical project model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityReport {
    pub runtime: Runtime,
    pub mode: String,
    pub source_roles: usize,
    pub source_skills: usize,
    pub generated_artifacts: usize,
    pub features: BTreeMap<String, CompatibilityFeature>,
    pub warnings: Vec<String>,
}

/// Comment syntax for a managed block embedded in a user-owned file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedBlockStyle {
    Html,
    Hash,
}

/// Checksum marker placement for a BA Agent-owned file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedFileStyle {
    HashFirst,
    HashAfterShebang,
    HtmlAfterFrontmatter,
}

/// How an artifact may be combined with existing project content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactMode {
    /// Replace only BA Agent's named block and preserve all other text.
    ManagedBlock {
        id: String,
        style: ManagedBlockStyle,
    },
    /// Replace the whole file only when its embedded checksum proves that the
    /// previous generated payload was not edited by the user.
    ManagedFile { style: ManagedFileStyle },
    /// Recursively add absent JSON keys; any unequal existing value is a conflict.
    JsonObject,
}

/// One generated project-relative artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub relative_path: PathBuf,
    pub contents: String,
    pub mode: ArtifactMode,
}

/// A deterministic set of artifacts for one runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedConfig {
    pub runtime: Runtime,
    pub artifacts: Vec<Artifact>,
}

/// Canonical role data used by both runtime renderers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDefinition {
    /// Stable filename identifier. It is supplied by the source filename and is
    /// not serialized into Codex TOML.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_mode: Option<String>,
    pub developer_instructions: String,
}

/// A validated Skill package entrypoint shared by both runtimes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillDefinition {
    pub name: String,
    pub markdown: String,
}

/// Canonical project configuration loaded from a BA Agent plugin bundle or
/// assembled by a caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectConfig {
    pub instructions: String,
    pub roles: Vec<RoleDefinition>,
    pub skills: Vec<SkillDefinition>,
    /// The `hooks` object, without a surrounding settings document.
    pub hooks: Map<String, Value>,
    /// Optional Python guard copied into the project so generated hook commands
    /// do not depend on an installation/cache path.
    pub hook_guard_script: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("unable to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid role template {path}: {source}")]
    RoleToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid hook descriptor {path}: {source}")]
    HookJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("hook descriptor {0} must contain an object field named `hooks`")]
    HookShape(PathBuf),
    #[error("invalid {kind} identifier `{value}`")]
    InvalidIdentifier { kind: &'static str, value: String },
    #[error("duplicate {kind} identifier `{value}`")]
    DuplicateIdentifier { kind: &'static str, value: String },
    #[error("skill {path} does not start with YAML frontmatter")]
    InvalidSkill { path: PathBuf },
    #[error("skill `{name}` does not start with YAML frontmatter")]
    InvalidSkillContent { name: String },
    #[error("role `{role}` field `{field}` must not be empty")]
    EmptyRoleField { role: String, field: &'static str },
    #[error("role `{role}` has unsupported sandbox mode `{value}`")]
    InvalidSandboxMode { role: String, value: String },
    #[error("unable to serialize generated configuration: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("unable to serialize role template: {0}")]
    SerializeToml(#[from] toml::ser::Error),
}

impl ProjectConfig {
    /// Load the canonical roles, Skills, hook descriptor, and hook script from
    /// a BA Agent plugin root. Directory iteration is sorted for reproducibility.
    ///
    /// # Errors
    ///
    /// Returns an error when a source file cannot be read or parsed, when the
    /// hook descriptor has the wrong shape, or when an identifier is unsafe.
    pub fn from_plugin(
        plugin_root: impl AsRef<Path>,
        instructions: impl Into<String>,
    ) -> Result<Self, ConfigError> {
        let plugin_root = plugin_root.as_ref();
        let mut roles = Vec::new();
        for path in sorted_files(&plugin_root.join("agents"), "toml")? {
            let text = read_text(&path)?;
            let mut role: RoleDefinition =
                toml::from_str(&text).map_err(|source| ConfigError::RoleToml {
                    path: path.clone(),
                    source,
                })?;
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .clone_into(&mut role.id);
            roles.push(role);
        }

        let skills_root = plugin_root.join("skills");
        let mut skill_dirs = read_dir_paths(&skills_root)?;
        skill_dirs.retain(|path| path.is_dir());
        skill_dirs.sort();
        let mut skills = Vec::new();
        for skill_root in skill_dirs {
            let Some(name) = skill_root.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let skill_path = skill_root.join("SKILL.md");
            let markdown = read_text(&skill_path)?;
            if !markdown.starts_with("---\n") && !markdown.starts_with("---\r\n") {
                return Err(ConfigError::InvalidSkill { path: skill_path });
            }
            skills.push(SkillDefinition {
                name: name.to_owned(),
                markdown,
            });
        }

        let hook_path = plugin_root.join("hooks").join("hooks.json");
        let hook_document: Value =
            serde_json::from_str(&read_text(&hook_path)?).map_err(|source| {
                ConfigError::HookJson {
                    path: hook_path.clone(),
                    source,
                }
            })?;
        let mut hooks = hook_document
            .get("hooks")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| ConfigError::HookShape(hook_path.clone()))?;

        let script_path = plugin_root.join("scripts").join("hook_guard.py");
        let hook_guard_script = if script_path.is_file() {
            Some(read_text(&script_path)?)
        } else {
            None
        };
        if hook_guard_script.is_some() {
            rewrite_hook_commands(&mut hooks);
        }

        let config = Self {
            instructions: instructions.into(),
            roles,
            skills,
            hooks,
            hook_guard_script,
        };
        config.validate()?;
        Ok(config)
    }

    /// Check path-safe uniqueness and role-name validity before generation.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe or duplicate role/Skill identifier.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_unique(self.roles.iter().map(|role| role.id.as_str()), "role")?;
        validate_unique(self.skills.iter().map(|skill| skill.name.as_str()), "skill")?;
        for role in &self.roles {
            validate_identifier(&role.name, "role name")?;
            for (field, value) in [
                ("name", role.name.as_str()),
                ("description", role.description.as_str()),
                (
                    "developer_instructions",
                    role.developer_instructions.as_str(),
                ),
            ] {
                if value.trim().is_empty() {
                    return Err(ConfigError::EmptyRoleField {
                        role: role.id.clone(),
                        field,
                    });
                }
            }
            if let Some(value) = role.sandbox_mode.as_deref() {
                if !matches!(
                    value,
                    "read-only" | "workspace-write" | "danger-full-access"
                ) {
                    return Err(ConfigError::InvalidSandboxMode {
                        role: role.id.clone(),
                        value: value.to_owned(),
                    });
                }
            }
        }
        for skill in &self.skills {
            if skill.name.contains('_') {
                return Err(ConfigError::InvalidIdentifier {
                    kind: "skill",
                    value: skill.name.clone(),
                });
            }
            if !skill.markdown.starts_with("---\n") && !skill.markdown.starts_with("---\r\n") {
                return Err(ConfigError::InvalidSkillContent {
                    name: skill.name.clone(),
                });
            }
        }
        Ok(())
    }

    /// Generate a deterministic plan. No filesystem state is read or changed.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or duplicate identifiers or when a target
    /// representation cannot be serialized.
    pub fn generate(&self, runtime: Runtime) -> Result<GeneratedConfig, ConfigError> {
        self.validate()?;
        let artifacts = match runtime {
            Runtime::Codex => self.generate_codex()?,
            Runtime::Claude => self.generate_claude()?,
        };
        Ok(GeneratedConfig { runtime, artifacts })
    }

    /// Describe supported and deliberately unsupported behavior for one renderer.
    ///
    /// # Errors
    ///
    /// Returns an error when canonical identifiers or a generated representation
    /// are invalid.
    pub fn compatibility_report(
        &self,
        runtime: Runtime,
    ) -> Result<CompatibilityReport, ConfigError> {
        let generated = self.generate(runtime)?;
        let mut features = BTreeMap::new();
        let mut feature = |name: &str, supported: bool, note: &str| {
            features.insert(
                name.to_owned(),
                CompatibilityFeature {
                    supported,
                    note: note.to_owned(),
                },
            );
        };
        feature(
            "project_instructions",
            true,
            "Checksum-managed instructions preserve user-owned surrounding content.",
        );
        feature("roles", true, "All canonical narrow roles are rendered.");
        feature("skills", true, "All canonical Skills are rendered.");
        feature(
            "hooks",
            true,
            "Shared hook policy is rendered with runtime-specific settings fields.",
        );
        feature(
            "status_import",
            true,
            "Structured snapshots import as immutable evidence without bypassing the task state machine.",
        );
        let live = runtime == Runtime::Codex;
        feature(
            "live_dispatch",
            live,
            if live {
                "Provided by the Codex runtime adapter after capability handshake."
            } else {
                "Claude live dispatch is outside the personal Alpha."
            },
        );
        feature(
            "realtime_events",
            live,
            if live {
                "Provided by App Server with exec-json fallback."
            } else {
                "Claude event streaming is not connected in the personal Alpha."
            },
        );
        feature(
            "interactive_approvals",
            live,
            if live {
                "Provided by the active Codex runtime session."
            } else {
                "Generated Hooks do not imply live BA Agent approval control."
            },
        );
        Ok(CompatibilityReport {
            runtime,
            mode: if live { "live" } else { "configuration_only" }.into(),
            source_roles: self.roles.len(),
            source_skills: self.skills.len(),
            generated_artifacts: generated.artifacts.len(),
            features,
            warnings: if live {
                Vec::new()
            } else {
                vec![
                    "Configuration compatibility is not runtime parity.".into(),
                    "Claude credentials are never read or stored by BA Agent.".into(),
                ]
            },
        })
    }

    fn generate_codex(&self) -> Result<Vec<Artifact>, ConfigError> {
        let mut artifacts = vec![Artifact {
            relative_path: PathBuf::from("AGENTS.md"),
            contents: canonical_text(&self.instructions),
            mode: ArtifactMode::ManagedBlock {
                id: "project-instructions".to_owned(),
                style: ManagedBlockStyle::Html,
            },
        }];

        for role in sorted_roles(&self.roles) {
            let source = CodexRole::from(role);
            artifacts.push(Artifact {
                relative_path: PathBuf::from(".codex")
                    .join("agents")
                    .join(format!("{}.toml", role.id)),
                contents: canonical_text(&toml::to_string_pretty(&source)?),
                mode: ArtifactMode::ManagedFile {
                    style: ManagedFileStyle::HashFirst,
                },
            });
        }

        for skill in sorted_skills(&self.skills) {
            artifacts.push(Artifact {
                relative_path: PathBuf::from(".agents")
                    .join("skills")
                    .join(&skill.name)
                    .join("SKILL.md"),
                contents: canonical_text(&skill.markdown),
                mode: ArtifactMode::ManagedFile {
                    style: ManagedFileStyle::HtmlAfterFrontmatter,
                },
            });
        }

        artifacts.extend(self.shared_hook_artifacts(Runtime::Codex)?);
        Ok(artifacts)
    }

    fn generate_claude(&self) -> Result<Vec<Artifact>, ConfigError> {
        let mut artifacts = vec![Artifact {
            relative_path: PathBuf::from("CLAUDE.md"),
            contents: canonical_text(&self.instructions),
            mode: ArtifactMode::ManagedBlock {
                id: "project-instructions".to_owned(),
                style: ManagedBlockStyle::Html,
            },
        }];

        for role in sorted_roles(&self.roles) {
            artifacts.push(Artifact {
                relative_path: PathBuf::from(".claude")
                    .join("agents")
                    .join(format!("{}.md", role.id)),
                contents: render_claude_role(role),
                mode: ArtifactMode::ManagedFile {
                    style: ManagedFileStyle::HtmlAfterFrontmatter,
                },
            });
        }

        for skill in sorted_skills(&self.skills) {
            artifacts.push(Artifact {
                relative_path: PathBuf::from(".claude")
                    .join("skills")
                    .join(&skill.name)
                    .join("SKILL.md"),
                contents: canonical_text(&skill.markdown),
                mode: ArtifactMode::ManagedFile {
                    style: ManagedFileStyle::HtmlAfterFrontmatter,
                },
            });
        }

        artifacts.extend(self.shared_hook_artifacts(Runtime::Claude)?);
        Ok(artifacts)
    }

    fn shared_hook_artifacts(&self, runtime: Runtime) -> Result<Vec<Artifact>, ConfigError> {
        let mut artifacts = Vec::new();
        if let Some(script) = &self.hook_guard_script {
            artifacts.push(Artifact {
                relative_path: PathBuf::from(".ba-agent")
                    .join("generated")
                    .join("hooks")
                    .join("hook_guard.py"),
                contents: canonical_text(script),
                mode: ArtifactMode::ManagedFile {
                    style: ManagedFileStyle::HashAfterShebang,
                },
            });
        }

        if self.hooks.is_empty() {
            return Ok(artifacts);
        }
        let document = Value::Object(Map::from_iter([(
            "hooks".to_owned(),
            Value::Object(hooks_for_runtime(&self.hooks, runtime)),
        )]));
        let relative_path = match runtime {
            Runtime::Codex => PathBuf::from(".codex").join("hooks.json"),
            Runtime::Claude => PathBuf::from(".claude").join("settings.json"),
        };
        artifacts.push(Artifact {
            relative_path,
            contents: format!("{}\n", serde_json::to_string_pretty(&document)?),
            mode: ArtifactMode::JsonObject,
        });
        Ok(artifacts)
    }
}

#[derive(Serialize)]
struct CodexRole<'a> {
    name: &'a str,
    description: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox_mode: Option<&'a str>,
    developer_instructions: &'a str,
}

impl<'a> From<&'a RoleDefinition> for CodexRole<'a> {
    fn from(role: &'a RoleDefinition) -> Self {
        Self {
            name: &role.name,
            description: &role.description,
            sandbox_mode: role.sandbox_mode.as_deref(),
            developer_instructions: &role.developer_instructions,
        }
    }
}

fn render_claude_role(role: &RoleDefinition) -> String {
    let name = yaml_string(&role.id);
    let description = yaml_string(&role.description);
    let permission_mode = match role.sandbox_mode.as_deref() {
        Some("read-only") => "plan",
        Some("workspace-write") => "acceptEdits",
        _ => "default",
    };
    canonical_text(&format!(
        "---\nname: {name}\ndescription: {description}\npermissionMode: {permission_mode}\n---\n\n{}",
        role.developer_instructions.trim()
    ))
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

fn sorted_roles(roles: &[RoleDefinition]) -> Vec<&RoleDefinition> {
    let mut values: Vec<_> = roles.iter().collect();
    values.sort_by(|left, right| left.id.cmp(&right.id));
    values
}

fn sorted_skills(skills: &[SkillDefinition]) -> Vec<&SkillDefinition> {
    let mut values: Vec<_> = skills.iter().collect();
    values.sort_by(|left, right| left.name.cmp(&right.name));
    values
}

fn validate_unique<'a>(
    values: impl IntoIterator<Item = &'a str>,
    kind: &'static str,
) -> Result<(), ConfigError> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_identifier(value, kind)?;
        if !seen.insert(value) {
            return Err(ConfigError::DuplicateIdentifier {
                kind,
                value: value.to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, kind: &'static str) -> Result<(), ConfigError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric);
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidIdentifier {
            kind,
            value: value.to_owned(),
        })
    }
}

fn sorted_files(directory: &Path, extension: &str) -> Result<Vec<PathBuf>, ConfigError> {
    let mut paths = read_dir_paths(directory)?;
    paths.retain(|path| {
        path.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    });
    paths.sort();
    Ok(paths)
}

fn read_dir_paths(directory: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    fs::read_dir(directory)
        .map_err(|source| ConfigError::Read {
            path: directory.to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry
                .map(|value| value.path())
                .map_err(|source| ConfigError::Read {
                    path: directory.to_path_buf(),
                    source,
                })
        })
        .collect()
}

fn read_text(path: &Path) -> Result<String, ConfigError> {
    fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn rewrite_hook_commands(hooks: &mut Map<String, Value>) {
    fn visit(value: &mut Value) {
        match value {
            Value::Array(values) => values.iter_mut().for_each(visit),
            Value::Object(values) => {
                for (key, value) in values {
                    match (key.as_str(), value) {
                        ("command", Value::String(command))
                            if command.contains("hook_guard.py") =>
                        {
                            "python3 \".ba-agent/generated/hooks/hook_guard.py\""
                                .clone_into(command);
                        }
                        ("commandWindows", Value::String(command))
                            if command.contains("hook_guard.py") =>
                        {
                            "py -3 \".ba-agent\\generated\\hooks\\hook_guard.py\""
                                .clone_into(command);
                        }
                        (_, nested) => visit(nested),
                    }
                }
            }
            _ => {}
        }
    }

    hooks.values_mut().for_each(visit);
}

fn hooks_for_runtime(hooks: &Map<String, Value>, runtime: Runtime) -> Map<String, Value> {
    fn visit(value: &mut Value) {
        match value {
            Value::Array(values) => values.iter_mut().for_each(visit),
            Value::Object(values) => {
                if cfg!(windows) {
                    let windows_command = values
                        .get("commandWindows")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    if let Some(command) = windows_command {
                        values.insert("command".to_owned(), Value::String(command));
                    }
                }
                values.remove("commandWindows");
                values.remove("additionalContextLimit");
                values.values_mut().for_each(visit);
            }
            _ => {}
        }
    }

    let mut hooks = hooks.clone();
    if runtime != Runtime::Claude {
        return hooks;
    }
    hooks.values_mut().for_each(visit);
    hooks
}

pub(crate) fn canonical_text(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    format!("{}\n", normalized.trim_end_matches('\n'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProjectConfig {
        ProjectConfig {
            instructions: "Keep evidence.\nAsk before remote writes.".to_owned(),
            roles: vec![RoleDefinition {
                id: "reviewer".to_owned(),
                name: "reviewer".to_owned(),
                description: "Review behavior and evidence.".to_owned(),
                sandbox_mode: Some("read-only".to_owned()),
                developer_instructions: "Stay read-only.".to_owned(),
            }],
            skills: vec![SkillDefinition {
                name: "ba-review".to_owned(),
                markdown: "---\nname: ba-review\ndescription: Review work.\n---\n\n# Review\n\nCheck evidence.\n".to_owned(),
            }],
            hooks: Map::from_iter([("Stop".to_owned(), Value::Array(Vec::new()))]),
            hook_guard_script: Some("#!/usr/bin/env python3\nprint('ok')\n".to_owned()),
        }
    }

    #[test]
    fn generates_runtime_specific_targets_from_one_model() -> Result<(), ConfigError> {
        let codex = sample().generate(Runtime::Codex)?;
        let claude = sample().generate(Runtime::Claude)?;
        let codex_paths: BTreeSet<_> = codex
            .artifacts
            .iter()
            .map(|item| item.relative_path.clone())
            .collect();
        let claude_paths: BTreeSet<_> = claude
            .artifacts
            .iter()
            .map(|item| item.relative_path.clone())
            .collect();

        assert!(codex_paths.contains(&PathBuf::from("AGENTS.md")));
        assert!(codex_paths.contains(&PathBuf::from(".codex/agents/reviewer.toml")));
        assert!(codex_paths.contains(&PathBuf::from(".agents/skills/ba-review/SKILL.md")));
        assert!(codex_paths.contains(&PathBuf::from(".codex/hooks.json")));
        assert!(claude_paths.contains(&PathBuf::from("CLAUDE.md")));
        assert!(claude_paths.contains(&PathBuf::from(".claude/agents/reviewer.md")));
        assert!(claude_paths.contains(&PathBuf::from(".claude/skills/ba-review/SKILL.md")));
        assert!(claude_paths.contains(&PathBuf::from(".claude/settings.json")));
        Ok(())
    }

    #[test]
    fn rejects_duplicate_and_traversal_like_identifiers() {
        let mut config = sample();
        config.skills.push(config.skills[0].clone());
        assert!(matches!(
            config.validate(),
            Err(ConfigError::DuplicateIdentifier { kind: "skill", .. })
        ));

        config.skills.pop();
        config.roles[0].id = "../reviewer".to_owned();
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidIdentifier { kind: "role", .. })
        ));

        config.roles[0].id = "reviewer".to_owned();
        config.roles[0].sandbox_mode = Some("unrestricted-ish".to_owned());
        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidSandboxMode { .. })
        ));
    }

    #[test]
    fn loads_the_repository_plugin_bundle() -> Result<(), ConfigError> {
        let plugin_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("plugins/personal-marketplace/plugins/ba-agent-control");
        let config = ProjectConfig::from_plugin(plugin_root, "Project policy")?;
        assert_eq!(config.roles.len(), 8);
        assert_eq!(config.skills.len(), 11);

        let codex = config.generate(Runtime::Codex)?;
        let hooks = codex
            .artifacts
            .iter()
            .find(|artifact| artifact.relative_path == Path::new(".codex/hooks.json"))
            .map(|artifact| artifact.contents.as_str())
            .unwrap_or_default();
        assert!(hooks.contains(".ba-agent/generated/hooks/hook_guard.py"));
        assert!(!hooks.contains("PLUGIN_ROOT"));

        let claude = config.generate(Runtime::Claude)?;
        let settings = claude
            .artifacts
            .iter()
            .find(|artifact| artifact.relative_path == Path::new(".claude/settings.json"))
            .map(|artifact| artifact.contents.as_str())
            .unwrap_or_default();
        assert!(!settings.contains("commandWindows"));
        assert!(!settings.contains("additionalContextLimit"));

        let report = config.compatibility_report(Runtime::Claude)?;
        assert_eq!(report.mode, "configuration_only");
        assert_eq!(report.source_roles, 8);
        assert_eq!(report.source_skills, 11);
        assert!(report.features["status_import"].supported);
        assert!(!report.features["live_dispatch"].supported);
        Ok(())
    }
}
