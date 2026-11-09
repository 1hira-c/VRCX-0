use crate::{commands, error::AppError, state::AppState};
use axum::{
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Response, Sse,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    convert::Infallible,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, Semaphore};
use tower_http::services::ServeDir;
use vrcx_0_application_core::RuntimeEventSink;
use vrcx_0_local_server::{generate_token, tokens_match};

const HOST_COMMANDS: &[&str] = &[
    "app__take_data_dir_migration_result",
    "app__take_pending_desktop_notification_activation",
    "app__append_error_log",
    "app__get_app_data_dir_state",
    "app__profile_restore_take_last_result",
    "app__shared_collection_import_status",
    "app__privacy_lock_setup_request_take",
    "app__drain_pending_deep_links",
    "app__runtime_background_job_record",
    "app__integration_api_status",
    "app__feed_logger_status",
    "app__get_host_capabilities",
    "app__startup_bootstrap_snapshot_get",
    "app__system_language",
    "app__system_culture",
    "app__backend_runtime_combined_snapshot_get",
    "app__ancillary_runtime_snapshot_get",
    "app__database_upgrade_preflight",
    "app__database_upgrade_run",
    "app__database_upgrade_progress",
    "app__database_upgrade_retry",
    "app__database_upgrade_failure_log_path",
    "app__get_legacy_vrcx_migration_status",
    "app__runtime_group_instances_refresh",
];

const SESSION_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
#[derive(Clone)]
pub struct WebState {
    pub runtime: Arc<AppState>,
    pub events: broadcast::Sender<Value>,
    pub shutdown: tokio_util::sync::CancellationToken,
    auth: Arc<WebAuth>,
    requests: Arc<Semaphore>,
}
struct WebSession {
    created: Instant,
    cookie_path: String,
}
struct WebAuth {
    token: String,
    sessions: Mutex<HashMap<String, WebSession>>,
    secure_cookie: bool,
}
impl WebState {
    pub fn new(runtime: Arc<AppState>, token: String, secure_cookie: bool) -> Self {
        let (events, _) = broadcast::channel(1024);
        runtime.0.set_runtime_event_sink(WebEvents(events.clone()));
        Self {
            runtime,
            events,
            shutdown: tokio_util::sync::CancellationToken::new(),
            auth: Arc::new(WebAuth {
                token,
                sessions: Mutex::new(HashMap::new()),
                secure_cookie,
            }),
            requests: Arc::new(Semaphore::new(16)),
        }
    }
}
struct WebEvents(broadcast::Sender<Value>);
impl RuntimeEventSink for WebEvents {
    fn emit(&self, name: &str, payload: Value) {
        let _ = self.0.send(json!({"event": name, "payload": payload}));
    }
}
impl WebAuth {
    fn session(&self, headers: &HeaderMap) -> Option<String> {
        let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
        let value = cookie
            .split(';')
            .find_map(|entry| entry.trim().strip_prefix("vrcx0_session="))?;
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, created| created.created.elapsed() < SESSION_AGE);
        sessions.contains_key(value).then(|| value.to_string())
    }
    fn cookie(&self, value: &str, age: u64, path: &str) -> String {
        format!(
            "vrcx0_session={value}; Path={path}; HttpOnly; SameSite=Strict; Max-Age={age}{}",
            if self.secure_cookie { "; Secure" } else { "" }
        )
    }
}

pub fn router(state: WebState, static_dir: PathBuf) -> Router {
    let protected = Router::new()
        .route(
            "/api/session",
            get(|| async { Json(json!({"authenticated":true})) }).delete(logout),
        )
        .route("/api/capabilities", get(capabilities))
        .route("/api/invoke/{command}", post(invoke))
        .route("/api/events", get(events))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));
    Router::new()
        .merge(protected)
        .route("/api/login", post(login))
        .route("/healthz", get(|| async { Json(json!({"ok":true})) }))
        .fallback_service(ServeDir::new(static_dir).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}
async fn security_headers(request: Request, next: Next) -> Response {
    // A non-simple header prevents cross-origin form/fetch CSRF. No CORS is enabled.
    if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) && request
        .headers()
        .get("x-vrcx-web")
        .and_then(|h| h.to_str().ok())
        != Some("1")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    // Home Assistant embeds the UI in a same-origin iframe.
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        "frame-ancestors 'self'".parse().unwrap(),
    );
    response
}
async fn authenticate(State(state): State<WebState>, request: Request, next: Next) -> Response {
    if state.auth.session(request.headers()).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Login {
    token: String,
    #[serde(default = "root_cookie_path")]
    cookie_path: String,
}
fn root_cookie_path() -> String {
    "/".into()
}
fn valid_cookie_path(path: &str) -> bool {
    path.starts_with('/')
        && path.ends_with('/')
        && path.len() <= 512
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-._~%".contains(&b))
}
async fn login(State(state): State<WebState>, Json(input): Json<Login>) -> Response {
    if !tokens_match(input.token.trim(), &state.auth.token) {
        tokio::time::sleep(Duration::from_millis(300)).await;
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !valid_cookie_path(&input.cookie_path) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(token) = generate_token() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut sessions = state.auth.sessions.lock().unwrap();
    sessions.retain(|_, created| created.created.elapsed() < SESSION_AGE);
    if sessions.len() >= 64 {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    sessions.insert(
        token.clone(),
        WebSession {
            created: Instant::now(),
            cookie_path: input.cookie_path.clone(),
        },
    );
    (
        [(
            header::SET_COOKIE,
            state
                .auth
                .cookie(&token, SESSION_AGE.as_secs(), &input.cookie_path),
        )],
        Json(json!({"authenticated": true})),
    )
        .into_response()
}
async fn logout(State(state): State<WebState>, headers: HeaderMap) -> Response {
    let path = state
        .auth
        .session(&headers)
        .and_then(|session| state.auth.sessions.lock().unwrap().remove(&session))
        .map(|session| session.cookie_path)
        .unwrap_or_else(root_cookie_path);
    (
        [(header::SET_COOKIE, state.auth.cookie("", 0, &path))],
        StatusCode::NO_CONTENT,
    )
        .into_response()
}
async fn capabilities() -> Json<Value> {
    Json(
        json!({"protocolVersion":1, "host":"server", "commands":commands::COMMANDS.iter().chain(HOST_COMMANDS).collect::<Vec<_>>(),
        "desktopAgents": {"supported":false,"devices":[]}, "capabilities": host_capabilities()}),
    )
}
fn host_capabilities() -> Value {
    let mut result =
        json!({"platform":"unknown","arch":std::env::consts::ARCH,"linuxPackageKind":"unknown"});
    for name in [
        "localDatabase",
        "websocketRuntime",
        "runtimeRealtimeTransport",
        "gameLogWatcher",
        "runtimeGameLogIngest",
        "runtimeGameLogSideEffects",
        "runtimeGameClientLifecycle",
        "gameProcessMonitor",
        "vrchatPathDiscovery",
        "steamLibraryDiscovery",
        "steamRuntimeIntegration",
        "registryPrefs",
        "gameLaunch",
        "vrchatLaunchPipe",
        "screenshotCache",
    ] {
        let enabled = matches!(
            name,
            "localDatabase" | "websocketRuntime" | "runtimeRealtimeTransport"
        );
        result[name] = json!({"supported":enabled,"enabled":enabled,"available":enabled, "reason":if enabled {None} else {Some("Requires a connected desktop agent (not implemented yet).")}});
    }
    result
}
async fn invoke(
    State(state): State<WebState>,
    Path(command): Path<String>,
    Json(args): Json<Value>,
) -> Response {
    let Ok(permit) = state.requests.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let runtime = state.runtime;
    let handle = tokio::runtime::Handle::current();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        handle.block_on(dispatch(runtime, &command, args))
    })
    .await;
    match result {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => (StatusCode::BAD_REQUEST, Json(error)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
async fn dispatch(state: Arc<AppState>, command: &str, args: Value) -> Result<Value, AppError> {
    let runtime = &state.0;
    Ok(match command {
        "app__append_error_log" => {
            let entry: String =
                serde_json::from_value(args.get("entry").cloned().unwrap_or(Value::Null))?;
            runtime.append_error_log(&entry.chars().take(16384).collect::<String>());
            Value::Null
        }
        "app__get_app_data_dir_state" => serde_json::to_value(runtime.data_dir().state()?)?,
        "app__profile_restore_take_last_result" => {
            serde_json::to_value(runtime.profile_backup().take_last_restore_result()?)?
        }
        "app__shared_collection_import_status" => {
            serde_json::to_value(runtime.shared_collection_import_status())?
        }
        "app__take_data_dir_migration_result" => {
            serde_json::to_value(runtime.data_dir().take_last_result()?)?
        }
        "app__take_pending_desktop_notification_activation" => Value::Null,
        "app__privacy_lock_setup_request_take" => json!(false),
        "app__drain_pending_deep_links" => json!([]),
        "app__runtime_background_job_record" => {
            runtime.record_runtime_job(serde_json::from_value(
                args.get("input").cloned().unwrap_or(Value::Null),
            )?);
            Value::Null
        }
        "app__integration_api_status" => {
            serde_json::to_value(runtime.integration_api().status().await?)?
        }
        "app__feed_logger_status" => serde_json::to_value(runtime.feed_logger().probe().await?)?,
        "app__get_host_capabilities" => host_capabilities(),
        "app__startup_bootstrap_snapshot_get" => {
            let mut snapshot = serde_json::to_value(runtime.startup_bootstrap_snapshot()?)?;
            snapshot["hostCapabilities"] = host_capabilities();
            snapshot
        }
        "app__system_language" => json!(vrcx_0_runtime_host_desktop::system_language()),
        "app__system_culture" => json!(vrcx_0_runtime_host_desktop::system_culture()),
        "app__backend_runtime_combined_snapshot_get" => {
            serde_json::to_value(runtime.backend_runtime_combined_snapshot())?
        }
        "app__ancillary_runtime_snapshot_get" => {
            serde_json::to_value(runtime.ancillary_runtime_snapshot().await)?
        }
        "app__database_upgrade_preflight" => {
            serde_json::to_value(runtime.database_upgrade().preflight().await?)?
        }
        "app__database_upgrade_run" => {
            serde_json::to_value(runtime.database_upgrade().run().await?)?
        }
        "app__database_upgrade_progress" => {
            serde_json::to_value(runtime.database_upgrade().progress())?
        }
        "app__database_upgrade_retry" => {
            serde_json::to_value(runtime.database_upgrade().retry().await?)?
        }
        "app__database_upgrade_failure_log_path" => {
            json!(runtime.database_upgrade().failure_log_path())
        }
        "app__get_legacy_vrcx_migration_status" => json!({"available":false,"detected":false}),
        "app__runtime_group_instances_refresh" => {
            runtime.refresh_runtime_group_instances().await;
            Value::Null
        }
        _ => return commands::dispatch(state, command, args).await,
    })
}
async fn events(State(state): State<WebState>, headers: HeaderMap) -> Response {
    let Some(session) = state.auth.session(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let receiver = state.events.subscribe();
    let stream = stream::unfold(
        (receiver, state.auth, session, state.shutdown),
        |(mut rx, auth, session, shutdown)| async move {
            loop {
                if !auth
                    .sessions
                    .lock()
                    .unwrap()
                    .get(&session)
                    .is_some_and(|created| created.created.elapsed() < SESSION_AGE)
                {
                    return None;
                }
                let received = tokio::select! {
                    _ = shutdown.cancelled() => return None,
                    received = tokio::time::timeout(Duration::from_secs(15), rx.recv()) => received,
                };
                let value = match received {
                    Ok(Ok(value)) => value,
                    Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                        json!({"event":"web:resync","payload":null})
                    }
                    Ok(Err(broadcast::error::RecvError::Closed)) => return None,
                    Err(_) => continue,
                };
                if !auth
                    .sessions
                    .lock()
                    .unwrap()
                    .get(&session)
                    .is_some_and(|created| created.created.elapsed() < SESSION_AGE)
                {
                    return None;
                }
                return Some((
                    Ok::<_, Infallible>(Event::default().json_data(value).unwrap()),
                    (rx, auth, session, shutdown),
                ));
            }
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_cookie_expires_and_is_not_the_access_token() {
        let auth = WebAuth {
            token: "access-token".into(),
            sessions: Mutex::new(HashMap::new()),
            secure_cookie: true,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "vrcx0_session=access-token".parse().unwrap(),
        );
        assert!(auth.session(&headers).is_none());
        auth.sessions.lock().unwrap().insert(
            "device-session".into(),
            WebSession {
                created: Instant::now(),
                cookie_path: "/".into(),
            },
        );
        headers.insert(
            header::COOKIE,
            "other=x; vrcx0_session=device-session".parse().unwrap(),
        );
        assert!(auth.session(&headers).is_some());
        auth.sessions.lock().unwrap().insert(
            "device-session".into(),
            WebSession {
                created: Instant::now() - SESSION_AGE,
                cookie_path: "/".into(),
            },
        );
        assert!(auth.session(&headers).is_none());
        assert!(auth
            .cookie("x", 10, "/api/hassio_ingress/test/")
            .contains("HttpOnly; SameSite=Strict; Max-Age=10; Secure"));
    }
    #[test]
    fn ingress_cookie_paths_cannot_inject_attributes() {
        assert!(valid_cookie_path("/api/hassio_ingress/test/"));
        assert!(!valid_cookie_path("/; Domain=example.com;/"));
        assert!(!valid_cookie_path("relative/"));
    }
    #[test]
    fn desktop_operations_are_not_exposed_by_the_web_bridge() {
        for forbidden in [
            "app__open_link",
            "app__get_file_base64",
            "app__write_config_file",
            "app__start_game_from_path",
            "app__open_file_selector_dialog",
        ] {
            assert!(!commands::COMMANDS.contains(&forbidden));
        }
        assert!(commands::COMMANDS.contains(&"app__vrchat_auth_session_start"));
        assert!(!host_capabilities()["gameLaunch"]["available"]
            .as_bool()
            .unwrap());
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use vrcx_0_platform::app_paths::{AppDataDirResolution, AppDataDirSource};
    use vrcx_0_runtime_host_desktop::{DesktopRuntimeHostOptions, DesktopRuntimeHostState};

    #[tokio::test(flavor = "multi_thread")]
    async fn authentication_csrf_rpc_events_and_logout() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let dir = std::env::temp_dir().join(format!("vrcx-web-test-{}", generate_token().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let runtime = Arc::new(AppState(
            DesktopRuntimeHostState::new_server(DesktopRuntimeHostOptions {
                realtime_origin: "http://localhost:9000".into(),
                launched_from_autostart: false,
                app_data_dir: AppDataDirResolution {
                    current_dir: dir.clone(),
                    default_dir: dir.clone(),
                    persisted_dir: None,
                    cli_dir: Some(dir.clone()),
                    source: AppDataDirSource::Cli,
                },
                app_version: "test".into(),
                app_update_build_label: String::new(),
                app_update_build_badge: String::new(),
                app_update_check_disabled: true,
                updater_port: Arc::new(vrcx_0_application_core::NoopUpdaterPort),
                database_maintenance_cache_dir: None,
                task_executor: Arc::new(crate::executor::TokioRuntimeTaskExecutor),
            })
            .unwrap(),
        ));
        let state = WebState::new(
            runtime.clone(),
            "a-test-access-token-at-least-32-characters".into(),
            true,
        );
        let app = router(state.clone(), dir.clone());
        let request = |method: &str, path: &str, cookie: Option<&str>, csrf: bool, body: Value| {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            if let Some(cookie) = cookie {
                builder = builder.header("cookie", cookie);
            }
            if csrf {
                builder = builder.header("x-vrcx-web", "1");
            }
            builder.body(Body::from(body.to_string())).unwrap()
        };
        let response = app
            .clone()
            .oneshot(request("GET", "/api/session", None, false, Value::Null))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let login_body = json!({"token":"a-test-access-token-at-least-32-characters"});
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/login",
                None,
                false,
                login_body.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/login",
                None,
                true,
                json!({"token":"wrong"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(request("POST", "/api/login", None, true, login_body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        assert!(!cookie.contains("a-test-access"));
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/invoke/storage__set",
                Some(&cookie),
                true,
                json!({"key":"test", "value":"persisted"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/invoke/storage__get_all",
                Some(&cookie),
                true,
                json!({}),
            ))
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["test"],
            "persisted"
        );
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/invoke/app__open_link",
                Some(&cookie),
                true,
                json!({"url":"file:///etc/passwd"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                "/api/events",
                Some(&cookie),
                false,
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/event-stream"
        );
        state
            .events
            .send(json!({"event":"test", "payload":{"revision":1}}))
            .unwrap();
        let mut body = response.into_body();
        let frame = tokio::time::timeout(Duration::from_secs(2), body.frame())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(std::str::from_utf8(frame.data_ref().unwrap())
            .unwrap()
            .contains("revision"));
        drop(body);
        let response = app
            .clone()
            .oneshot(request(
                "DELETE",
                "/api/session",
                Some(&cookie),
                true,
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = app
            .oneshot(request(
                "GET",
                "/api/capabilities",
                Some(&cookie),
                false,
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        runtime.0.stop_for_application_exit("test", || {});
        drop(state);
        drop(runtime);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
