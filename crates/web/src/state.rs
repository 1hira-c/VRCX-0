use std::sync::Arc;
use vrcx_0_runtime_host_desktop::DesktopRuntimeHostState;

pub struct AppState(pub DesktopRuntimeHostState);
pub type State<'a, T> = &'a T;
pub type AppHandle = Arc<AppState>;
impl AppState {
    pub fn runtime_host(&self) -> &DesktopRuntimeHostState {
        &self.0
    }
}
