use crate::shared::error::AppError;
use linked_hash_map::LinkedHashMap;
use parking_lot::RwLock;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Entry {
    data: Vec<u8>,
    expires_at: Instant,
    disk_path: Option<PathBuf>,
    size: usize,
    created_at: Instant,
}

/// Two-level LRU+TTL cache with memory (fast) and disk (persistent) layers.
/// Uses LinkedHashMap for O(1) touch/evict operations.
pub struct Cache {
    memory: RwLock<LinkedHashMap<String, Entry>>,
    disk_root: PathBuf,
    default_ttl: Duration,
    max_memory_entries: usize,
}

impl Cache {
    pub fn new(disk_root: PathBuf, default_ttl: Duration, max_memory_entries: usize) -> Self {
        let _ = fs::create_dir_all(&disk_root);
        Self {
            memory: RwLock::new(LinkedHashMap::new()),
            disk_root,
            default_ttl,
            max_memory_entries,
        }
    }

    /// Retrieve a value by key. Checks memory first, then disk.
    /// Touches LRU order on access (O(1) via LinkedHashMap).
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        // Check memory first + touch LRU (O(1))
        {
            let mut mem = self.memory.write();
            if let Some(entry) = mem.get_refresh(key) {
                if Instant::now() < entry.expires_at {
                    return Some(entry.data.clone());
                }
            }
        }

        // Check disk
        let disk_path = self.disk_root.join(sanitize_key(key));
        if disk_path.exists() {
            match fs::read(&disk_path) {
                Ok(data) => {
                    let expires_at = Instant::now() + self.default_ttl;
                    if Instant::now() >= expires_at {
                        let _ = fs::remove_file(&disk_path);
                        return None;
                    }
                    let size = data.len();
                    let created_at = Instant::now();
                    {
                        let mut mem = self.memory.write();
                        mem.insert(
                            key.to_string(),
                            Entry {
                                data: data.clone(),
                                expires_at,
                                disk_path: Some(disk_path),
                                size,
                                created_at,
                            },
                        );
                        Self::enforce_limit(&mut mem, self.max_memory_entries);
                    }
                    return Some(data);
                }
                Err(_) => {}
            }
        }

        None
    }

    /// Store a value in both memory and disk.
    pub fn put(&self, key: &str, data: Vec<u8>) -> Result<(), AppError> {
        let disk_path = self.disk_root.join(sanitize_key(key));

        if let Some(parent) = disk_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(AppError::Internal(format!("Failed to create cache dir: {e}")));
            }
        }
        if let Err(e) = fs::write(&disk_path, &data) {
            return Err(AppError::Internal(format!("Failed to write cache file: {e}")));
        }

        let expires_at = Instant::now() + self.default_ttl;
        let size = data.len();
        let created_at = Instant::now();

        let mut mem = self.memory.write();

        // LinkedHashMap::insert moves to back (most recent) automatically
        mem.insert(
            key.to_string(),
            Entry {
                data,
                expires_at,
                disk_path: Some(disk_path),
                size,
                created_at,
            },
        );
        Self::enforce_limit(&mut mem, self.max_memory_entries);

        Ok(())
    }

    /// Remove a key from both memory and disk.
    pub fn evict(&self, key: &str) {
        {
            let mut mem = self.memory.write();
            mem.remove(key);
        }
        let disk_path = self.disk_root.join(sanitize_key(key));
        let _ = fs::remove_file(&disk_path);
    }

    /// Check if a key exists on disk.
    pub fn exists_on_disk(&self, key: &str) -> bool {
        let disk_path = self.disk_root.join(sanitize_key(key));
        disk_path.exists()
    }

    /// Evict LRU entries until under limit. LinkedHashMap pops front = LRU.
    fn enforce_limit(
        mem: &mut LinkedHashMap<String, Entry>,
        max_entries: usize,
    ) {
        while mem.len() > max_entries {
            if mem.pop_front().is_none() {
                break;
            }
        }
    }

    /// Background cleaner loop that periodically evicts expired entries.
    pub async fn cleaner_loop(&self, interval: Duration) {
        loop {
            tokio::time::sleep(interval).await;
            let expired_keys: Vec<String> = {
                let mem = self.memory.read();
                mem.iter()
                    .filter(|(_, entry)| Instant::now() >= entry.expires_at)
                    .map(|(key, _)| key.clone())
                    .collect()
            };
            for key in expired_keys {
                self.evict(&key);
            }
        }
    }
}

/// Normalize a cache key for filesystem use.
fn sanitize_key(key: &str) -> String {
    key.replace('/', "_").replace('\\', "_").replace('\0', "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("cache_tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn test_cache_put_get() {
        let dir = test_dir("put_get");
        let cache = Cache::new(dir, Duration::from_secs(60), 10);
        cache.put("key1", b"hello world".to_vec()).unwrap();
        let result = cache.get("key1");
        assert_eq!(result, Some(b"hello world".to_vec()));
    }

    #[test]
    fn test_cache_miss() {
        let dir = test_dir("miss");
        let cache = Cache::new(dir, Duration::from_secs(60), 10);
        let result = cache.get("nonexistent");
        assert_eq!(result, None);
    }

    #[test]
    fn test_cache_evict() {
        let dir = test_dir("evict");
        let cache = Cache::new(dir, Duration::from_secs(60), 10);
        cache.put("key1", b"data".to_vec()).unwrap();
        assert!(cache.get("key1").is_some());
        cache.evict("key1");
        assert!(cache.get("key1").is_none());
    }

    #[test]
    fn test_cache_expiry() {
        let dir = test_dir("expiry");
        let cache = Cache::new(dir, Duration::from_secs(0), 10);
        cache.put("key1", b"data".to_vec()).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let result = cache.get("key1");
        assert_eq!(result, None, "Expired entry should return None");
    }

    #[test]
    fn test_cache_disk_survives_restart() {
        let dir = test_dir("disk_survives");
        {
            let cache = Cache::new(dir.clone(), Duration::from_secs(60), 10);
            cache.put("persist_key", b"persisted data".to_vec()).unwrap();
        }
        let cache2 = Cache::new(dir, Duration::from_secs(60), 10);
        let result = cache2.get("persist_key");
        assert_eq!(result, Some(b"persisted data".to_vec()));
    }

    #[test]
    fn test_cache_exists_check() {
        let dir = test_dir("exists_check");
        let cache = Cache::new(dir, Duration::from_secs(60), 10);
        assert!(!cache.exists_on_disk("key1"));
        cache.put("key1", b"data".to_vec()).unwrap();
        assert!(cache.exists_on_disk("key1"));
    }

    #[test]
    fn test_cache_lru_eviction() {
        let dir = test_dir("lru_eviction");
        let cache = Cache::new(dir, Duration::from_secs(60), 3);

        cache.put("a", b"1".to_vec()).unwrap();
        cache.put("b", b"2".to_vec()).unwrap();
        cache.put("c", b"3".to_vec()).unwrap();
        cache.put("d", b"4".to_vec()).unwrap();

        let mem_size = {
            let mem = cache.memory.read();
            mem.len()
        };
        assert!(mem_size <= 3, "Memory should have at most 3 entries, got {mem_size}");

        assert!(cache.exists_on_disk("a"), "Evicted entry should still be on disk");
    }

    #[test]
    fn test_cache_lru_get_refreshes_order() {
        let dir = test_dir("lru_refresh");
        let cache = Cache::new(dir, Duration::from_secs(60), 3);

        cache.put("a", b"1".to_vec()).unwrap();
        cache.put("b", b"2".to_vec()).unwrap();
        cache.put("c", b"3".to_vec()).unwrap();

        let _ = cache.get("a");

        cache.put("d", b"4".to_vec()).unwrap();

        assert!(cache.get("a").is_some(), "Recently accessed 'a' should still be available");
        assert!(cache.exists_on_disk("b"), "Evicted from memory but still on disk");
    }

    #[actix_rt::test]
    async fn test_cache_background_cleaner() {
        let dir = test_dir("background_cleaner");
        let cache = std::sync::Arc::new(Cache::new(dir, Duration::from_millis(50), 10));

        cache.put("expire_soon", b"data".to_vec()).unwrap();

        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            cache_clone.cleaner_loop(Duration::from_millis(20)).await;
        });

        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(cache.get("expire_soon").is_none(), "Expired entry should be cleaned up");

        handle.abort();
    }
}
