use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::model::{Artifact, ArtifactMode, ManagedBlockStyle, ManagedFileStyle, canonical_text};

const HASH_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeOutcome {
    Created,
    AddedManagedContent,
    UpdatedManagedContent,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeResult {
    pub contents: String,
    pub outcome: MergeOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MergeConflict {
    ModifiedManagedBlock { id: String },
    ModifiedManagedFile,
    UnmanagedFile,
    JsonValue { pointer: String },
}

#[derive(Debug, Error)]
pub enum MergeError {
    #[error("managed block `{id}` is malformed: {reason}")]
    MalformedBlock { id: String, reason: &'static str },
    #[error("managed content conflict: {0:?}")]
    Conflict(MergeConflict),
    #[error("existing JSON is invalid: {0}")]
    ExistingJson(#[source] serde_json::Error),
    #[error("generated JSON is invalid: {0}")]
    GeneratedJson(#[source] serde_json::Error),
    #[error("generated Markdown does not contain closed YAML frontmatter")]
    MissingFrontmatter,
    #[error("managed-file checksum marker is malformed")]
    MalformedFileMarker,
}

/// Merge one generated artifact without silently replacing conflicting content.
///
/// # Errors
///
/// Returns an error when managed markers are malformed, a checksum shows user
/// modification, JSON is invalid, or generated keys conflict with user values.
pub fn merge_artifact(
    existing: Option<&str>,
    artifact: &Artifact,
) -> Result<MergeResult, MergeError> {
    match &artifact.mode {
        ArtifactMode::ManagedBlock { id, style } => {
            merge_managed_block(existing, id, &artifact.contents, *style)
        }
        ArtifactMode::ManagedFile { style } => {
            merge_managed_file(existing, &artifact.contents, *style)
        }
        ArtifactMode::JsonObject => merge_json(existing, &artifact.contents),
    }
}

/// Insert or replace one checksum-protected block in a user-owned document.
/// Existing content outside the named block is byte-preserved.
///
/// # Errors
///
/// Returns an error when the named markers are malformed or the current block
/// body no longer matches its recorded checksum.
pub fn merge_managed_block(
    existing: Option<&str>,
    id: &str,
    desired_body: &str,
    style: ManagedBlockStyle,
) -> Result<MergeResult, MergeError> {
    let desired = canonical_text(desired_body);
    let newline = existing.map_or("\n", detect_newline);
    let rendered = render_block(id, &desired, style, newline);
    let Some(current) = existing else {
        return Ok(MergeResult {
            contents: rendered,
            outcome: MergeOutcome::Created,
        });
    };

    let canonical_current = current.replace("\r\n", "\n").replace('\r', "\n");
    let (start_prefix, end_marker) = block_markers(id, style);
    let start_matches: Vec<_> = canonical_current.match_indices(&start_prefix).collect();
    let end_matches: Vec<_> = canonical_current.match_indices(&end_marker).collect();

    if start_matches.is_empty() && end_matches.is_empty() {
        let mut output = current.to_owned();
        if !output.is_empty() {
            if !output.ends_with('\n') && !output.ends_with('\r') {
                output.push_str(newline);
            }
            if !output.ends_with(&format!("{newline}{newline}")) {
                output.push_str(newline);
            }
        }
        output.push_str(&rendered);
        return Ok(MergeResult {
            contents: output,
            outcome: MergeOutcome::AddedManagedContent,
        });
    }
    if start_matches.len() != 1 || end_matches.len() != 1 {
        return Err(MergeError::MalformedBlock {
            id: id.to_owned(),
            reason: "expected exactly one start marker and one end marker",
        });
    }

    let start_line = line_bounds(&canonical_current, start_matches[0].0);
    let end_line = line_bounds(&canonical_current, end_matches[0].0);
    if end_line.0 <= start_line.1 {
        return Err(MergeError::MalformedBlock {
            id: id.to_owned(),
            reason: "end marker precedes start marker",
        });
    }
    let start_marker = canonical_current[start_line.0..start_line.1].trim_end_matches('\n');
    let expected_hash = parse_block_hash(start_marker, &start_prefix).ok_or_else(|| {
        MergeError::MalformedBlock {
            id: id.to_owned(),
            reason: "start marker has no valid SHA-256 checksum",
        }
    })?;
    let current_body = canonical_text(&canonical_current[start_line.1..end_line.0]);
    if checksum(&current_body) != expected_hash {
        return Err(MergeError::Conflict(MergeConflict::ModifiedManagedBlock {
            id: id.to_owned(),
        }));
    }
    if current_body == desired {
        return Ok(MergeResult {
            contents: current.to_owned(),
            outcome: MergeOutcome::Unchanged,
        });
    }

    let byte_start = newline_offset(current, start_line.0);
    let byte_end = newline_offset(current, end_line.1);
    let mut output = String::with_capacity(current.len() + rendered.len());
    output.push_str(&current[..byte_start]);
    output.push_str(&rendered);
    output.push_str(&current[byte_end..]);
    Ok(MergeResult {
        contents: output,
        outcome: MergeOutcome::UpdatedManagedContent,
    })
}

fn merge_managed_file(
    existing: Option<&str>,
    desired_payload: &str,
    style: ManagedFileStyle,
) -> Result<MergeResult, MergeError> {
    let desired = canonical_text(desired_payload);
    let rendered = render_managed_file(&desired, style)?;
    let Some(current) = existing else {
        return Ok(MergeResult {
            contents: rendered,
            outcome: MergeOutcome::Created,
        });
    };
    let newline = detect_newline(current);
    let normalized = current.replace("\r\n", "\n").replace('\r', "\n");
    match remove_file_marker(&normalized, style)? {
        Some((payload, expected_hash)) => {
            let payload = canonical_text(&payload);
            if checksum(&payload) != expected_hash {
                return Err(MergeError::Conflict(MergeConflict::ModifiedManagedFile));
            }
            if payload == desired {
                return Ok(MergeResult {
                    contents: current.to_owned(),
                    outcome: MergeOutcome::Unchanged,
                });
            }
        }
        None if canonical_text(&normalized) == desired => {}
        None => return Err(MergeError::Conflict(MergeConflict::UnmanagedFile)),
    }
    Ok(MergeResult {
        contents: with_newline(&rendered, newline),
        outcome: MergeOutcome::UpdatedManagedContent,
    })
}

fn merge_json(existing: Option<&str>, generated: &str) -> Result<MergeResult, MergeError> {
    let desired: Value = serde_json::from_str(generated).map_err(MergeError::GeneratedJson)?;
    let Some(current_text) = existing else {
        return Ok(MergeResult {
            contents: format!(
                "{}\n",
                serde_json::to_string_pretty(&desired).map_err(MergeError::GeneratedJson)?
            ),
            outcome: MergeOutcome::Created,
        });
    };
    let mut current: Value =
        serde_json::from_str(current_text).map_err(MergeError::ExistingJson)?;
    let before = current.clone();
    merge_json_value(&mut current, &desired, "")?;
    if current == before {
        return Ok(MergeResult {
            contents: current_text.to_owned(),
            outcome: MergeOutcome::Unchanged,
        });
    }
    Ok(MergeResult {
        contents: format!(
            "{}\n",
            serde_json::to_string_pretty(&current).map_err(MergeError::GeneratedJson)?
        ),
        outcome: MergeOutcome::AddedManagedContent,
    })
}

fn merge_json_value(current: &mut Value, desired: &Value, pointer: &str) -> Result<(), MergeError> {
    match (current, desired) {
        (Value::Object(current), Value::Object(desired)) => {
            for (key, desired_value) in desired {
                let next = format!("{pointer}/{}", json_pointer_escape(key));
                if let Some(current_value) = current.get_mut(key) {
                    merge_json_value(current_value, desired_value, &next)?;
                } else {
                    current.insert(key.clone(), desired_value.clone());
                }
            }
            Ok(())
        }
        (current, desired) => {
            if &*current == desired {
                Ok(())
            } else {
                Err(MergeError::Conflict(MergeConflict::JsonValue {
                    pointer: if pointer.is_empty() {
                        "/".to_owned()
                    } else {
                        pointer.to_owned()
                    },
                }))
            }
        }
    }
}

fn render_block(id: &str, body: &str, style: ManagedBlockStyle, newline: &str) -> String {
    let (start_prefix, end) = block_markers(id, style);
    with_newline(
        &format!("{start_prefix}{}\n{body}{end}\n", checksum(body)),
        newline,
    )
}

fn block_markers(id: &str, style: ManagedBlockStyle) -> (String, String) {
    match style {
        ManagedBlockStyle::Html => (
            format!("<!-- ba-agent:managed:start id={id} sha256="),
            format!("<!-- ba-agent:managed:end id={id} -->"),
        ),
        ManagedBlockStyle::Hash => (
            format!("# ba-agent:managed:start id={id} sha256="),
            format!("# ba-agent:managed:end id={id}"),
        ),
    }
}

fn parse_block_hash<'a>(marker: &'a str, prefix: &str) -> Option<&'a str> {
    let remainder = marker.strip_prefix(prefix)?;
    let value = remainder.strip_suffix(" -->").unwrap_or(remainder);
    is_hash(value).then_some(value)
}

fn render_managed_file(payload: &str, style: ManagedFileStyle) -> Result<String, MergeError> {
    let marker_hash = checksum(payload);
    match style {
        ManagedFileStyle::HashFirst => Ok(format!(
            "# ba-agent:managed-file sha256={marker_hash}\n{payload}"
        )),
        ManagedFileStyle::HashAfterShebang => {
            if let Some(rest) = payload.strip_prefix("#!") {
                let line_end = rest.find('\n').map_or(payload.len(), |index| index + 3);
                Ok(format!(
                    "{}# ba-agent:managed-file sha256={marker_hash}\n{}",
                    &payload[..line_end],
                    &payload[line_end..]
                ))
            } else {
                Ok(format!(
                    "# ba-agent:managed-file sha256={marker_hash}\n{payload}"
                ))
            }
        }
        ManagedFileStyle::HtmlAfterFrontmatter => {
            let close = frontmatter_end(payload).ok_or(MergeError::MissingFrontmatter)?;
            Ok(format!(
                "{}<!-- ba-agent:managed-file sha256={marker_hash} -->\n{}",
                &payload[..close],
                &payload[close..]
            ))
        }
    }
}

fn remove_file_marker(
    current: &str,
    style: ManagedFileStyle,
) -> Result<Option<(String, String)>, MergeError> {
    let (prefix, suffix) = match style {
        ManagedFileStyle::HashFirst | ManagedFileStyle::HashAfterShebang => {
            ("# ba-agent:managed-file sha256=", "")
        }
        ManagedFileStyle::HtmlAfterFrontmatter => ("<!-- ba-agent:managed-file sha256=", " -->"),
    };
    let mut matches = current
        .match_indices(prefix)
        .map(|(index, _)| line_bounds(current, index));
    let Some(bounds) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(MergeError::MalformedFileMarker);
    }
    let marker = current[bounds.0..bounds.1].trim_end_matches('\n');
    let hash = marker
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .filter(|value| is_hash(value))
        .ok_or(MergeError::MalformedFileMarker)?
        .to_owned();
    let mut payload = String::with_capacity(current.len());
    payload.push_str(&current[..bounds.0]);
    payload.push_str(&current[bounds.1..]);
    Ok(Some((payload, hash)))
}

fn frontmatter_end(payload: &str) -> Option<usize> {
    if !payload.starts_with("---\n") {
        return None;
    }
    payload[4..]
        .find("\n---\n")
        .map(|index| index + 4 + "\n---\n".len())
}

fn line_bounds(text: &str, index: usize) -> (usize, usize) {
    let start = text[..index].rfind('\n').map_or(0, |value| value + 1);
    let end = text[index..]
        .find('\n')
        .map_or(text.len(), |value| index + value + 1);
    (start, end)
}

fn newline_offset(original: &str, canonical_offset: usize) -> usize {
    let mut canonical = 0;
    for (index, character) in original.char_indices() {
        if canonical == canonical_offset {
            return index;
        }
        if character != '\r' {
            canonical += character.len_utf8();
        }
    }
    original.len()
}

fn checksum(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn is_hash(value: &str) -> bool {
    value.len() == HASH_LEN && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn detect_newline(value: &str) -> &'static str {
    if value.contains("\r\n") { "\r\n" } else { "\n" }
}

fn with_newline(value: &str, newline: &str) -> String {
    if newline == "\r\n" {
        value.replace('\n', "\r\n")
    } else {
        value.to_owned()
    }
}

fn json_pointer_escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn managed_block_preserves_user_text_and_updates_verified_content() -> Result<(), MergeError> {
        let first = merge_managed_block(
            Some("user before\n\nuser after\n"),
            "instructions",
            "generated v1",
            ManagedBlockStyle::Html,
        )?;
        assert_eq!(first.outcome, MergeOutcome::AddedManagedContent);
        assert!(first.contents.starts_with("user before\n\nuser after\n"));

        let second = merge_managed_block(
            Some(&first.contents),
            "instructions",
            "generated v2",
            ManagedBlockStyle::Html,
        )?;
        assert_eq!(second.outcome, MergeOutcome::UpdatedManagedContent);
        assert!(second.contents.contains("generated v2"));
        assert!(second.contents.starts_with("user before\n\nuser after\n"));
        Ok(())
    }

    #[test]
    fn managed_block_refuses_an_edited_generated_body() -> Result<(), MergeError> {
        let first =
            merge_managed_block(None, "instructions", "generated", ManagedBlockStyle::Html)?;
        let edited = first.contents.replace("generated", "user edit");
        let result = merge_managed_block(
            Some(&edited),
            "instructions",
            "replacement",
            ManagedBlockStyle::Html,
        );
        assert!(matches!(
            result,
            Err(MergeError::Conflict(
                MergeConflict::ModifiedManagedBlock { .. }
            ))
        ));
        Ok(())
    }

    #[test]
    fn managed_block_preserves_crlf() -> Result<(), MergeError> {
        let first = merge_managed_block(
            Some("user\r\n"),
            "instructions",
            "generated",
            ManagedBlockStyle::Html,
        )?;
        assert!(!first.contents.replace("\r\n", "").contains('\n'));
        let second = merge_managed_block(
            Some(&first.contents),
            "instructions",
            "updated",
            ManagedBlockStyle::Html,
        )?;
        assert!(!second.contents.replace("\r\n", "").contains('\n'));
        Ok(())
    }

    #[test]
    fn managed_file_refuses_unmanaged_and_modified_files() -> Result<(), MergeError> {
        let artifact = Artifact {
            relative_path: PathBuf::from("role.toml"),
            contents: "name = \"role\"\n".to_owned(),
            mode: ArtifactMode::ManagedFile {
                style: ManagedFileStyle::HashFirst,
            },
        };
        assert!(matches!(
            merge_artifact(Some("user = true\n"), &artifact),
            Err(MergeError::Conflict(MergeConflict::UnmanagedFile))
        ));
        let created = merge_artifact(None, &artifact)?;
        let edited = created.contents.replace("role\"", "changed\"");
        assert!(matches!(
            merge_artifact(Some(&edited), &artifact),
            Err(MergeError::Conflict(MergeConflict::ModifiedManagedFile))
        ));
        Ok(())
    }

    #[test]
    fn skill_marker_keeps_yaml_frontmatter_first() -> Result<(), MergeError> {
        let artifact = Artifact {
            relative_path: PathBuf::from("SKILL.md"),
            contents: "---\nname: demo\ndescription: Demo.\n---\n\n# Demo\n".to_owned(),
            mode: ArtifactMode::ManagedFile {
                style: ManagedFileStyle::HtmlAfterFrontmatter,
            },
        };
        let created = merge_artifact(None, &artifact)?;
        assert!(created.contents.starts_with("---\n"));
        assert!(
            created
                .contents
                .contains("\n---\n<!-- ba-agent:managed-file")
        );
        assert_eq!(
            merge_artifact(Some(&created.contents), &artifact)?.outcome,
            MergeOutcome::Unchanged
        );
        Ok(())
    }

    #[test]
    fn script_marker_keeps_the_shebang_first() -> Result<(), MergeError> {
        let artifact = Artifact {
            relative_path: PathBuf::from("hook_guard.py"),
            contents: "#!/usr/bin/env python3\nprint('guard')\n".to_owned(),
            mode: ArtifactMode::ManagedFile {
                style: ManagedFileStyle::HashAfterShebang,
            },
        };
        let created = merge_artifact(None, &artifact)?;
        assert!(
            created
                .contents
                .starts_with("#!/usr/bin/env python3\n# ba-agent:")
        );
        assert_eq!(
            merge_artifact(Some(&created.contents), &artifact)?.outcome,
            MergeOutcome::Unchanged
        );
        Ok(())
    }

    #[test]
    fn json_merge_adds_absent_keys_and_reports_exact_conflict_pointer() -> Result<(), MergeError> {
        let artifact = Artifact {
            relative_path: PathBuf::from("settings.json"),
            contents: "{\"hooks\":{\"Stop\":[]}}\n".to_owned(),
            mode: ArtifactMode::JsonObject,
        };
        let added = merge_artifact(Some("{\"language\":\"zh-CN\"}\n"), &artifact)?;
        assert!(added.contents.contains("\"language\""));
        assert!(added.contents.contains("\"hooks\""));

        let conflict = merge_artifact(Some("{\"hooks\":{\"Stop\":[{\"user\":true}]}}"), &artifact);
        assert!(matches!(
            conflict,
            Err(MergeError::Conflict(MergeConflict::JsonValue { pointer }))
                if pointer == "/hooks/Stop"
        ));
        Ok(())
    }
}
