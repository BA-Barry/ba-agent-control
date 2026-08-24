//! Shared, checksum-safe project configuration generation for Codex and Claude.
//!
//! The generator is deliberately pure: it produces [`Artifact`] values and
//! merges them with caller-supplied current content. Callers remain responsible
//! for filesystem authorization and atomic writes.

mod merge;
mod model;

pub use merge::{
    MergeConflict, MergeError, MergeOutcome, MergeResult, merge_artifact, merge_managed_block,
};
pub use model::{
    Artifact, ArtifactMode, CompatibilityFeature, CompatibilityReport, ConfigError,
    GeneratedConfig, ManagedBlockStyle, ManagedFileStyle, ProjectConfig, RoleDefinition, Runtime,
    SkillDefinition,
};
