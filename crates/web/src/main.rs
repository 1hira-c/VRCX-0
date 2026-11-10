use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::Arc,
};
use vrcx_0_application_core::NoopUpdaterPort;
use vrcx_0_platform::app_paths::resolve_app_data_dir;
use vrcx_0_runtime_host_desktop::{DesktopRuntimeHostOptions, DesktopRuntimeHostState};
use vrcx_0_vrchat_client::web_client::{set_user_agent_identity, UserAgentIdentity};
use vrcx_0_web::executor;
use vrcx_0_web::{
    server::{router, WebState},
    state::AppState,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vrcx_0=info".into()),
        )
        .init();
    // This host is an unofficial fork; identify it separately from upstream VRCX-0.
    set_user_agent_identity(UserAgentIdentity {
        product: "VRCX-0-Web",
        comment: Some("+https://github.com/1hira-c/VRCX-0"),
    });
    while serve().await? {}
    Ok(())
}

async fn serve() -> Result<bool, Box<dyn std::error::Error>> {
    let data = resolve_app_data_dir()?;
    let key_path = data.current_dir.join("web-secret-key");
    // Stable across container replacements; never derive encryption from a container ID.
    persistent_secret(&key_path)?;
    std::env::set_var("VRCX_0_SECRETS_KEY_FILE", &key_path);
    let token_path = std::env::var_os("VRCX_WEB_TOKEN_FILE")
        .map(Into::into)
        .unwrap_or_else(|| data.current_dir.join("web-access-token"));
    let token = access_token(&token_path)?;
    let runtime = Arc::new(AppState(DesktopRuntimeHostState::new_server(
        DesktopRuntimeHostOptions {
            realtime_origin: "http://localhost:9000".into(),
            launched_from_autostart: false,
            app_data_dir: data,
            app_version: serde_json::from_str::<serde_json::Value>(include_str!(
                "../../../src-tauri/tauri.conf.json"
            ))?["version"]
                .as_str()
                .unwrap()
                .into(),
            app_update_build_label: "web".into(),
            app_update_build_badge: "Web".into(),
            app_update_check_disabled: true,
            updater_port: Arc::new(NoopUpdaterPort),
            database_maintenance_cache_dir: None,
            task_executor: Arc::new(executor::TokioRuntimeTaskExecutor),
        },
    )?));
    let state = WebState::new(
        runtime.clone(),
        token,
        std::env::var("VRCX_WEB_SECURE_COOKIE").as_deref() == Ok("1"),
    );
    let static_dir = std::env::var_os("VRCX_WEB_STATIC_DIR")
        .unwrap_or_else(|| "dist".into())
        .into();
    let address = std::env::var("VRCX_WEB_LISTEN").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!(%address, "VRCX-0 Web is listening");
    let startup = state.clone();
    let restore = tokio::spawn(async move {
        startup.start_backend().await;
    });
    let cancellation = state.shutdown.clone();
    axum::serve(listener, router(state.clone(), static_dir))
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown() => cancellation.cancel(),
                _ = cancellation.cancelled() => {},
            }
        })
        .await?;
    // Startup cancels readiness/auth waits but joins an active blocking migration.
    state.stop_backend(restore).await?;
    Ok(state.restart_requested())
}
fn persistent_secret(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        writeln!(file, "{}", vrcx_0_local_server::generate_token()?)?;
        file.sync_all()?;
    }
    if fs::read_to_string(path)?.trim().len() < 32 {
        return Err("Invalid persistent encryption key".into());
    }
    Ok(())
}
fn access_token(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if path.exists() {
        let token = fs::read_to_string(path)?.trim().to_string();
        if token.len() < 32 {
            return Err("Web access token must contain at least 32 characters".into());
        }
        tracing::info!(path=%path.display(), "Web access token file (enter its contents in the browser)");
        return Ok(token);
    }
    let token = vrcx_0_local_server::generate_token()?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    writeln!(file, "{token}")?;
    file.sync_all()?;
    // Printed once for first-run HAOS installs; never emit VRChat credentials.
    eprintln!("VRCX-0 Web access token: {token}");
    Ok(token)
}
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
