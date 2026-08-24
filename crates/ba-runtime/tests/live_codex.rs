use std::{collections::BTreeMap, time::Duration};

use ba_runtime::{CodexAdapter, RunRequest, RuntimeAdapter, RuntimeEventKind, SandboxMode};

#[tokio::test]
#[ignore = "requires a logged-in Codex CLI and performs one live model turn"]
async fn logged_in_codex_completes_an_app_server_turn() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let adapter = CodexAdapter::default();
    let mut events = adapter.subscribe();
    let handle = adapter
        .launch(RunRequest {
            prompt: "Respond with exactly BA_AGENT_RUNTIME_OK. Do not use tools.".into(),
            cwd: directory.path().canonicalize()?,
            sandbox: SandboxMode::ReadOnly,
            environment: BTreeMap::from([("BA_AGENT_MANAGED_RUN".into(), "1".into())]),
        })
        .await?;

    let terminal = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let event = events.recv().await?;
            if event.run_id == handle.run_id
                && event.kind == RuntimeEventKind::Lifecycle
                && matches!(event.name.as_str(), "succeeded" | "failed" | "cancelled")
            {
                return Ok::<_, tokio::sync::broadcast::error::RecvError>(event);
            }
        }
    })
    .await;
    let terminal = match terminal {
        Ok(event) => event?,
        Err(error) => {
            let _ignored = adapter.cancel(&handle.run_id).await;
            return Err(error.into());
        }
    };
    assert_eq!(terminal.name, "succeeded", "{terminal:?}");
    Ok(())
}
