//! Optional GitHub integration with explicit, non-persistent credential precedence.
//!
//! Authentication is resolved in this order: an authenticated GitHub CLI session,
//! a BA Agent PAT in the operating-system credential store, then `GITHUB_TOKEN` from
//! the current process. Secrets are never serialized or included in diagnostics.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::process::Command;
use zeroize::Zeroizing;

pub const KEYRING_SERVICE: &str = "ba-agent-control/github";
pub const KEYRING_USER: &str = "github.com";
const GITHUB_HOST: &str = "github.com";
const API_BASE: &str = "https://api.github.com";
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubAuthSource {
    GhCli,
    OsCredentialStore,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhCliStatus {
    pub installed: bool,
    pub authenticated: bool,
    pub version: Option<String>,
    pub account: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub supported: bool,
    pub configured: bool,
    pub verified: bool,
    pub account: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubCapabilities {
    pub read_repository: bool,
    pub remote_writes: bool,
    pub remote_writes_require_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubProbe {
    pub adapter: String,
    pub available: bool,
    pub auth_source: Option<GitHubAuthSource>,
    pub account: Option<String>,
    pub gh_cli: GhCliStatus,
    pub os_credential_store: CredentialStatus,
    pub environment: CredentialStatus,
    pub capabilities: GitHubCapabilities,
    pub credentials_stored_by_ba_agent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySummary {
    pub name_with_owner: String,
    pub url: String,
    pub default_branch: Option<String>,
    pub private: bool,
    pub auth_source: GitHubAuthSource,
}

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("no usable GitHub authentication was found")]
    AuthenticationUnavailable,
    #[error("invalid GitHub repository identifier: {0}")]
    InvalidRepository(String),
    #[error("GitHub CLI failed: {0}")]
    GhCli(String),
    #[error("GitHub API request failed: {0}")]
    Api(String),
    #[error("GitHub adapter timed out")]
    Timeout,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

trait CredentialProvider: Send + Sync + std::fmt::Debug {
    fn os_token(&self) -> Result<Option<Zeroizing<String>>, String>;
    fn environment_token(&self) -> Option<Zeroizing<String>>;
}

#[derive(Debug)]
struct SystemCredentialProvider;

impl CredentialProvider for SystemCredentialProvider {
    fn os_token(&self) -> Result<Option<Zeroizing<String>>, String> {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
                .map_err(|error| error.to_string())?;
            match entry.get_password() {
                Ok(token) if !token.trim().is_empty() => Ok(Some(Zeroizing::new(token))),
                Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
                Err(error) => Err(error.to_string()),
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err("operating-system credential store is not supported".into())
        }
    }

    fn environment_token(&self) -> Option<Zeroizing<String>> {
        std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|token| !token.trim().is_empty())
            .map(Zeroizing::new)
    }
}

#[derive(Clone)]
pub struct GitHubAdapter {
    gh_binary: PathBuf,
    credentials: Arc<dyn CredentialProvider>,
    client: reqwest::Client,
    api_base: String,
}

impl std::fmt::Debug for GitHubAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GitHubAdapter")
            .field("gh_binary", &self.gh_binary)
            .field("credentials", &"<credential-provider>")
            .field("api_base", &self.api_base)
            .finish_non_exhaustive()
    }
}

impl Default for GitHubAdapter {
    fn default() -> Self {
        Self::new("gh")
    }
}

impl GitHubAdapter {
    #[must_use]
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            gh_binary: binary.into(),
            credentials: Arc::new(SystemCredentialProvider),
            client: reqwest::Client::new(),
            api_base: API_BASE.into(),
        }
    }

    /// Probe configured authentication sources without exposing credential values.
    pub async fn probe(&self) -> GitHubProbe {
        let (gh_cli, _) = self.probe_gh().await;
        let mut os_status = empty_credential_status(platform_keyring_supported());
        let mut env_status = empty_credential_status(true);
        let mut selected = gh_cli
            .authenticated
            .then_some((GitHubAuthSource::GhCli, gh_cli.account.clone()));

        let os_token = if selected.is_none() {
            match self.read_os_token().await {
                Ok(token) => {
                    os_status.configured = token.is_some();
                    token
                }
                Err(error) => {
                    os_status.error = Some(error);
                    None
                }
            }
        } else {
            None
        };
        if let Some(token) = os_token.as_deref() {
            match self.verify_token(token).await {
                Ok(account) => {
                    os_status.verified = true;
                    os_status.account = Some(account.clone());
                    selected = Some((GitHubAuthSource::OsCredentialStore, Some(account)));
                }
                Err(error) => os_status.error = Some(error.to_string()),
            }
        }

        let environment_token = if selected.is_none() {
            self.credentials.environment_token()
        } else {
            None
        };
        env_status.configured = environment_token.is_some();
        if let Some(token) = environment_token.as_deref() {
            match self.verify_token(token).await {
                Ok(account) => {
                    env_status.verified = true;
                    env_status.account = Some(account.clone());
                    selected = Some((GitHubAuthSource::Environment, Some(account)));
                }
                Err(error) => env_status.error = Some(error.to_string()),
            }
        }

        let (auth_source, account) =
            selected.map_or((None, None), |(source, account)| (Some(source), account));
        GitHubProbe {
            adapter: "github".into(),
            available: auth_source.is_some(),
            auth_source,
            account,
            gh_cli,
            os_credential_store: os_status,
            environment: env_status,
            capabilities: GitHubCapabilities {
                read_repository: auth_source.is_some(),
                remote_writes: false,
                remote_writes_require_approval: true,
            },
            credentials_stored_by_ba_agent: false,
        }
    }

    /// Return read-only metadata for an `owner/repository` identifier.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid identifier, missing authentication, a failed
    /// GitHub CLI invocation, or an unsuccessful GitHub API response.
    pub async fn view_repository(
        &self,
        repository: &str,
    ) -> Result<RepositorySummary, GitHubError> {
        validate_repository(repository)?;
        let (gh_status, gh_path) = self.probe_gh().await;
        if gh_status.authenticated {
            let path = gh_path.ok_or(GitHubError::AuthenticationUnavailable)?;
            return self.view_repository_with_gh(&path, repository).await;
        }

        if let Ok(Some(token)) = self.read_os_token().await {
            if self.verify_token(&token).await.is_ok() {
                return self
                    .view_repository_with_api(
                        repository,
                        &token,
                        GitHubAuthSource::OsCredentialStore,
                    )
                    .await;
            }
        }
        if let Some(token) = self.credentials.environment_token() {
            if self.verify_token(&token).await.is_ok() {
                return self
                    .view_repository_with_api(repository, &token, GitHubAuthSource::Environment)
                    .await;
            }
        }
        Err(GitHubError::AuthenticationUnavailable)
    }

    async fn probe_gh(&self) -> (GhCliStatus, Option<PathBuf>) {
        let Some(path) = resolve_binary_on_path(
            &self.gh_binary,
            std::env::var_os("PATH").as_deref(),
            std::env::var_os("PATHEXT").as_deref(),
        ) else {
            return (
                GhCliStatus {
                    installed: false,
                    authenticated: false,
                    version: None,
                    account: None,
                    error: None,
                },
                None,
            );
        };
        let version = run_gh(&path, &["--version"]).await;
        let version = match version {
            Ok(value) => value.lines().next().map(str::to_owned),
            Err(error) => {
                return (
                    GhCliStatus {
                        installed: true,
                        authenticated: false,
                        version: None,
                        account: None,
                        error: Some(error.to_string()),
                    },
                    Some(path),
                );
            }
        };
        match run_gh(
            &path,
            &[
                "auth",
                "status",
                "--active",
                "--hostname",
                GITHUB_HOST,
                "--json",
                "hosts",
            ],
        )
        .await
        {
            Ok(source) => match parse_gh_auth_status(&source) {
                Ok(account) => (
                    GhCliStatus {
                        installed: true,
                        authenticated: account.is_some(),
                        version,
                        account,
                        error: None,
                    },
                    Some(path),
                ),
                Err(error) => (
                    GhCliStatus {
                        installed: true,
                        authenticated: false,
                        version,
                        account: None,
                        error: Some(error.to_string()),
                    },
                    Some(path),
                ),
            },
            Err(error) => (
                GhCliStatus {
                    installed: true,
                    authenticated: false,
                    version,
                    account: None,
                    error: Some(error.to_string()),
                },
                Some(path),
            ),
        }
    }

    async fn read_os_token(&self) -> Result<Option<Zeroizing<String>>, String> {
        let credentials = self.credentials.clone();
        tokio::task::spawn_blocking(move || credentials.os_token())
            .await
            .map_err(|error| format!("credential worker failed: {error}"))?
    }

    async fn verify_token(&self, token: &str) -> Result<String, GitHubError> {
        let response = tokio::time::timeout(
            PROBE_TIMEOUT,
            self.client
                .get(format!("{}/user", self.api_base))
                .headers(token_headers(token)?)
                .send(),
        )
        .await
        .map_err(|_| GitHubError::Timeout)?
        .map_err(|error| GitHubError::Api(error.to_string()))?;
        if !response.status().is_success() {
            return Err(GitHubError::Api(format!(
                "authentication returned HTTP {}",
                response.status()
            )));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|error| GitHubError::Api(error.to_string()))?;
        value
            .get("login")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Api("user response omitted login".into()))
    }

    async fn view_repository_with_gh(
        &self,
        gh_path: &Path,
        repository: &str,
    ) -> Result<RepositorySummary, GitHubError> {
        let source = run_gh(
            gh_path,
            &[
                "repo",
                "view",
                repository,
                "--json",
                "nameWithOwner,url,defaultBranchRef,isPrivate",
            ],
        )
        .await?;
        let value: Value = serde_json::from_str(&source)?;
        Ok(RepositorySummary {
            name_with_owner: required_string(&value, "nameWithOwner")?,
            url: required_string(&value, "url")?,
            default_branch: value
                .pointer("/defaultBranchRef/name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            private: value
                .get("isPrivate")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            auth_source: GitHubAuthSource::GhCli,
        })
    }

    async fn view_repository_with_api(
        &self,
        repository: &str,
        token: &str,
        auth_source: GitHubAuthSource,
    ) -> Result<RepositorySummary, GitHubError> {
        let response = tokio::time::timeout(
            PROBE_TIMEOUT,
            self.client
                .get(format!("{}/repos/{repository}", self.api_base))
                .headers(token_headers(token)?)
                .send(),
        )
        .await
        .map_err(|_| GitHubError::Timeout)?
        .map_err(|error| GitHubError::Api(error.to_string()))?;
        if !response.status().is_success() {
            return Err(GitHubError::Api(format!(
                "repository lookup returned HTTP {}",
                response.status()
            )));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|error| GitHubError::Api(error.to_string()))?;
        Ok(RepositorySummary {
            name_with_owner: required_string(&value, "full_name")?,
            url: required_string(&value, "html_url")?,
            default_branch: value
                .get("default_branch")
                .and_then(Value::as_str)
                .map(str::to_owned),
            private: value
                .get("private")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            auth_source,
        })
    }
}

fn platform_keyring_supported() -> bool {
    cfg!(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    ))
}

fn empty_credential_status(supported: bool) -> CredentialStatus {
    CredentialStatus {
        supported,
        configured: false,
        verified: false,
        account: None,
        error: None,
    }
}

fn token_headers(token: &str) -> Result<HeaderMap, GitHubError> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("ba-agent-control"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    let authorization_source = Zeroizing::new(format!("Bearer {token}"));
    let authorization = HeaderValue::from_str(&authorization_source)
        .map_err(|_| GitHubError::Api("credential contained invalid header bytes".into()))?;
    headers.insert(AUTHORIZATION, authorization);
    headers.insert(
        "x-github-api-version",
        HeaderValue::from_static("2022-11-28"),
    );
    Ok(headers)
}

fn required_string(value: &Value, field: &str) -> Result<String, GitHubError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| GitHubError::Api(format!("response omitted {field}")))
}

fn parse_gh_auth_status(source: &str) -> Result<Option<String>, GitHubError> {
    let value: Value = serde_json::from_str(source)?;
    Ok(value
        .pointer("/hosts/github.com")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry.get("active").and_then(Value::as_bool) == Some(true)
                    && entry.get("state").and_then(Value::as_str) == Some("success")
            })
        })
        .and_then(|entry| entry.get("login"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

fn validate_repository(repository: &str) -> Result<(), GitHubError> {
    let mut parts = repository.split('/');
    let valid_part = |part: &str| {
        !part.is_empty()
            && !matches!(part, "." | "..")
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    if parts.next().is_some_and(valid_part)
        && parts.next().is_some_and(valid_part)
        && parts.next().is_none()
    {
        Ok(())
    } else {
        Err(GitHubError::InvalidRepository(repository.into()))
    }
}

async fn run_gh(path: &Path, args: &[&str]) -> Result<String, GitHubError> {
    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        Command::new(path)
            .args(args)
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_ENTERPRISE_TOKEN")
            .env_remove("GITHUB_ENTERPRISE_TOKEN")
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| GitHubError::Timeout)??;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(GitHubError::GhCli(if error.is_empty() {
            format!("process exited with {}", output.status)
        } else {
            error
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn resolve_binary_on_path(
    binary: &Path,
    path_value: Option<&OsStr>,
    pathext: Option<&OsStr>,
) -> Option<PathBuf> {
    if binary.is_absolute()
        || binary
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return binary.is_file().then(|| binary.to_path_buf());
    }
    let path_value = path_value?;
    for directory in std::env::split_paths(path_value) {
        let candidate = directory.join(binary);
        #[cfg(windows)]
        {
            if binary.extension().is_some() && candidate.is_file() {
                return Some(candidate);
            }
            if binary.extension().is_none() {
                let extensions = pathext
                    .and_then(OsStr::to_str)
                    .unwrap_or(".COM;.EXE;.BAT;.CMD");
                for extension in extensions.split(';').filter(|value| !value.is_empty()) {
                    let candidate = candidate.with_extension(extension.trim_start_matches('.'));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = pathext;
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Debug)]
    struct MockCredentialProvider {
        os_token: Option<String>,
        environment_token: Option<String>,
        os_reads: Arc<AtomicUsize>,
        environment_reads: Arc<AtomicUsize>,
    }

    impl CredentialProvider for MockCredentialProvider {
        fn os_token(&self) -> Result<Option<Zeroizing<String>>, String> {
            self.os_reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.os_token.clone().map(Zeroizing::new))
        }

        fn environment_token(&self) -> Option<Zeroizing<String>> {
            self.environment_reads.fetch_add(1, Ordering::SeqCst);
            self.environment_token.clone().map(Zeroizing::new)
        }
    }

    #[test]
    fn parses_only_active_successful_github_account() -> Result<(), GitHubError> {
        let source = r#"{"hosts":{"github.com":[{"active":true,"state":"success","login":"octocat","tokenSource":"keyring"}]}}"#;
        assert_eq!(parse_gh_auth_status(source)?, Some("octocat".into()));
        let failed =
            r#"{"hosts":{"github.com":[{"active":true,"state":"error","login":"octocat"}]}}"#;
        assert_eq!(parse_gh_auth_status(failed)?, None);
        Ok(())
    }

    #[test]
    fn repository_identifiers_are_strictly_scoped() {
        for valid in ["openai/codex", "ba-agent-control/ba_agent.control"] {
            assert!(validate_repository(valid).is_ok());
        }
        for invalid in [
            "codex",
            "openai/codex/issues",
            "../codex",
            "openai/codex?x=1",
        ] {
            assert!(validate_repository(invalid).is_err());
        }
    }

    #[test]
    fn serialized_probe_cannot_contain_a_token() -> Result<(), serde_json::Error> {
        let probe = GitHubProbe {
            adapter: "github".into(),
            available: true,
            auth_source: Some(GitHubAuthSource::Environment),
            account: Some("octocat".into()),
            gh_cli: GhCliStatus {
                installed: false,
                authenticated: false,
                version: None,
                account: None,
                error: None,
            },
            os_credential_store: empty_credential_status(true),
            environment: CredentialStatus {
                supported: true,
                configured: true,
                verified: true,
                account: Some("octocat".into()),
                error: None,
            },
            capabilities: GitHubCapabilities {
                read_repository: true,
                remote_writes: false,
                remote_writes_require_approval: true,
            },
            credentials_stored_by_ba_agent: false,
        };
        let serialized = serde_json::to_string(&probe)?;
        assert!(!serialized.contains("ghp_secret"));
        assert!(!serialized.contains("token"));
        Ok(())
    }

    #[tokio::test]
    async fn operating_system_store_precedes_environment() -> Result<(), Box<dyn std::error::Error>>
    {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut request = vec![0_u8; 4_096];
            let length = stream.read(&mut request).await?;
            let request = String::from_utf8_lossy(&request[..length]).to_ascii_lowercase();
            assert!(request.contains("authorization: bearer os-store-secret"));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 22\r\nConnection: close\r\n\r\n{\"login\":\"os-account\"}",
                )
                .await?;
            Ok::<_, std::io::Error>(())
        });
        let os_reads = Arc::new(AtomicUsize::new(0));
        let environment_reads = Arc::new(AtomicUsize::new(0));
        let adapter = GitHubAdapter {
            gh_binary: PathBuf::from("definitely-missing-ba-agent-gh"),
            credentials: Arc::new(MockCredentialProvider {
                os_token: Some("os-store-secret".into()),
                environment_token: Some("environment-secret".into()),
                os_reads: os_reads.clone(),
                environment_reads: environment_reads.clone(),
            }),
            client: reqwest::Client::new(),
            api_base: format!("http://{address}"),
        };

        let probe = adapter.probe().await;
        assert_eq!(probe.auth_source, Some(GitHubAuthSource::OsCredentialStore));
        assert_eq!(probe.account.as_deref(), Some("os-account"));
        assert_eq!(os_reads.load(Ordering::SeqCst), 1);
        assert_eq!(environment_reads.load(Ordering::SeqCst), 0);
        server.await??;
        Ok(())
    }

    #[tokio::test]
    async fn authenticated_gh_precedes_all_token_sources() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let binary = directory
            .path()
            .join(if cfg!(windows) { "gh.cmd" } else { "gh" });
        #[cfg(windows)]
        std::fs::write(
            &binary,
            "@echo off\r\nif \"%1\"==\"--version\" (echo gh version 2.99.0& exit /b 0)\r\necho {\"hosts\":{\"github.com\":[{\"active\":true,\"state\":\"success\",\"login\":\"gh-account\"}]}}\r\n",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::write(
                &binary,
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf 'gh version 2.99.0\\n'; else printf '%s\\n' '{\"hosts\":{\"github.com\":[{\"active\":true,\"state\":\"success\",\"login\":\"gh-account\"}]}}'; fi\n",
            )?;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
        }
        let os_reads = Arc::new(AtomicUsize::new(0));
        let environment_reads = Arc::new(AtomicUsize::new(0));
        let adapter = GitHubAdapter {
            gh_binary: binary,
            credentials: Arc::new(MockCredentialProvider {
                os_token: Some("os-store-secret".into()),
                environment_token: Some("environment-secret".into()),
                os_reads: os_reads.clone(),
                environment_reads: environment_reads.clone(),
            }),
            client: reqwest::Client::new(),
            api_base: API_BASE.into(),
        };

        let probe = adapter.probe().await;
        assert_eq!(probe.auth_source, Some(GitHubAuthSource::GhCli));
        assert_eq!(probe.account.as_deref(), Some("gh-account"));
        assert_eq!(os_reads.load(Ordering::SeqCst), 0);
        assert_eq!(environment_reads.load(Ordering::SeqCst), 0);
        Ok(())
    }
}
