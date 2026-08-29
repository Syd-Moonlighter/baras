//! Tauri commands module
//!
//! All Tauri-invokable commands are centralized here for easy discovery.
//!
//! # Command Categories
//!
//! - `overlay` - Overlay show/hide, move mode, status, refresh
//! - `service` - Log files, tailing, config, session info, profiles
//! - `timers` - Encounter timer CRUD for the timer editor UI (LEGACY)
//! - `encounters` - Unified encounter item CRUD (NEW - replaces timers)
//! - `effects` - Effect definition CRUD for the effect editor UI
//! - `parsely` - Parsely.io log upload
//! - `url` - URL opening with portal support for Linux

mod effects;
mod encounters;
mod overlay;
mod parsely;
mod query;
mod service;
mod stream_overlay;
mod url;

// Re-export all commands for the invoke_handler
pub use effects::*;
pub use encounters::*;
pub use overlay::*;
pub use parsely::*;
pub use query::*;
pub use service::*;
pub use stream_overlay::*;
pub use url::*;
