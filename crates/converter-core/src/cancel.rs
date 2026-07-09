use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};

#[derive(Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    children: Arc<Mutex<Vec<Child>>>,
}

impl std::fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            children: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
        if let Ok(mut children) = self.children.lock() {
            for child in children.iter_mut() {
                let _ = child.kill();
            }
            children.clear();
        }
    }

    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            bail!("operation cancelled");
        }
        Ok(())
    }

    pub fn register_child(&self, child: Child) {
        if let Ok(mut children) = self.children.lock() {
            children.push(child);
        }
    }

    pub fn take_children(&self) {
        if let Ok(mut children) = self.children.lock() {
            children.clear();
        }
    }
}