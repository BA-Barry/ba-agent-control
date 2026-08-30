//! Loopback-only HTTP, SSE, and embedded-console server for BA Agent.

use std::{
    collections::{BTreeSet, HashMap},
    convert::Infallible,
    fs,
    io::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path as FsPath, PathBuf},
    process::Command as ProcessCommand,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response, Sse, sse::Event},
    routing::{get, post},
};
use ba_core::{
    EntityId, Milestone, Project, Run, RunEvent, RunStatus, Task, TaskStatus, Timestamp,
    Workstream, Worktree, now,
};
use ba_git::RepoPath;
use ba_github::{GitHubAdapter, GitHubError};
use ba_runtime::{ApprovalDecision as RuntimeApprovalDecision, CodexAdapter, RuntimeAdapter};
use ba_scheduler::{
    ApprovalResolution, DispatchCoordinator, FinalizationCoordinator, GateCoordinator,
    ProjectOnboardingMode, ProjectOnboardingRequest, ProjectOnboardingService, RecoveryAction,
    RecoveryCoordinator, RuntimeCoordinator, SchedulerError, SchedulerPolicyService,
    configured_gate, configured_gate_names, ensure_builtin_agent_profiles,
};
use ba_store::{SqliteStore, StoreError};
use futures_util::Stream;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
    sync::broadcast,
};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower_http::{catch_panic::CatchPanicLayer, compression::CompressionLayer, trace::TraceLayer};

pub const API_PREFIX: &str = "/api/v1";
pub const LOOPBACK_HOST: &str = "127.0.0.1";

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub port: u16,
    pub database_path: PathBuf,
    pub worktree_root: PathBuf,
    pub codex_binary: PathBuf,
    pub control_descriptor_path: PathBuf,
}

impl ServerConfig {
    #[must_use]
    pub fn new(database_path: PathBuf) -> Self {
        let worktree_root = database_path.parent().map_or_else(
            || PathBuf::from("worktrees"),
            |parent| parent.join("worktrees"),
        );
        let control_descriptor_path = database_path.parent().map_or_else(
            || PathBuf::from("control.json"),
            |parent| parent.join("control.json"),
        );
        Self {
            port: 0,
            database_path,
            worktree_root,
            codex_binary: PathBuf::from("codex"),
            control_descriptor_path,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDescriptor {
    pub address: SocketAddr,
    pub origin: String,
    pub session_token: String,
    pub csrf_token: String,
    pub process_id: u32,
    pub started_at: String,
}

#[derive(Clone, Debug)]
pub enum ControlDescriptorInspection {
    Missing,
    Stale,
    Live(ControlDescriptor),
}

impl std::fmt::Debug for ControlDescriptor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControlDescriptor")
            .field("address", &self.address)
            .field("origin", &self.origin)
            .field("session_token", &"<redacted>")
            .field("csrf_token", &"<redacted>")
            .field("process_id", &self.process_id)
            .field("started_at", &self.started_at)
            .finish()
    }
}

#[derive(Debug)]
pub struct ServerHandle {
    pub address: SocketAddr,
    pub launch_url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl ServerHandle {
    /// Stop the loopback server and wait for graceful shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error when the server task fails or cannot be joined.
    pub async fn shutdown(mut self) -> anyhow::Result<()> {
        if let Some(sender) = self.shutdown.take() {
            let _ignored = sender.send(());
        }
        self.task.await??;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AppState {
    store: SqliteStore,
    security: Arc<SessionSecurity>,
    events: broadcast::Sender<RunEvent>,
    worktree_root: PathBuf,
    runtime: Arc<dyn RuntimeAdapter>,
    control: RuntimeCoordinator,
    github: GitHubAdapter,
}

impl AppState {
    #[must_use]
    pub fn store(&self) -> &SqliteStore {
        &self.store
    }

    /// Persist and publish an ordered run event.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the event cannot be persisted.
    pub fn append_event(
        &self,
        run_id: &EntityId,
        kind: impl Into<String>,
        payload: Value,
    ) -> Result<RunEvent, StoreError> {
        let event = self.store.append_run_event(run_id, kind, payload)?;
        let _receiver_count = self.events.send(event.clone());
        Ok(event)
    }
}

#[derive(Debug)]
struct SessionSecurity {
    session_token: String,
    csrf_token: String,
    origin: String,
}

impl SessionSecurity {
    fn new(address: SocketAddr) -> Self {
        Self {
            session_token: random_token(),
            csrf_token: random_token(),
            origin: format!("http://{address}"),
        }
    }
}

#[derive(Debug)]
struct ControlClaim {
    path: PathBuf,
    session_token: String,
}

impl Drop for ControlClaim {
    fn drop(&mut self) {
        remove_control_descriptor_if_owned(&self.path, &self.session_token);
    }
}

async fn claim_control_descriptor(
    path: &FsPath,
    descriptor: &ControlDescriptor,
) -> anyhow::Result<ControlClaim> {
    if let Ok(source) = fs::read_to_string(path) {
        if let Ok(existing) = serde_json::from_str::<ControlDescriptor>(&source) {
            if control_descriptor_is_live(&existing).await {
                anyhow::bail!(
                    "another BA Agent control service is already listening at {}",
                    existing.address
                );
            }
        }
        fs::remove_file(path)?;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(descriptor)?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    if let Err(error) = file.write_all(&body).and_then(|()| file.sync_all()) {
        let _ignored = fs::remove_file(path);
        return Err(error.into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
            let _ignored = fs::remove_file(path);
            return Err(error.into());
        }
    }
    Ok(ControlClaim {
        path: path.to_path_buf(),
        session_token: descriptor.session_token.clone(),
    })
}

/// Return the authenticated descriptor only when it belongs to a responsive BA Agent service.
///
/// A plain TCP connection is insufficient because an unrelated process may have reused the port
/// recorded by a stale descriptor. The authenticated health response prevents opening a session
/// token against that unrelated local process.
pub async fn live_control_descriptor(path: &FsPath) -> Option<ControlDescriptor> {
    match inspect_control_descriptor(path).await {
        ControlDescriptorInspection::Live(descriptor) => Some(descriptor),
        ControlDescriptorInspection::Missing | ControlDescriptorInspection::Stale => None,
    }
}

/// Classify the local control descriptor without exposing its session credential.
pub async fn inspect_control_descriptor(path: &FsPath) -> ControlDescriptorInspection {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ControlDescriptorInspection::Missing;
        }
        Err(_) => return ControlDescriptorInspection::Stale,
    };
    let Ok(descriptor) = serde_json::from_str::<ControlDescriptor>(&source) else {
        return ControlDescriptorInspection::Stale;
    };
    if control_descriptor_is_live(&descriptor).await {
        ControlDescriptorInspection::Live(descriptor)
    } else {
        ControlDescriptorInspection::Stale
    }
}

async fn control_descriptor_is_live(descriptor: &ControlDescriptor) -> bool {
    if !descriptor.address.ip().is_loopback()
        || descriptor.origin != format!("http://{}", descriptor.address)
        || descriptor.session_token.len() != 64
        || !descriptor
            .session_token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return false;
    }
    let Ok(Ok(mut stream)) = tokio::time::timeout(
        Duration::from_millis(250),
        TcpStream::connect(descriptor.address),
    )
    .await
    else {
        return false;
    };
    let request = format!(
        "GET {API_PREFIX}/health HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        descriptor.address, descriptor.session_token
    );
    if !matches!(
        tokio::time::timeout(
            Duration::from_millis(250),
            stream.write_all(request.as_bytes()),
        )
        .await,
        Ok(Ok(()))
    ) {
        return false;
    }
    let mut response = Vec::with_capacity(1024);
    let mut limited = stream.take(8 * 1024);
    let read = limited.read_to_end(&mut response);
    if !matches!(
        tokio::time::timeout(Duration::from_millis(500), read).await,
        Ok(Ok(_))
    ) {
        return false;
    }
    response.starts_with(b"HTTP/1.1 200")
        && response
            .windows(b"\"service\":\"ba-agent-control\"".len())
            .any(|window| window == b"\"service\":\"ba-agent-control\"")
}

fn remove_control_descriptor_if_owned(path: &FsPath, session_token: &str) {
    let owned = fs::read_to_string(path)
        .ok()
        .and_then(|source| serde_json::from_str::<ControlDescriptor>(&source).ok())
        .is_some_and(|descriptor| descriptor.session_token == session_token);
    if owned {
        let _ignored = fs::remove_file(path);
    }
}

/// Start a loopback-only server with a fresh session credential.
///
/// # Errors
///
/// Returns an error when binding, database initialization, or server startup fails.
pub async fn start(config: ServerConfig) -> anyhow::Result<ServerHandle> {
    let requested_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.port);
    let listener = TcpListener::bind(requested_address).await.with_context(|| {
        if config.port == 0 {
            "unable to bind an available BA Agent loopback port".to_owned()
        } else {
            format!(
                "unable to bind BA Agent to {requested_address}; the requested local port is occupied or unavailable"
            )
        }
    })?;
    let address = listener.local_addr()?;
    if !address.ip().is_loopback() {
        anyhow::bail!("refusing non-loopback address: {address}");
    }
    let security = Arc::new(SessionSecurity::new(address));
    let launch_url = format!(
        "{}/session?token={}",
        security.origin, security.session_token
    );
    let descriptor = ControlDescriptor {
        address,
        origin: security.origin.clone(),
        session_token: security.session_token.clone(),
        csrf_token: security.csrf_token.clone(),
        process_id: std::process::id(),
        started_at: now().to_rfc3339(),
    };
    let control_claim =
        claim_control_descriptor(&config.control_descriptor_path, &descriptor).await?;
    let (events, _receiver) = broadcast::channel(1_024);
    let store = SqliteStore::open(config.database_path)?;
    ensure_builtin_agent_profiles(&store)?;
    let runtime: Arc<dyn RuntimeAdapter> = Arc::new(CodexAdapter::new(config.codex_binary));
    let control =
        RuntimeCoordinator::with_event_sender(store.clone(), runtime.clone(), events.clone())
            .with_queue(config.worktree_root.clone());
    let state = AppState {
        store: store.clone(),
        security,
        events,
        worktree_root: config.worktree_root,
        runtime: runtime.clone(),
        control: control.clone(),
        github: GitHubAdapter::default(),
    };
    let (shutdown, receiver) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let result = axum::serve(listener, router(state))
            .with_graceful_shutdown(async move {
                let _ignored = receiver.await;
            })
            .await
            .map_err(anyhow::Error::from);
        drop(control_claim);
        result
    });
    tokio::spawn(async move {
        let capabilities = match runtime.probe().await {
            Ok(capabilities) => capabilities,
            Err(error) => {
                tracing::warn!(error = %error, "startup runtime capability handshake failed");
                return;
            }
        };
        let recovery_records =
            match RecoveryCoordinator::new(store).reconcile(&BTreeSet::new(), &capabilities) {
                Ok(records) => records,
                Err(error) => {
                    tracing::warn!(error = %error, "startup runtime recovery scan failed");
                    return;
                }
            };
        for record in recovery_records {
            if matches!(record.action, RecoveryAction::Resume { .. }) {
                if let Err(error) = control.resume(&record.run_id, None).await {
                    tracing::warn!(
                        run_id = %record.run_id,
                        error = %error,
                        "automatic runtime recovery remained orphaned"
                    );
                }
            }
        }
        if let Err(error) = control.drain_queue().await {
            tracing::warn!(error = %error, "startup durable queue drain failed");
        }
    });
    Ok(ServerHandle {
        address,
        launch_url,
        shutdown: Some(shutdown),
        task,
    })
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/session", get(session_info))
        .route("/overview", get(overview))
        .route("/projects", get(list_projects).post(create_project))
        .route("/projects/onboard", post(onboard_project))
        .route("/projects/{id}", get(get_project))
        .route("/worktrees", get(list_worktrees))
        .route("/agent-profiles", get(list_agent_profiles))
        .route(
            "/workstreams",
            get(list_workstreams).post(create_workstream),
        )
        .route("/milestones", get(list_milestones).post(create_milestone))
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{id}/transition", post(transition_task))
        .route("/dispatches", post(create_dispatch))
        .route("/dispatches/queue", get(list_dispatch_queue))
        .route("/dispatches/queue/reorder", post(reorder_dispatch_queue))
        .route("/dispatches/{id}/cancel", post(cancel_dispatch))
        .route(
            "/scheduler",
            get(get_scheduler_policy).post(set_scheduler_policy),
        )
        .route("/scheduler/projects/{id}", post(set_project_concurrency))
        .route("/runs", get(list_runs))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/events", get(run_events))
        .route("/runs/{id}/message", post(message_run))
        .route("/runs/{id}/cancel", post(cancel_run))
        .route("/runs/{id}/retry", post(retry_run))
        .route("/approvals", get(list_approvals))
        .route("/approvals/{id}/respond", post(respond_approval))
        .route("/gates", get(list_gates))
        .route("/gates/configured", get(list_configured_gates))
        .route("/gates/run", post(run_gate))
        .route("/receipts", get(list_receipts))
        .route("/receipts/verify", post(verify_receipts))
        .route("/adapters", get(adapter_status))
        .route("/adapters/github/repository", get(github_repository))
        .route_layer(middleware::from_fn_with_state(state.clone(), api_security));

    Router::new()
        .route("/session", get(open_session))
        .route("/session-recovery.css", get(session_recovery_css))
        .route("/api/v1/health", get(health))
        .nest(API_PREFIX, api)
        .route("/", get(index))
        .route("/{*path}", get(asset))
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(security_headers))
}

async fn open_session(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    if query.get("token").map(String::as_str) != Some(state.security.session_token.as_str()) {
        return Ok(session_recovery_response());
    }
    let cookie = format!(
        "ba_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=28800",
        state.security.session_token
    );
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(ApiError::internal)?,
    );
    response.headers_mut().insert(
        "x-ba-csrf-token",
        HeaderValue::from_str(&state.security.csrf_token).map_err(ApiError::internal)?,
    );
    Ok(response)
}

fn session_recovery_response() -> Response {
    let body = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>会话失效 · BA Agent</title>
  <link rel="stylesheet" href="/session-recovery.css" />
</head>
<body>
  <main>
    <header><strong>BA Agent</strong><span>LOCAL CONTROL PLANE</span></header>
    <section aria-labelledby="recovery-title">
      <p class="status">SESSION INVALID · 会话失效</p>
      <h1 id="recovery-title">这个启动链接已经失效</h1>
      <p>本地服务仍然安全运行，但一次性启动凭据已更换或不正确。页面没有尝试猜测、复用或保存该凭据。</p>
      <p lang="en">This launch link is no longer valid. The local service did not reuse or store an unverified credential.</p>
      <ol>
        <li><span>01</span><div><strong>打开终端</strong><p>在任意目录打开 PowerShell、Terminal 或 shell。</p></div></li>
        <li><span>02</span><div><strong>重新打开控制台</strong><p>运行下方命令；BA Agent 会复用已验证的现有服务，或安全启动一个新服务。</p></div></li>
      </ol>
      <code>ba-agent-control ui</code>
      <p class="note">不要分享终端打印的一次性会话 URL。关闭此页不会停止正在运行的任务。</p>
    </section>
  </main>
</body>
</html>"#;
    let mut response = (StatusCode::UNAUTHORIZED, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn session_recovery_css() -> Response {
    const CSS: &str = r#":root{color-scheme:light;--paper:#f3f0e6;--raised:#f8f6ef;--deep:#e7e1d2;--ink:#202722;--muted:#58635c;--rule:#aeb4aa;--red:#a3453f;--red-soft:#f0dcda;--brass:#805217;--header-copy:#eef1e9;--sans:"Segoe UI","Microsoft YaHei UI","PingFang SC",sans-serif;--mono:"Cascadia Mono",Consolas,monospace}*{box-sizing:border-box}body{margin:0;min-width:320px;min-height:100vh;background:var(--paper);color:var(--ink);font-family:var(--sans);line-height:1.55}main{width:min(760px,calc(100% - 32px));margin:clamp(24px,8vh,72px) auto;border:1px solid var(--rule);background:var(--raised)}header{display:flex;align-items:center;justify-content:space-between;gap:18px;padding:18px 24px;background:var(--ink);color:var(--header-copy)}header strong{font-size:19px;letter-spacing:-.025em}header span,.status,code,ol>li>span{font-family:var(--mono);font-size:10px;letter-spacing:.08em}section{padding:clamp(24px,6vw,54px)}.status{display:inline-block;margin:0 0 18px;padding:6px 8px;border:1px solid var(--red);background:var(--red-soft);color:var(--red);font-weight:700}h1{max-width:18ch;margin:0 0 14px;font-size:24px;line-height:1.2;letter-spacing:-.025em;text-wrap:balance}section>p{max-width:68ch;margin:10px 0;color:var(--muted)}ol{margin:28px 0 22px;padding:0;border-block:1px solid var(--rule);list-style:none}li{display:grid;grid-template-columns:42px 1fr;gap:14px;padding:16px 0;border-bottom:1px solid color-mix(in srgb,var(--rule) 45%,transparent)}li:last-child{border-bottom:0}li span{color:var(--brass);font-weight:700}li p{margin:4px 0 0;color:var(--muted)}code{display:block;overflow-x:auto;padding:16px 18px;border:1px solid var(--ink);background:var(--deep);color:var(--ink);font-weight:700;white-space:nowrap}.note{margin-top:18px!important}::selection{background:#ead9b8;color:var(--ink)}:focus-visible{outline:3px solid var(--brass);outline-offset:3px}@media(max-width:520px){main{width:100%;margin:0;border-inline:0}header{align-items:flex-start;flex-direction:column;padding:16px 20px}section{padding:28px 20px}h1{font-size:20px}li{grid-template-columns:34px 1fr}}@media(prefers-color-scheme:dark){:root{color-scheme:dark;--paper:#19221e;--raised:#202b26;--deep:#131a17;--ink:#edf0e8;--muted:#a9b5ad;--rule:#526159;--red:#df817a;--red-soft:#4a2a29;--brass:#d5a75a}header{background:#131a17}}"#;
    let mut response = CSS.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn session_info(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "csrfToken": state.security.csrf_token,
        "origin": state.security.origin,
        "expiresInSeconds": 28_800
    }))
}

async fn api_security(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let headers = request.headers();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix("ba_session="))
        });
    if bearer != Some(state.security.session_token.as_str())
        && cookie != Some(state.security.session_token.as_str())
    {
        return ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "session required")
            .into_response();
    }
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        let origin_ok = headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            == Some(state.security.origin.as_str());
        let csrf_ok = headers
            .get("x-ba-csrf-token")
            .and_then(|value| value.to_str().ok())
            == Some(state.security.csrf_token.as_str());
        if !origin_ok || !csrf_ok {
            return ApiError::new(StatusCode::FORBIDDEN, "forbidden", "Origin or CSRF failed")
                .into_response();
        }
    }
    next.run(request).await
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; font-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn health(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    state.store.integrity_check()?;
    Ok(Json(json!({
        "status": "ok",
        "service": "ba-agent-control",
        "schemaVersion": state.store.schema_version()?,
        "telemetry": false
    })))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    projects: Vec<Project>,
    pending_approvals: usize,
    running_runs: usize,
    failed_runs: usize,
    orphaned_runs: usize,
}

async fn overview(State(state): State<AppState>) -> Result<Json<Overview>, ApiError> {
    Ok(Json(Overview {
        projects: state.store.list_projects()?,
        pending_approvals: state.store.list_pending_approvals()?.len(),
        running_runs: state
            .store
            .list_runs_by_status(ba_core::RunStatus::Running)?
            .len(),
        failed_runs: state
            .store
            .list_runs_by_status(ba_core::RunStatus::Failed)?
            .len(),
        orphaned_runs: state
            .store
            .list_runs_by_status(ba_core::RunStatus::Orphaned)?
            .len(),
    }))
}

async fn list_projects(State(state): State<AppState>) -> Result<Json<Vec<Project>>, ApiError> {
    Ok(Json(state.store.list_projects()?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProject {
    name: String,
    root_path: PathBuf,
    #[serde(default)]
    description: String,
    #[serde(default = "default_language")]
    language: String,
}

async fn create_project(
    State(state): State<AppState>,
    Json(input): Json<CreateProject>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("project name must not be empty"));
    }
    let root = input
        .root_path
        .canonicalize()
        .map_err(|error| ApiError::bad_request(format!("invalid project root: {error}")))?;
    ensure_git_root(&root)?;
    let stamp = now();
    let project = Project {
        id: EntityId::new(),
        name: input.name.trim().to_owned(),
        root_path: root.to_string_lossy().into_owned(),
        description: input.description,
        language: input.language,
        created_at: stamp,
        updated_at: stamp,
    };
    state.store.upsert_project(&project)?;
    Ok((StatusCode::CREATED, Json(project)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnboardProject {
    mode: ProjectOnboardingMode,
    path: PathBuf,
    name: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default = "default_language")]
    language: String,
}

async fn onboard_project(
    State(state): State<AppState>,
    Json(input): Json<OnboardProject>,
) -> Result<(StatusCode, Json<ba_scheduler::ProjectOnboardingResult>), ApiError> {
    if !input.path.is_absolute() {
        return Err(ApiError::bad_request("project path must be absolute"));
    }
    let result = ProjectOnboardingService::new(state.store.clone())
        .onboard(ProjectOnboardingRequest {
            mode: input.mode,
            path: input.path,
            name: input.name,
            description: input.description,
            language: input.language,
        })
        .map_err(project_onboarding_error)?;
    Ok((StatusCode::CREATED, Json(result)))
}

fn project_onboarding_error(error: SchedulerError) -> ApiError {
    match error {
        SchedulerError::ProjectAlreadyRegistered(_)
        | SchedulerError::ProjectDirectoryNotEmpty(_) => ApiError::conflict(error.to_string()),
        SchedulerError::Store(_) => ApiError::internal(error),
        _ => ApiError::bad_request(error.to_string()),
    }
}

async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Project>, ApiError> {
    state
        .store
        .get_project(&parse_id(&id)?)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("project not found"))
}

async fn list_worktrees(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Vec<Worktree>>, ApiError> {
    Ok(Json(
        state.store.list_worktrees(&parse_id(&query.project_id)?)?,
    ))
}

#[derive(Debug, Deserialize)]
struct ProjectQuery {
    project_id: String,
}

async fn list_workstreams(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Vec<Workstream>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_workstreams(&parse_id(&query.project_id)?)?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorkstream {
    project_id: String,
    name: String,
    #[serde(default)]
    description: String,
}

async fn create_workstream(
    State(state): State<AppState>,
    Json(input): Json<CreateWorkstream>,
) -> Result<(StatusCode, Json<Workstream>), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("workstream name must not be empty"));
    }
    let project_id = parse_id(&input.project_id)?;
    if state.store.get_project(&project_id)?.is_none() {
        return Err(ApiError::not_found("project not found"));
    }
    let stamp = now();
    let workstream = Workstream {
        id: EntityId::new(),
        project_id,
        name: input.name.trim().to_owned(),
        description: input.description,
        status: "active".into(),
        created_at: stamp,
        updated_at: stamp,
    };
    state.store.upsert_workstream(&workstream)?;
    Ok((StatusCode::CREATED, Json(workstream)))
}

async fn list_milestones(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Vec<Milestone>>, ApiError> {
    Ok(Json(
        state.store.list_milestones(&parse_id(&query.project_id)?)?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateMilestone {
    project_id: String,
    workstream_id: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_milestone_status")]
    status: String,
    due_at: Option<Timestamp>,
}

async fn create_milestone(
    State(state): State<AppState>,
    Json(input): Json<CreateMilestone>,
) -> Result<(StatusCode, Json<Milestone>), ApiError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("milestone name must not be empty"));
    }
    let status = input.status.trim().to_ascii_lowercase();
    if !matches!(
        status.as_str(),
        "planned" | "active" | "completed" | "cancelled"
    ) {
        return Err(ApiError::bad_request("invalid milestone status"));
    }
    let project_id = parse_id(&input.project_id)?;
    if state.store.get_project(&project_id)?.is_none() {
        return Err(ApiError::not_found("project not found"));
    }
    let workstream_id = input.workstream_id.map(|id| parse_id(&id)).transpose()?;
    if let Some(id) = &workstream_id {
        let workstream = state
            .store
            .get_workstream(id)?
            .ok_or_else(|| ApiError::not_found("workstream not found"))?;
        if workstream.project_id != project_id {
            return Err(ApiError::bad_request(
                "workstream belongs to a different project",
            ));
        }
    }
    let stamp = now();
    let milestone = Milestone {
        id: EntityId::new(),
        project_id,
        workstream_id,
        name: name.to_owned(),
        description: input.description,
        status,
        due_at: input.due_at,
        created_at: stamp,
        updated_at: stamp,
    };
    state.store.create_milestone(&milestone)?;
    Ok((StatusCode::CREATED, Json(milestone)))
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Vec<Task>>, ApiError> {
    Ok(Json(state.store.list_tasks(&parse_id(&query.project_id)?)?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTask {
    project_id: String,
    workstream_id: Option<String>,
    workstream_name: Option<String>,
    milestone_id: Option<String>,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    write: bool,
    #[serde(default)]
    path_scope: Vec<String>,
    #[serde(default)]
    depends_on: Vec<String>,
}

fn resolve_task_workstream(
    state: &AppState,
    project_id: &EntityId,
    requested_id: Option<String>,
    requested_name: Option<&str>,
    stamp: Timestamp,
) -> Result<(Option<EntityId>, Option<Workstream>), ApiError> {
    let name = requested_name
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if requested_id.is_some() && name.is_some() {
        return Err(ApiError::bad_request(
            "provide either workstreamId or workstreamName, not both",
        ));
    }
    if let Some(name) = name {
        let workstream = Workstream {
            id: EntityId::new(),
            project_id: project_id.clone(),
            name: name.to_owned(),
            description: String::new(),
            status: "active".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        return Ok((Some(workstream.id.clone()), Some(workstream)));
    }
    let id = requested_id.map(|id| parse_id(&id)).transpose()?;
    if let Some(id) = &id {
        let workstream = state
            .store
            .get_workstream(id)?
            .ok_or_else(|| ApiError::not_found("workstream not found"))?;
        if workstream.project_id != *project_id {
            return Err(ApiError::bad_request(
                "workstream belongs to a different project",
            ));
        }
    }
    Ok((id, None))
}

async fn create_task(
    State(state): State<AppState>,
    Json(input): Json<CreateTask>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    if input.title.trim().is_empty() || (input.write && input.path_scope.is_empty()) {
        return Err(ApiError::bad_request(
            "task title and write scope are required",
        ));
    }
    if !input.write && !input.path_scope.is_empty() {
        return Err(ApiError::bad_request(
            "read-only tasks cannot declare write scopes",
        ));
    }
    let project_id = parse_id(&input.project_id)?;
    if state.store.get_project(&project_id)?.is_none() {
        return Err(ApiError::not_found("project not found"));
    }
    let stamp = now();
    let (workstream_id, generated_workstream) = resolve_task_workstream(
        &state,
        &project_id,
        input.workstream_id,
        input.workstream_name.as_deref(),
        stamp,
    )?;
    let milestone_id = input.milestone_id.map(|id| parse_id(&id)).transpose()?;
    if let Some(id) = &milestone_id {
        let milestone = state
            .store
            .get_milestone(id)?
            .ok_or_else(|| ApiError::not_found("milestone not found"))?;
        if milestone.project_id != project_id {
            return Err(ApiError::bad_request(
                "milestone belongs to a different project",
            ));
        }
        if milestone.workstream_id.is_some() && milestone.workstream_id != workstream_id {
            return Err(ApiError::bad_request(
                "milestone belongs to a different workstream",
            ));
        }
    }
    for scope in &input.path_scope {
        RepoPath::new(scope)
            .map_err(|error| ApiError::bad_request(format!("invalid write scope: {error}")))?;
    }
    let depends_on = input
        .depends_on
        .into_iter()
        .map(|id| parse_id(&id))
        .collect::<Result<Vec<_>, _>>()?;
    for dependency_id in &depends_on {
        let dependency = state
            .store
            .get_task(dependency_id)?
            .ok_or_else(|| ApiError::not_found("dependency task not found"))?;
        if dependency.project_id != project_id {
            return Err(ApiError::bad_request(
                "dependency task belongs to a different project",
            ));
        }
    }
    let task = Task {
        id: EntityId::new(),
        project_id,
        workstream_id,
        milestone_id,
        title: input.title.trim().to_owned(),
        description: input.description,
        status: TaskStatus::Draft,
        write: input.write,
        path_scope: input.path_scope,
        depends_on,
        created_at: stamp,
        updated_at: stamp,
    };
    if let Some(workstream) = &generated_workstream {
        state.store.create_workstream_and_task(workstream, &task)?;
    } else {
        state.store.upsert_task(&task)?;
    }
    Ok((StatusCode::CREATED, Json(task)))
}

#[derive(Debug, Deserialize)]
struct TransitionInput {
    status: String,
}

async fn transition_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<TransitionInput>,
) -> Result<Json<Task>, ApiError> {
    let status = TaskStatus::from_str(&input.status)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let task = state.store.transition_task(&parse_id(&id)?, status)?;
    if status == TaskStatus::Completed {
        state
            .control
            .drain_queue()
            .await
            .map_err(ApiError::bad_gateway)?;
    }
    Ok(Json(task))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateDispatch {
    task_id: String,
    agent_profile_id: Option<String>,
    prompt: Option<String>,
}

async fn create_dispatch(
    State(state): State<AppState>,
    Json(input): Json<CreateDispatch>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let task_id = parse_id(&input.task_id)?;
    let profile_id = input.agent_profile_id.map(|id| parse_id(&id)).transpose()?;
    let bundle = DispatchCoordinator::new(state.store.clone(), state.worktree_root.clone())
        .enqueue_with_prompt(&task_id, profile_id.as_ref(), input.prompt)
        .map_err(|error| ApiError::conflict(error.to_string()))?;
    state
        .control
        .drain_queue()
        .await
        .map_err(ApiError::bad_gateway)?;
    let run = state.store.get_run(&bundle.run.id)?.unwrap_or(bundle.run);
    let dispatch = state
        .store
        .get_dispatch(&bundle.dispatch.id)?
        .unwrap_or(bundle.dispatch);
    let bundle = ba_scheduler::DispatchBundle {
        run,
        dispatch,
        worktree: None,
    };
    Ok((StatusCode::ACCEPTED, Json(json!(bundle))))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueResource {
    dispatch: ba_core::Dispatch,
    run: Run,
    task: Task,
}

async fn list_dispatch_queue(
    State(state): State<AppState>,
) -> Result<Json<Vec<QueueResource>>, ApiError> {
    let mut queue = Vec::new();
    for dispatch in state.store.list_dispatches_by_status("queued")? {
        let run = state
            .store
            .get_run(&dispatch.run_id)?
            .ok_or_else(|| ApiError::not_found("queued run not found"))?;
        let task = state
            .store
            .get_task(&dispatch.task_id)?
            .ok_or_else(|| ApiError::not_found("queued task not found"))?;
        queue.push(QueueResource {
            dispatch,
            run,
            task,
        });
    }
    Ok(Json(queue))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderQueueInput {
    dispatch_ids: Vec<String>,
}

async fn reorder_dispatch_queue(
    State(state): State<AppState>,
    Json(input): Json<ReorderQueueInput>,
) -> Result<Json<Vec<ba_core::Dispatch>>, ApiError> {
    let ids = input
        .dispatch_ids
        .iter()
        .map(|id| parse_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(state.store.reorder_queued_dispatches(&ids)?))
}

async fn cancel_dispatch(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let dispatch = state
        .store
        .get_dispatch(&parse_id(&id)?)?
        .ok_or_else(|| ApiError::not_found("dispatch not found"))?;
    let (run, event) = state
        .control
        .cancel(&dispatch.run_id, "local_user")
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(
        json!({"dispatchId": dispatch.id, "run": run, "event": event}),
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SchedulerPolicyResource {
    global_max_concurrency: usize,
    projects: Vec<ba_scheduler::ProjectConcurrency>,
}

fn scheduler_policy_resource(state: &AppState) -> Result<SchedulerPolicyResource, ApiError> {
    let service = SchedulerPolicyService::new(state.store.clone());
    let global = service
        .global()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let projects = state
        .store
        .list_projects()?
        .iter()
        .map(|project| service.project(project))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(SchedulerPolicyResource {
        global_max_concurrency: global.global_max_concurrency,
        projects,
    })
}

async fn get_scheduler_policy(
    State(state): State<AppState>,
) -> Result<Json<SchedulerPolicyResource>, ApiError> {
    Ok(Json(scheduler_policy_resource(&state)?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetConcurrencyInput {
    max_concurrency: usize,
}

async fn set_scheduler_policy(
    State(state): State<AppState>,
    Json(input): Json<SetConcurrencyInput>,
) -> Result<Json<SchedulerPolicyResource>, ApiError> {
    SchedulerPolicyService::new(state.store.clone())
        .set_global(input.max_concurrency)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    state
        .control
        .drain_queue()
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(scheduler_policy_resource(&state)?))
}

async fn set_project_concurrency(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<SetConcurrencyInput>,
) -> Result<Json<ba_scheduler::ProjectConcurrency>, ApiError> {
    let project = state
        .store
        .get_project(&parse_id(&id)?)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    let result = SchedulerPolicyService::new(state.store.clone())
        .set_project(&project, input.max_concurrency)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    state
        .control
        .drain_queue()
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct RunQuery {
    task_id: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunResource {
    #[serde(flatten)]
    run: Run,
    event_sequence: u64,
}

fn run_resource(state: &AppState, run: Run) -> Result<RunResource, ApiError> {
    let event_sequence = state.store.latest_run_event_sequence(&run.id)?;
    Ok(RunResource {
        run,
        event_sequence,
    })
}

async fn list_runs(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> Result<Json<Vec<RunResource>>, ApiError> {
    let runs = if let Some(id) = query.task_id {
        state.store.list_runs(&parse_id(&id)?)?
    } else if let Some(id) = query.project_id {
        state.store.list_project_runs(&parse_id(&id)?)?
    } else {
        return Err(ApiError::bad_request("task_id or project_id is required"));
    };
    Ok(Json(
        runs.into_iter()
            .map(|run| run_resource(&state, run))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunResource>, ApiError> {
    state
        .store
        .get_run(&parse_id(&id)?)?
        .map(|run| run_resource(&state, run).map(Json))
        .transpose()?
        .ok_or_else(|| ApiError::not_found("run not found"))
}

#[derive(Debug, Deserialize)]
struct RunMessageInput {
    message: String,
}

async fn message_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<RunMessageInput>,
) -> Result<(StatusCode, Json<RunEvent>), ApiError> {
    if input.message.trim().is_empty() {
        return Err(ApiError::bad_request("message must not be empty"));
    }
    let run_id = parse_id(&id)?;
    let event = state
        .control
        .message(&run_id, &input.message)
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok((StatusCode::ACCEPTED, Json(event)))
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let run_id = parse_id(&id)?;
    let (run, event) = state
        .control
        .cancel(&run_id, "local_user")
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(json!({"run": run, "event": event})))
}

async fn retry_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let run_id = parse_id(&id)?;
    let previous = state
        .store
        .get_run(&run_id)?
        .ok_or_else(|| ApiError::not_found("run not found"))?;

    if previous.status == RunStatus::Orphaned {
        let run = state
            .control
            .resume(&run_id, None)
            .await
            .map_err(ApiError::bad_gateway)?;
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({"mode":"resumed","run":run})),
        ));
    }
    if previous.status != RunStatus::Failed {
        return Err(ApiError::conflict(
            "only failed or orphaned runs can be retried",
        ));
    }

    let task = state
        .store
        .get_task(&previous.task_id)?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    match task.status {
        TaskStatus::Failed | TaskStatus::Blocked => {
            state.store.transition_task(&task.id, TaskStatus::Ready)?;
        }
        TaskStatus::Ready => {}
        _ => {
            return Err(ApiError::conflict(format!(
                "task {} cannot be retried from {}",
                task.id, task.status
            )));
        }
    }
    let bundle = DispatchCoordinator::new(state.store.clone(), state.worktree_root.clone())
        .enqueue(&task.id, previous.agent_profile_id.as_ref())
        .map_err(|error| ApiError::conflict(error.to_string()))?;
    state
        .control
        .drain_queue()
        .await
        .map_err(ApiError::bad_gateway)?;
    let run = state.store.get_run(&bundle.run.id)?.unwrap_or(bundle.run);
    let dispatch = state
        .store
        .get_dispatch(&bundle.dispatch.id)?
        .unwrap_or(bundle.dispatch);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"mode":"retried","run":run,"dispatch":dispatch})),
    ))
}

#[derive(Debug, Deserialize)]
struct EventQuery {
    #[serde(default)]
    after: u64,
}

async fn run_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let run_id = parse_id(&id)?;
    if state.store.get_run(&run_id)?.is_none() {
        return Err(ApiError::not_found("run not found"));
    }
    let initial = tokio_stream::iter(
        state
            .store
            .list_run_events(&run_id, query.after)?
            .into_iter()
            .map(|event| Ok(event_to_sse(&event))),
    );
    let target = run_id.clone();
    let live =
        BroadcastStream::new(state.events.subscribe()).filter_map(move |result| match result {
            Ok(event) if event.run_id == target && event.sequence > query.after => {
                Some(Ok(event_to_sse(&event)))
            }
            _ => None,
        });
    Ok(Sse::new(initial.chain(live)).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

fn event_to_sse(event: &RunEvent) -> Event {
    let data = serde_json::to_string(&event)
        .unwrap_or_else(|error| json!({"error": error.to_string()}).to_string());
    Event::default()
        .id(event.sequence.to_string())
        .event("run-event")
        .data(data)
}

async fn list_approvals(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.store.list_pending_approvals()?)))
}

#[derive(Debug, Deserialize)]
struct ApprovalInput {
    decision: String,
    #[serde(default)]
    reason: String,
}

async fn respond_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<ApprovalInput>,
) -> Result<Json<Value>, ApiError> {
    let (resolution, runtime_decision) = match input.decision.as_str() {
        "approved" => (ApprovalResolution::Approve, RuntimeApprovalDecision::Accept),
        "approved_for_session" => (
            ApprovalResolution::Approve,
            RuntimeApprovalDecision::AcceptForSession,
        ),
        "rejected" | "denied" => (ApprovalResolution::Reject, RuntimeApprovalDecision::Decline),
        "cancelled" => (ApprovalResolution::Cancel, RuntimeApprovalDecision::Cancel),
        _ => return Err(ApiError::bad_request("unknown approval decision")),
    };
    let approval_id = parse_id(&id)?;
    let evidence = json!({
        "reason": input.reason,
        "actor": "local_user"
    });
    let (approval, event) = state
        .control
        .respond_approval(&approval_id, resolution, runtime_decision, &evidence)
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(json!({"approval": approval, "event": event})))
}

#[derive(Debug, Deserialize)]
struct TaskQuery {
    task_id: String,
}

async fn list_gates(
    State(state): State<AppState>,
    Query(query): Query<TaskQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        state.store.list_gates(&parse_id(&query.task_id)?)?
    )))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfiguredGateResource {
    name: String,
    program: String,
    args: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfiguredGatesResource {
    project_id: String,
    gates: Vec<ConfiguredGateResource>,
    unavailable_reason: Option<String>,
}

async fn list_configured_gates(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<ConfiguredGatesResource>, ApiError> {
    let project_id = parse_id(&query.project_id)?;
    let project = state
        .store
        .get_project(&project_id)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    let project_root = PathBuf::from(&project.root_path)
        .canonicalize()
        .map_err(ApiError::internal)?;
    let names = match configured_gate_names(&project_root) {
        Ok(names) => names,
        Err(error) => {
            return Ok(Json(ConfiguredGatesResource {
                project_id: project_id.to_string(),
                gates: Vec::new(),
                unavailable_reason: Some(error.to_string()),
            }));
        }
    };
    let gates = names
        .into_iter()
        .map(|name| {
            configured_gate(&project_root, &name).map(|gate| ConfiguredGateResource {
                name: gate.name,
                program: gate.program,
                args: gate.args,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::internal)?;
    Ok(Json(ConfiguredGatesResource {
        project_id: project_id.to_string(),
        gates,
        unavailable_reason: None,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunGateInput {
    task_id: String,
    run_id: Option<String>,
    gate: String,
}

async fn run_gate(
    State(state): State<AppState>,
    Json(input): Json<RunGateInput>,
) -> Result<Json<Value>, ApiError> {
    let task_id = parse_id(&input.task_id)?;
    let task = state
        .store
        .get_task(&task_id)?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    let project = state
        .store
        .get_project(&task.project_id)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    let project_root = PathBuf::from(&project.root_path)
        .canonicalize()
        .map_err(ApiError::internal)?;
    let definition = configured_gate(&project_root, &input.gate)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let run_id = input.run_id.map(|id| parse_id(&id)).transpose()?;
    if let Some(id) = &run_id {
        let run = state
            .store
            .get_run(id)?
            .ok_or_else(|| ApiError::not_found("run not found"))?;
        if run.task_id != task_id {
            return Err(ApiError::bad_request("run belongs to a different task"));
        }
    }
    let gate_cwd = gate_working_directory(&state, &task, run_id.as_ref(), &project_root)?;
    let gate = GateCoordinator::new(state.store.clone())
        .run(&task_id, run_id.as_ref(), &gate_cwd, &definition)
        .await
        .map_err(ApiError::internal)?;
    if let Some(run_id) = &run_id {
        let _event = state.append_event(run_id, "gate.completed", json!({"gate": &gate}))?;
    }
    let finalization =
        FinalizationCoordinator::new(state.store.clone(), state.worktree_root.clone())
            .finalize_if_eligible(&task_id, run_id.as_ref())
            .map_err(ApiError::internal)?;
    if let (Some(run_id), Some(finalization)) = (&run_id, &finalization) {
        let event = if finalization.git.is_some() {
            "worktree.committed"
        } else {
            "task.completed"
        };
        let _event = state.append_event(run_id, event, json!({"finalization": finalization}))?;
    }
    if finalization.is_some() {
        state
            .control
            .drain_queue()
            .await
            .map_err(ApiError::bad_gateway)?;
    }
    Ok(Json(json!({"gate": gate, "finalization": finalization})))
}

fn gate_working_directory(
    state: &AppState,
    task: &Task,
    run_id: Option<&EntityId>,
    project_root: &FsPath,
) -> Result<PathBuf, ApiError> {
    if !task.write {
        return Ok(project_root.to_path_buf());
    }
    let run_id = run_id.ok_or_else(|| {
        ApiError::bad_request("write-task quality gates require the originating run_id")
    })?;
    let worktree = state
        .store
        .list_worktrees(&task.project_id)?
        .into_iter()
        .find(|worktree| worktree.task_id == task.id && worktree.run_id.as_ref() == Some(run_id))
        .ok_or_else(|| ApiError::not_found("worktree for run not found"))?;
    PathBuf::from(worktree.path)
        .canonicalize()
        .map_err(|error| ApiError::internal(format!("worktree is unavailable: {error}")))
}

async fn list_receipts(
    State(state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        state.store.list_receipts(&parse_id(&query.project_id)?)?
    )))
}

async fn verify_receipts(
    State(state): State<AppState>,
    Json(query): Json<ProjectQuery>,
) -> Result<Json<Value>, ApiError> {
    state
        .store
        .verify_receipt_chain(&parse_id(&query.project_id)?)?;
    Ok(Json(json!({"valid": true})))
}

async fn adapter_status(State(state): State<AppState>) -> Json<Value> {
    let github = state.github.probe().await;
    let codex = match state.runtime.probe().await {
        Ok(capabilities) => json!(capabilities),
        Err(error) => json!({
            "adapter": "codex",
            "available": false,
            "authenticated": false,
            "version": null,
            "minimum_version": ba_runtime::MINIMUM_CODEX_VERSION,
            "app_server": false,
            "exec_json_fallback": false,
            "approvals": false,
            "resume": false,
            "guide": false,
            "cancellation": false,
            "degraded_reason": error.to_string(),
        }),
    };
    Json(json!({"adapters": [
        {"id":"codex","minimumVersion":"0.130.0","preferredTransport":"app-server","fallbackTransport":"exec-json","credentialsStored":false,"capabilities":codex},
        {"id":"claude","mode":"configuration-only","liveDispatch":false},
        {"id":"github","mode":"optional-read-only","status":github}
    ]}))
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoryQuery {
    repository: String,
}

async fn github_repository(
    State(state): State<AppState>,
    Query(query): Query<GitHubRepositoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let repository = state
        .github
        .view_repository(&query.repository)
        .await
        .map_err(|error| match error {
            GitHubError::InvalidRepository(_) => ApiError::bad_request(error.to_string()),
            GitHubError::AuthenticationUnavailable => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "github_auth_unavailable",
                error.to_string(),
            ),
            _ => ApiError::new(StatusCode::BAD_GATEWAY, "github_error", error.to_string()),
        })?;
    Ok(Json(json!(repository)))
}

async fn list_agent_profiles(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.store.list_agent_profiles()?)))
}

async fn index() -> Response {
    embedded_response("index.html")
}

async fn asset(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/');
    if path.split('/').any(|part| part == "." || part == "..") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    embedded_asset(path).map_or_else(
        || embedded_response("index.html"),
        |(bytes, mime)| bytes_response(bytes, mime),
    )
}

fn embedded_response(path: &str) -> Response {
    embedded_asset(path).map_or_else(
        || StatusCode::NOT_FOUND.into_response(),
        |(bytes, mime)| bytes_response(bytes, mime),
    )
}

fn bytes_response(bytes: &'static [u8], mime: &'static str) -> Response {
    let mut response = bytes.to_vec().into_response();
    if let Ok(value) = HeaderValue::from_str(mime) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
}

fn ensure_git_root(root: &FsPath) -> Result<(), ApiError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(ApiError::internal)?;
    if !output.status.success() {
        return Err(ApiError::bad_request(
            "project root is not a Git repository",
        ));
    }
    let actual = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        .canonicalize()
        .map_err(ApiError::internal)?;
    if actual != root {
        return Err(ApiError::bad_request(format!(
            "project path must be the repository root: {}",
            actual.display()
        )));
    }
    Ok(())
}

fn parse_id(value: &str) -> Result<EntityId, ApiError> {
    EntityId::from_str(value).map_err(|error| ApiError::bad_request(error.to_string()))
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn default_language() -> String {
    "zh-CN".into()
}

fn default_milestone_status() -> String {
    "planned".into()
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
    fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }
    fn bad_gateway(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "runtime_error", error.to_string())
    }
    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            error.to_string(),
        )
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        Self::internal(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"error":{"code":self.code,"message":self.message}})),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use ba_core::{Run, RunStatus, WorktreeStatus};
    use ba_runtime::{
        ApprovalDecision, CodexVersion, MINIMUM_CODEX_VERSION, ResumeRequest, RunRequest,
        RuntimeAdapter, RuntimeCapabilities, RuntimeError, RuntimeEvent, RuntimeEventKind,
        RuntimeRunHandle, RuntimeTransport, SandboxMode,
    };
    use tempfile::tempdir;
    use tower::ServiceExt;

    #[derive(Debug)]
    struct MockRuntime {
        events: broadcast::Sender<RuntimeEvent>,
    }

    impl MockRuntime {
        fn new() -> Self {
            let (events, _receiver) = broadcast::channel(16);
            Self { events }
        }
    }

    #[async_trait::async_trait]
    impl RuntimeAdapter for MockRuntime {
        async fn probe(&self) -> Result<RuntimeCapabilities, RuntimeError> {
            Ok(RuntimeCapabilities {
                adapter: "mock".into(),
                available: true,
                authenticated: true,
                version: Some(CodexVersion::new(0, 130, 0)),
                minimum_version: MINIMUM_CODEX_VERSION,
                app_server: true,
                exec_json_fallback: true,
                approvals: true,
                resume: true,
                guide: true,
                cancellation: true,
                degraded_reason: None,
            })
        }

        async fn launch(&self, request: RunRequest) -> Result<RuntimeRunHandle, RuntimeError> {
            if request
                .environment
                .get("BA_AGENT_MANAGED_RUN")
                .map(String::as_str)
                != Some("1")
                || request.sandbox != SandboxMode::ReadOnly
            {
                return Err(RuntimeError::Protocol(
                    "managed runtime policy was not applied".into(),
                ));
            }
            let _receivers = self.events.send(RuntimeEvent {
                sequence: 1,
                run_id: "mock-runtime-run".into(),
                kind: RuntimeEventKind::Lifecycle,
                name: "succeeded".into(),
                payload: json!({"status": 0}),
            });
            Ok(RuntimeRunHandle {
                run_id: "mock-runtime-run".into(),
                runtime_thread_id: Some("mock-thread".into()),
                transport: RuntimeTransport::AppServer,
                degraded_reason: None,
            })
        }

        async fn resume(&self, request: ResumeRequest) -> Result<RuntimeRunHandle, RuntimeError> {
            self.launch(RunRequest {
                prompt: request.prompt,
                cwd: request.cwd,
                sandbox: request.sandbox,
                environment: request.environment,
            })
            .await
        }

        async fn guide(&self, _run_id: &str, _prompt: String) -> Result<(), RuntimeError> {
            Ok(())
        }

        async fn cancel(&self, _run_id: &str) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> {
            self.events.subscribe()
        }

        async fn respond_approval(
            &self,
            _run_id: &str,
            _request_id: &str,
            _decision: ApprovalDecision,
        ) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    fn state() -> anyhow::Result<AppState> {
        let directory = tempdir()?.keep();
        let store = SqliteStore::open(directory.join("ba-agent.db"))?;
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41415);
        let (events, _receiver) = broadcast::channel(16);
        let runtime: Arc<dyn RuntimeAdapter> = Arc::new(MockRuntime::new());
        let control =
            RuntimeCoordinator::with_event_sender(store.clone(), runtime.clone(), events.clone());
        Ok(AppState {
            store,
            security: Arc::new(SessionSecurity::new(address)),
            events,
            worktree_root: directory.join("worktrees"),
            runtime,
            control,
            github: GitHubAdapter::default(),
        })
    }

    #[tokio::test]
    async fn adapter_status_includes_live_runtime_authentication() -> anyhow::Result<()> {
        let Json(body) = adapter_status(State(state()?)).await;
        let codex = body
            .pointer("/adapters/0/capabilities")
            .ok_or_else(|| anyhow::anyhow!("Codex capabilities missing"))?;

        assert_eq!(codex["available"], true);
        assert_eq!(codex["authenticated"], true);
        assert_eq!(codex["app_server"], true);
        Ok(())
    }

    #[tokio::test]
    async fn configured_gates_are_discoverable_without_breaking_uninitialized_projects()
    -> anyhow::Result<()> {
        let state = state()?;
        let repository = tempdir()?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "Configured gates".into(),
            root_path: repository.path().to_string_lossy().into_owned(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_project(&project)?;

        let Json(uninitialized) = list_configured_gates(
            State(state.clone()),
            Query(ProjectQuery {
                project_id: project.id.to_string(),
            }),
        )
        .await?;
        assert!(uninitialized.gates.is_empty());
        assert!(uninitialized.unavailable_reason.is_some());

        fs::create_dir_all(repository.path().join(".ba-agent"))?;
        fs::write(
            repository.path().join(".ba-agent/project.toml"),
            "[gates]\nlint = [\"cargo\", \"check\"]\n",
        )?;
        let Json(configured) = list_configured_gates(
            State(state),
            Query(ProjectQuery {
                project_id: project.id.to_string(),
            }),
        )
        .await?;
        assert!(configured.unavailable_reason.is_none());
        assert_eq!(configured.gates.len(), 1);
        assert_eq!(configured.gates[0].name, "lint");
        assert_eq!(configured.gates[0].program, "cargo");
        assert_eq!(configured.gates[0].args, ["check"]);
        Ok(())
    }

    #[test]
    fn write_task_gates_use_the_originating_run_worktree() -> anyhow::Result<()> {
        let state = state()?;
        let repository = tempdir()?;
        let worktree = tempdir()?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "Worktree gate".into(),
            root_path: repository.path().to_string_lossy().into_owned(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_project(&project)?;
        let task = Task {
            id: EntityId::new(),
            project_id: project.id.clone(),
            workstream_id: None,
            milestone_id: None,
            title: "Write safely".into(),
            description: String::new(),
            status: TaskStatus::Review,
            write: true,
            path_scope: vec!["src/**".into()],
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_task(&task)?;
        let run = Run {
            id: EntityId::new(),
            task_id: task.id.clone(),
            agent_profile_id: None,
            status: RunStatus::Succeeded,
            adapter: "mock".into(),
            runtime_run_id: Some("runtime".into()),
            runtime_thread_id: Some("thread".into()),
            started_at: Some(stamp),
            finished_at: Some(stamp),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_run(&run)?;
        state.store.upsert_worktree(&Worktree {
            id: EntityId::new(),
            project_id: project.id,
            task_id: task.id.clone(),
            run_id: Some(run.id.clone()),
            path: worktree.path().to_string_lossy().into_owned(),
            base_revision: None,
            write_scope: vec!["src/**".into()],
            branch: None,
            status: WorktreeStatus::Active,
            created_at: stamp,
            updated_at: stamp,
        })?;

        let cwd = gate_working_directory(&state, &task, Some(&run.id), repository.path())?;
        assert_eq!(cwd, worktree.path().canonicalize()?);
        Ok(())
    }

    #[tokio::test]
    async fn health_has_security_headers() -> anyhow::Result<()> {
        let response = router(state()?)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .contains_key(header::CONTENT_SECURITY_POLICY)
        );
        Ok(())
    }

    #[test]
    fn production_console_assets_are_embedded_instead_of_the_fallback() -> anyhow::Result<()> {
        let (index, mime) = embedded_asset("index.html")
            .ok_or_else(|| anyhow::anyhow!("embedded index is missing"))?;
        let index = std::str::from_utf8(index)?;
        assert_eq!(mime, "text/html; charset=utf-8");
        assert!(index.contains("/assets/index-"));
        assert!(!index.contains("控制台资源尚未构建"));
        let script_start = index
            .find("src=\"/assets/")
            .ok_or_else(|| anyhow::anyhow!("hashed script path is missing"))?
            + 5;
        let script_end = index[script_start..]
            .find('"')
            .ok_or_else(|| anyhow::anyhow!("hashed script path is unterminated"))?
            + script_start;
        let script = &index[script_start..script_end];
        assert!(embedded_asset(script.trim_start_matches('/')).is_some());
        Ok(())
    }

    #[tokio::test]
    async fn sse_uses_a_stable_envelope_for_arbitrary_domain_event_kinds() -> anyhow::Result<()> {
        let event = RunEvent {
            run_id: EntityId::new(),
            sequence: 7,
            kind: "runtime.future_protocol_event".into(),
            payload: json!({"value": 1}),
            created_at: now(),
        };
        let response = Sse::new(tokio_stream::iter([Ok::<Event, Infallible>(event_to_sse(
            &event,
        ))]))
        .into_response();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let body = std::str::from_utf8(&body)?;
        assert!(body.contains("event: run-event"));
        assert!(body.contains("runtime.future_protocol_event"));
        assert!(body.contains("id: 7"));
        Ok(())
    }

    #[tokio::test]
    async fn control_descriptor_is_single_owner_and_removed_on_drop() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("control.json");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let session_token = "a".repeat(64);
        let descriptor = ControlDescriptor {
            address,
            origin: format!("http://{address}"),
            session_token,
            csrf_token: "csrf-fixture".into(),
            process_id: std::process::id(),
            started_at: now().to_rfc3339(),
        };
        let claim = claim_control_descriptor(&path, &descriptor).await?;
        assert!(path.is_file());
        let responder = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut request = [0_u8; 2048];
            let _read = stream.read(&mut request).await?;
            let body = r#"{"service":"ba-agent-control"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await?;
            anyhow::Ok(())
        });
        assert!(claim_control_descriptor(&path, &descriptor).await.is_err());
        responder.await??;
        drop(claim);
        assert!(!path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn live_descriptor_requires_an_authenticated_ba_agent_health_response()
    -> anyhow::Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("control.json");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let descriptor = ControlDescriptor {
            address,
            origin: format!("http://{address}"),
            session_token: "b".repeat(64),
            csrf_token: "csrf-fixture".into(),
            process_id: std::process::id(),
            started_at: now().to_rfc3339(),
        };
        fs::write(&path, serde_json::to_vec(&descriptor)?)?;
        let responder = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut request = [0_u8; 2048];
            let _read = stream.read(&mut request).await?;
            let body = r#"{"service":"another-local-process"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await?;
            anyhow::Ok(())
        });
        assert!(live_control_descriptor(&path).await.is_none());
        responder.await??;
        Ok(())
    }

    #[tokio::test]
    async fn descriptor_inspection_distinguishes_stopped_and_stale() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("control.json");
        assert!(matches!(
            inspect_control_descriptor(&path).await,
            ControlDescriptorInspection::Missing
        ));
        fs::write(&path, b"not-json")?;
        assert!(matches!(
            inspect_control_descriptor(&path).await,
            ControlDescriptorInspection::Stale
        ));
        Ok(())
    }

    #[tokio::test]
    async fn live_descriptor_reopens_the_authenticated_service() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let mut config = ServerConfig::new(directory.path().join("state.sqlite3"));
        config.control_descriptor_path = directory.path().join("control.json");
        let server = start(config).await?;
        let descriptor = live_control_descriptor(&directory.path().join("control.json"))
            .await
            .ok_or_else(|| anyhow::anyhow!("running service was not detected"))?;
        assert_eq!(descriptor.address, server.address);
        assert!(server.launch_url.ends_with(&descriptor.session_token));
        server.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn resources_require_session() -> anyhow::Result<()> {
        let response = router(state()?)
            .oneshot(
                Request::builder()
                    .uri("/api/v1/projects")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        Ok(())
    }

    #[tokio::test]
    async fn invalid_launch_token_returns_an_accessible_recovery_page() -> anyhow::Result<()> {
        let application = router(state()?);
        let response = application
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/session?token=invalid")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/html; charset=utf-8"))
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );
        assert!(
            response
                .headers()
                .contains_key(header::CONTENT_SECURITY_POLICY)
        );
        let body = to_bytes(response.into_body(), 32 * 1024).await?;
        let body = std::str::from_utf8(&body)?;
        assert!(body.contains("这个启动链接已经失效"));
        assert!(body.contains("This launch link is no longer valid"));
        assert!(body.contains("ba-agent-control ui"));
        assert!(body.contains("href=\"/session-recovery.css\""));

        let stylesheet = application
            .oneshot(
                Request::builder()
                    .uri("/session-recovery.css")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(stylesheet.status(), StatusCode::OK);
        assert_eq!(
            stylesheet.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/css; charset=utf-8"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn explicit_occupied_port_has_an_actionable_error() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let mut config = ServerConfig::new(directory.path().join("state.sqlite3"));
        config.port = listener.local_addr()?.port();
        config.control_descriptor_path = directory.path().join("control.json");
        let error = match start(config).await {
            Ok(server) => {
                server.shutdown().await?;
                anyhow::bail!("the occupied port was unexpectedly accepted");
            }
            Err(error) => error,
        };
        let message = format!("{error:#}");
        assert!(message.contains("requested local port is occupied or unavailable"));
        Ok(())
    }

    #[tokio::test]
    async fn authenticated_session_exposes_csrf_and_mutations_require_it() -> anyhow::Result<()> {
        let state = state()?;
        let session = state.security.session_token.clone();
        let csrf = state.security.csrf_token.clone();
        let origin = state.security.origin.clone();
        let cookie = format!("ba_session={session}");
        let application = router(state);

        let response = application
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/session")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 16 * 1024).await?;
        let body: Value = serde_json::from_slice(&body)?;
        assert_eq!(body["csrfToken"], csrf);

        let project_id = EntityId::new();
        let payload = serde_json::to_vec(&json!({"project_id": project_id}))?;
        let denied = application
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/receipts/verify")
                    .header(header::COOKIE, &cookie)
                    .header(header::ORIGIN, &origin)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.clone()))?,
            )
            .await?;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);

        let worktrees = application
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/worktrees?project_id={project_id}"))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(worktrees.status(), StatusCode::OK);
        let body = to_bytes(worktrees.into_body(), 16 * 1024).await?;
        let body: Value = serde_json::from_slice(&body)?;
        assert_eq!(body, json!([]));

        let allowed = application
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/receipts/verify")
                    .header(header::COOKIE, cookie)
                    .header(header::ORIGIN, origin)
                    .header("x-ba-csrf-token", csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload))?,
            )
            .await?;
        assert_eq!(allowed.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn project_onboarding_endpoint_creates_a_local_only_repository() -> anyhow::Result<()> {
        let state = state()?;
        let parent = tempdir()?;
        let target = parent.path().join("guided-project");
        let (status, Json(result)) = onboard_project(
            State(state.clone()),
            Json(OnboardProject {
                mode: ProjectOnboardingMode::Create,
                path: target.clone(),
                name: Some("Guided project".into()),
                description: "Created from the local console".into(),
                language: "en".into(),
            }),
        )
        .await?;
        assert_eq!(status, StatusCode::CREATED);
        assert!(result.created_directory);
        assert!(result.initialized_git);
        assert!(result.created_project_config);
        assert!(target.join(".git").is_dir());
        assert!(target.join(".ba-agent/project.toml").is_file());
        assert_eq!(state.store.list_projects()?.len(), 1);

        let duplicate = onboard_project(
            State(state),
            Json(OnboardProject {
                mode: ProjectOnboardingMode::Import,
                path: target,
                name: None,
                description: String::new(),
                language: "en".into(),
            }),
        )
        .await;
        let Err(error) = duplicate else {
            anyhow::bail!("duplicate project registration unexpectedly succeeded");
        };
        assert_eq!(error.into_response().status(), StatusCode::CONFLICT);
        Ok(())
    }

    #[tokio::test]
    async fn task_creation_can_atomically_create_its_workstream() -> anyhow::Result<()> {
        let state = state()?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "Atomic project".into(),
            root_path: "C:/repository".into(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_project(&project)?;

        let (status, Json(task)) = create_task(
            State(state.clone()),
            Json(CreateTask {
                project_id: project.id.to_string(),
                workstream_id: None,
                workstream_name: Some("Console".into()),
                milestone_id: None,
                title: "Create an atomic task".into(),
                description: String::new(),
                write: false,
                path_scope: Vec::new(),
                depends_on: Vec::new(),
            }),
        )
        .await?;

        assert_eq!(status, StatusCode::CREATED);
        let workstreams = state.store.list_workstreams(&project.id)?;
        assert_eq!(workstreams.len(), 1);
        assert_eq!(task.workstream_id.as_ref(), Some(&workstreams[0].id));

        let read_scope_error = create_task(
            State(state.clone()),
            Json(CreateTask {
                project_id: project.id.to_string(),
                workstream_id: None,
                workstream_name: None,
                milestone_id: None,
                title: "Invalid read scope".into(),
                description: String::new(),
                write: false,
                path_scope: vec!["apps/console/**".into()],
                depends_on: Vec::new(),
            }),
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("read-only write scope was accepted"))?;
        assert_eq!(read_scope_error.status, StatusCode::BAD_REQUEST);

        let foreign_project = Project {
            id: EntityId::new(),
            name: "Foreign project".into(),
            root_path: "C:/foreign-repository".into(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_project(&foreign_project)?;
        let foreign_task = Task {
            id: EntityId::new(),
            project_id: foreign_project.id,
            workstream_id: None,
            milestone_id: None,
            title: "Foreign dependency".into(),
            description: String::new(),
            status: TaskStatus::Ready,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_task(&foreign_task)?;
        let dependency_error = create_task(
            State(state),
            Json(CreateTask {
                project_id: project.id.to_string(),
                workstream_id: None,
                workstream_name: None,
                milestone_id: None,
                title: "Invalid dependency".into(),
                description: String::new(),
                write: false,
                path_scope: Vec::new(),
                depends_on: vec![foreign_task.id.to_string()],
            }),
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("cross-project dependency was accepted"))?;
        assert_eq!(dependency_error.status, StatusCode::BAD_REQUEST);
        Ok(())
    }

    #[tokio::test]
    async fn milestone_resource_persists_real_project_progress_boundaries() -> anyhow::Result<()> {
        let state = state()?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "Milestone project".into(),
            root_path: "C:/milestone-project".into(),
            description: "this is not a milestone".into(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        let other_project = Project {
            id: EntityId::new(),
            name: "Other project".into(),
            root_path: "C:/other-project".into(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_project(&project)?;
        state.store.upsert_project(&other_project)?;
        let foreign_workstream = Workstream {
            id: EntityId::new(),
            project_id: other_project.id,
            name: "Foreign".into(),
            description: String::new(),
            status: "active".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        state.store.upsert_workstream(&foreign_workstream)?;

        let invalid = create_milestone(
            State(state.clone()),
            Json(CreateMilestone {
                project_id: project.id.to_string(),
                workstream_id: Some(foreign_workstream.id.to_string()),
                name: "Invalid link".into(),
                description: String::new(),
                status: "planned".into(),
                due_at: None,
            }),
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("cross-project milestone was accepted"))?;
        assert_eq!(invalid.status, StatusCode::BAD_REQUEST);

        let due_at = stamp + Duration::from_secs(7 * 24 * 60 * 60);
        let (status, Json(milestone)) = create_milestone(
            State(state.clone()),
            Json(CreateMilestone {
                project_id: project.id.to_string(),
                workstream_id: None,
                name: "Personal Alpha".into(),
                description: "real milestone resource".into(),
                status: "active".into(),
                due_at: Some(due_at),
            }),
        )
        .await?;
        assert_eq!(status, StatusCode::CREATED);
        let Json(milestones) = list_milestones(
            State(state),
            Query(ProjectQuery {
                project_id: project.id.to_string(),
            }),
        )
        .await?;
        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[0].id, milestone.id);
        assert_eq!(milestones[0].name, "Personal Alpha");
        assert_eq!(milestones[0].due_at, Some(due_at));
        Ok(())
    }

    #[tokio::test]
    async fn dispatch_launch_persists_runtime_identity_and_terminal_state() -> anyhow::Result<()> {
        let directory = tempdir()?.keep();
        let store = SqliteStore::open(directory.join("ba-agent.db"))?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "runtime-test".into(),
            root_path: directory.to_string_lossy().into_owned(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        let task = Task {
            id: EntityId::new(),
            project_id: project.id.clone(),
            workstream_id: None,
            milestone_id: None,
            title: "Inspect the repository".into(),
            description: "Return evidence".into(),
            status: TaskStatus::Draft,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_task(&task)?;
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41416);
        let (events, _receiver) = broadcast::channel(16);
        let control = RuntimeCoordinator::with_event_sender(
            store.clone(),
            Arc::new(MockRuntime::new()),
            events.clone(),
        );
        let state = AppState {
            store: store.clone(),
            security: Arc::new(SessionSecurity::new(address)),
            events,
            worktree_root: directory.join("worktrees"),
            runtime: Arc::new(MockRuntime::new()),
            control,
            github: GitHubAdapter::default(),
        };
        let bundle = DispatchCoordinator::new(store.clone(), state.worktree_root.clone())
            .enqueue(&task.id, None)?;
        let launched = state.control.launch(bundle, None).await?;
        assert_eq!(
            launched.run.runtime_run_id.as_deref(),
            Some("mock-runtime-run")
        );

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let run = store.get_run(&launched.run.id).ok().flatten();
                if run
                    .as_ref()
                    .is_some_and(|item| item.status == RunStatus::Succeeded)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert_eq!(
            store
                .get_task(&task.id)?
                .ok_or_else(|| anyhow::anyhow!("task missing"))?
                .status,
            TaskStatus::Review
        );
        assert_eq!(
            store
                .get_dispatch(&launched.dispatch.id)?
                .ok_or_else(|| anyhow::anyhow!("dispatch missing"))?
                .status,
            "succeeded"
        );
        assert!(store.list_run_events(&launched.run.id, 0)?.len() >= 3);
        Ok(())
    }

    #[tokio::test]
    async fn failed_run_retry_creates_a_new_attempt_and_reuses_the_profile() -> anyhow::Result<()> {
        let directory = tempdir()?.keep();
        let store = SqliteStore::open(directory.join("ba-agent.db"))?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "retry-test".into(),
            root_path: directory.to_string_lossy().into_owned(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        ensure_builtin_agent_profiles(&store)?;
        let profile = store
            .list_agent_profiles()?
            .into_iter()
            .next()
            .map(|item| item.id);
        let task = Task {
            id: EntityId::new(),
            project_id: project.id,
            workstream_id: None,
            milestone_id: None,
            title: "Retry a failed read-only run".into(),
            description: String::new(),
            status: TaskStatus::Failed,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_task(&task)?;
        let previous = ba_core::Run {
            id: EntityId::new(),
            task_id: task.id.clone(),
            agent_profile_id: profile.clone(),
            status: RunStatus::Failed,
            adapter: "codex-app-server".into(),
            runtime_run_id: Some("failed-runtime".into()),
            runtime_thread_id: Some("failed-thread".into()),
            started_at: Some(stamp),
            finished_at: Some(stamp),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_run(&previous)?;
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 41417);
        let (events, _receiver) = broadcast::channel(16);
        let state = AppState {
            store: store.clone(),
            security: Arc::new(SessionSecurity::new(address)),
            events: events.clone(),
            worktree_root: directory.join("worktrees"),
            runtime: Arc::new(MockRuntime::new()),
            control: RuntimeCoordinator::with_event_sender(
                store.clone(),
                Arc::new(MockRuntime::new()),
                events,
            ),
            github: GitHubAdapter::default(),
        };

        let (status, Json(body)) = retry_run(State(state), Path(previous.id.to_string())).await?;

        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body["mode"], "retried");
        let attempts = store.list_runs(&task.id)?;
        assert_eq!(attempts.len(), 2);
        let retried = attempts
            .iter()
            .find(|run| run.id != previous.id)
            .ok_or_else(|| anyhow::anyhow!("retry run missing"))?;
        assert_eq!(retried.agent_profile_id, profile);
        Ok(())
    }

    #[tokio::test]
    async fn orphaned_run_resumes_from_persisted_thread_and_reaches_review() -> anyhow::Result<()> {
        let directory = tempdir()?.keep();
        let store = SqliteStore::open(directory.join("ba-agent.db"))?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "recovery-test".into(),
            root_path: directory.to_string_lossy().into_owned(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        let task = Task {
            id: EntityId::new(),
            project_id: project.id,
            workstream_id: None,
            milestone_id: None,
            title: "Resume the interrupted task".into(),
            description: String::new(),
            status: TaskStatus::Blocked,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_task(&task)?;
        let run = Run {
            id: EntityId::new(),
            task_id: task.id.clone(),
            agent_profile_id: None,
            status: RunStatus::Orphaned,
            adapter: "codex-app-server".into(),
            runtime_run_id: Some("stale-process".into()),
            runtime_thread_id: Some("persisted-thread".into()),
            started_at: Some(stamp),
            finished_at: None,
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_run(&run)?;
        let control = RuntimeCoordinator::new(store.clone(), Arc::new(MockRuntime::new()));

        let resumed = control.resume(&run.id, None).await?;
        assert_eq!(resumed.runtime_run_id.as_deref(), Some("mock-runtime-run"));
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if store
                    .get_run(&run.id)
                    .ok()
                    .flatten()
                    .is_some_and(|item| item.status == RunStatus::Succeeded)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert_eq!(
            store
                .get_task(&task.id)?
                .ok_or_else(|| anyhow::anyhow!("task missing"))?
                .status,
            TaskStatus::Review
        );
        assert!(
            store
                .list_run_events(&run.id, 0)?
                .iter()
                .any(|event| event.kind == "recovery.resumed")
        );
        Ok(())
    }
}
