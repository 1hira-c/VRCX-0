#[path = "../../../src-tauri/src/error.rs"]
mod error;
pub mod state;
#[allow(non_snake_case, unused_imports)]
pub(crate) mod commands {
    include!(concat!(env!("OUT_DIR"), "/commands.rs"));
}
pub mod executor;
pub mod server;
