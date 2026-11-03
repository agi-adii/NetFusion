use crate::db::Database;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
pub struct AppLogger {
    db: Arc<Database>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl AppLogger {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn attach_app_handle(&self, handle: AppHandle) {
        if let Ok(mut guard) = self.app_handle.lock() {
            *guard = Some(handle);
        }
    }

    pub fn log(&self, level: &str, category: &str, message: impl Into<String>) {
        let msg = message.into();
        let entry = self.db.insert_log(level, category, &msg);
        if let Ok(guard) = self.app_handle.lock() {
            if let Some(ref app) = *guard {
                let _ = app.emit("log-added", &entry);
            }
        }
    }

    pub fn info(&self, category: &str, message: impl Into<String>) {
        self.log("INFO", category, message);
    }

    pub fn success(&self, category: &str, message: impl Into<String>) {
        self.log("SUCCESS", category, message);
    }

    pub fn warn(&self, category: &str, message: impl Into<String>) {
        self.log("WARN", category, message);
    }

    pub fn error(&self, category: &str, message: impl Into<String>) {
        self.log("ERROR", category, message);
    }
}

