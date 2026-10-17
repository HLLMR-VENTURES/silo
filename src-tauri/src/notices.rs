//! One-time notices from startup work (a database restored, an archive found again). The
//! frontend reads them once after it loads and shows them as an info banner.

use std::sync::Mutex;

static NOTICES: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn push(notice: impl Into<String>) {
    NOTICES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(notice.into());
}

pub fn take() -> Vec<String> {
    std::mem::take(&mut *NOTICES.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Take only the notices that mention `needle`, leaving the rest queued. Tests run in parallel
/// and share the queue, so each takes just its own (by its temp folder's path).
#[cfg(test)]
pub fn take_mentioning(needle: &str) -> Vec<String> {
    let mut all = NOTICES.lock().unwrap_or_else(|e| e.into_inner());
    let (mine, rest) = std::mem::take(&mut *all)
        .into_iter()
        .partition(|n| n.contains(needle));
    *all = rest;
    mine
}
