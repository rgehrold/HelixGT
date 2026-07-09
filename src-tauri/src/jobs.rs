use std::collections::HashMap;
use std::sync::Mutex;

use converter_core::CancelToken;

pub struct JobManager {
    jobs: Mutex<HashMap<String, CancelToken>>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, id: impl Into<String>) -> CancelToken {
        let token = CancelToken::new();
        let id = id.into();
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(id, token.clone());
        }
        token
    }

    pub fn cancel(&self, id: &str) -> bool {
        let token = if let Ok(jobs) = self.jobs.lock() {
            jobs.get(id).cloned()
        } else {
            None
        };
        if let Some(token) = token {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn finish(&self, id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(id);
        }
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.jobs
            .lock()
            .map(|jobs| jobs.contains_key(id))
            .unwrap_or(false)
    }
}