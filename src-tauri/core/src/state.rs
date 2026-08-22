use crate::db::Db;
use crate::settings::Settings;
use std::sync::Mutex;

/// Shared application state: the SQLite handle and the user's settings (which
/// carry the workspace roots and the enabled scan sources).
pub struct AppState {
    pub db: Mutex<Db>,
    pub settings: Mutex<Settings>,
}
