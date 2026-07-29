use std::sync::Arc;

use tokio::sync::{watch, Mutex};

/// In-process tracker for the single background chat-sync loop.
///
/// The Kindroid chat-history endpoint is capped at 600 requests per 24 h
/// per token, so multiple parallel syncs would multiply the budget
/// pressure. We only allow one sync at a time across all targets.
pub struct SyncRegistry {
    inner: Mutex<Option<SyncEntry>>,
}

struct SyncEntry {
    ai_id: String,
    cancel_tx: Arc<watch::Sender<bool>>,
}

impl Default for SyncRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Try to start a sync for `ai_id`. Returns `Ok(handle)` if no sync
    /// is currently running. Returns `Err(current_ai_id)` if a sync is
    /// already running for any ai_id; the caller must cancel the existing
    /// one first.
    pub async fn start(&self, ai_id: &str) -> Result<SyncHandle, String> {
        let mut guard = self.inner.lock().await;
        if let Some(existing) = guard.as_ref() {
            return Err(existing.ai_id.clone());
        }
        let (tx, rx) = watch::channel(false);
        *guard = Some(SyncEntry {
            ai_id: ai_id.to_string(),
            cancel_tx: Arc::new(tx),
        });
        Ok(SyncHandle { cancel_rx: rx })
    }

    /// Signal the current sync to stop, but keep the slot held. The
    /// sync loop calls `release()` on exit. Returns the ai_id that was
    /// being synced, if any.
    pub async fn cancel(&self) -> Option<String> {
        let guard = self.inner.lock().await;
        if let Some(entry) = guard.as_ref() {
            let _ = entry.cancel_tx.send(true);
            Some(entry.ai_id.clone())
        } else {
            None
        }
    }

    /// Clear the current sync slot without signalling cancellation. Used
    /// by the sync loop when it exits so a future sync can take the slot.
    pub async fn release(&self) -> Option<String> {
        let mut guard = self.inner.lock().await;
        guard.take().map(|e| e.ai_id)
    }

    /// The ai_id currently syncing, if any.
    pub async fn current(&self) -> Option<String> {
        self.inner.lock().await.as_ref().map(|e| e.ai_id.clone())
    }
}

/// Handle returned from `start`. Dropping it does *not* cancel — the
/// loop calls `release()` on the registry explicitly when it exits.
#[derive(Debug)]
pub struct SyncHandle {
    pub cancel_rx: watch::Receiver<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn single_slot_semantics() {
        let r = SyncRegistry::new();
        assert!(r.current().await.is_none());

        let h1 = r.start("ai_a").await.unwrap();
        assert_eq!(r.current().await.as_deref(), Some("ai_a"));

        // Second start with a different ai_id must conflict — the slot is
        // still held by ai_a (dropping the handle does NOT release).
        let conflict = r.start("ai_b").await.unwrap_err();
        assert_eq!(conflict, "ai_a");

        // Release the slot via the registry (this is what the sync loop does
        // on exit) and try again.
        let released = r.release().await;
        assert_eq!(released.as_deref(), Some("ai_a"));
        assert!(r.current().await.is_none());

        let h2 = r.start("ai_b").await.unwrap();
        assert_eq!(r.current().await.as_deref(), Some("ai_b"));

        let cancelled = r.cancel().await;
        assert_eq!(cancelled.as_deref(), Some("ai_b"));
        // cancel signals but does not release the slot; sync loop must release.
        assert_eq!(r.current().await.as_deref(), Some("ai_b"));
        let released = r.release().await;
        assert_eq!(released.as_deref(), Some("ai_b"));
        assert!(r.current().await.is_none());
        drop(h1);
        drop(h2);
    }

    #[tokio::test]
    async fn cancel_signals_watch() {
        let r = SyncRegistry::new();
        let h = r.start("ai_x").await.unwrap();
        let mut rx = h.cancel_rx;
        // Initial value is false.
        assert!(!*rx.borrow());
        r.cancel().await;
        rx.changed().await.unwrap();
        assert!(*rx.borrow());
    }
}
