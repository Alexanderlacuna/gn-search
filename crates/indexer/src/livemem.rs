//! Live-document accounting for the streaming write path.
//!
//! The memory objective of the streaming refactor is that the number
//! of `Document` objects simultaneously alive in the run orchestrator
//! is O(1), not O(crawl size). [`CountedDocument`] wraps one built
//! document; constructing it increments a process-global live counter
//! and records the high-water mark, dropping it decrements. The peak
//! is reported on stdout as `max_live_documents` when
//! `GN4_INDEXER_LIVE_COUNTER=1`.
//!
//! The RED control (`GN4_INDEXER_LIVE_RED=retain`) makes the caller
//! retain every wrapped document instead of releasing it after the
//! write, so the counter must rise to O(docs) and the flatness
//! assertion must fail.

use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// A built document whose lifetime is tracked by the counter.
pub struct CountedDocument<T> {
    inner: T,
}

impl<T> CountedDocument<T> {
    pub fn new(inner: T) -> Self {
        let live = LIVE.fetch_add(1, Ordering::SeqCst) + 1;
        PEAK.fetch_max(live, Ordering::SeqCst);
        Self { inner }
    }

    pub fn get(&self) -> &T {
        &self.inner
    }
}

impl<T> Drop for CountedDocument<T> {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Reset the live and peak counters before a run.
pub fn reset() {
    LIVE.store(0, Ordering::SeqCst);
    PEAK.store(0, Ordering::SeqCst);
}

/// The high-water mark of simultaneously live documents this run.
pub fn max_live() -> usize {
    PEAK.load(Ordering::SeqCst)
}

/// Whether the run should retain every document (the RED control).
pub fn red_retain() -> bool {
    std::env::var("GN4_INDEXER_LIVE_RED").as_deref() == Ok("retain")
}
