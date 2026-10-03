//! Small, allocation-bounded admission primitives for capture workers.
//!
//! Capture polling is intentionally best-effort: a new deep read is dropped
//! when another read (including its database write) is still in flight.  This
//! keeps a burst of window changes from becoming an unbounded Tokio queue.

use std::{collections::HashMap, hash::Hash, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, time::{Duration, Instant}};

/// Process-wide admission gate for deep captures.
#[derive(Clone)]
pub(crate) struct DeepCaptureAdmission {
    in_flight: Arc<AtomicBool>,
    last_started: Arc<Mutex<Option<Instant>>>,
    cooldown: Duration,
}

impl Default for DeepCaptureAdmission {
    fn default() -> Self {
        Self::with_cooldown(Duration::from_secs(2))
    }
}

impl DeepCaptureAdmission {
    pub(crate) fn try_acquire(&self) -> Option<DeepCapturePermit> {
        self.try_acquire_at(Instant::now())
    }

    fn with_cooldown(cooldown: Duration) -> Self {
        Self { in_flight: Arc::new(AtomicBool::new(false)), last_started: Arc::new(Mutex::new(None)), cooldown }
    }

    fn try_acquire_at(&self, now: Instant) -> Option<DeepCapturePermit> {
        let mut last_started = self.last_started.lock().ok()?;
        if last_started.is_some_and(|last| now.saturating_duration_since(last) < self.cooldown) {
            return None;
        }
        self.in_flight
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| {
                *last_started = Some(now);
                DeepCapturePermit { in_flight: Arc::clone(&self.in_flight) }
            })
    }

    #[cfg(test)]
    pub(crate) fn is_in_flight(&self) -> bool {
        self.in_flight.load(Ordering::Acquire)
    }
}

/// Permit held from the reader through the final persistence future.
pub(crate) struct DeepCapturePermit {
    in_flight: Arc<AtomicBool>,
}

impl Drop for DeepCapturePermit {
    fn drop(&mut self) {
        self.in_flight.store(false, Ordering::Release);
    }
}

/// A small TTL cache.  Entries are evicted by oldest insertion/update time;
/// all operations prune expired values, so its memory use is bounded even when
/// callers continuously encounter new window titles.
pub(crate) struct TtlCache<K, V> {
    entries: HashMap<K, (V, Instant)>,
    max_entries: usize,
    ttl: Duration,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash + Clone,
{
    pub(crate) fn new(max_entries: usize, ttl: Duration) -> Self {
        Self { entries: HashMap::new(), max_entries: max_entries.max(1), ttl }
    }

    pub(crate) fn get_cloned(&mut self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        self.prune_expired();
        self.entries.get(key).map(|(value, _)| value.clone())
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        self.prune_expired();
        if !self.entries.contains_key(&key) && self.entries.len() >= self.max_entries {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, inserted_at))| *inserted_at)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, (value, Instant::now()));
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    fn prune_expired(&mut self) {
        let ttl = self.ttl;
        self.entries.retain(|_, (_, inserted_at)| inserted_at.elapsed() < ttl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread};

    #[test]
    fn burst_only_admits_one_capture() {
        let admission = DeepCaptureAdmission::with_cooldown(Duration::ZERO);
        let first = admission.try_acquire().expect("first capture admitted");
        let attempts = (0..128).filter(|_| admission.try_acquire().is_some()).count();
        assert_eq!(attempts, 0);
        drop(first);
        assert!(admission.try_acquire().is_some());
    }

    #[test]
    fn permit_lifecycle_releases_after_drop() {
        let admission = DeepCaptureAdmission::with_cooldown(Duration::ZERO);
        {
            let _permit = admission.try_acquire().expect("capture admitted");
            assert!(admission.is_in_flight());
        }
        assert!(!admission.is_in_flight());
        let gate = Arc::new(admission);
        let worker = Arc::clone(&gate);
        let joined = thread::spawn(move || worker.try_acquire().is_some()).join().unwrap();
        assert!(joined);
    }

    #[test]
    fn cache_is_bounded() {
        let mut cache = TtlCache::new(4, Duration::from_secs(60));
        for key in 0..64 {
            cache.insert(key, key);
        }
        assert_eq!(cache.len(), 4);
    }

    #[test]
    fn sustained_changes_do_not_reset_cooldown() {
        let admission = DeepCaptureAdmission::default();
        let start = Instant::now();
        drop(admission.try_acquire_at(start).unwrap());
        for millis in [100, 500, 1000, 1999] {
            assert!(admission.try_acquire_at(start + Duration::from_millis(millis)).is_none());
        }
        assert!(admission.try_acquire_at(start + Duration::from_secs(2)).is_some());
    }

    #[test]
    fn cache_expiry_and_refresh_are_bounded() {
        let mut cache = TtlCache::new(2, Duration::ZERO);
        cache.insert("old", 1);
        assert_eq!(cache.get_cloned(&"old"), None);
        cache.insert("new", 2);
        assert_eq!(cache.len(), 1);
    }
}
