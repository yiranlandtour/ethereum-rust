use rocksdb::{DB, Options, WriteBatch as RocksWriteBatch, IteratorMode, Direction};
use std::path::Path;
use std::sync::Arc;
use std::any::Any;

use crate::{Database, StorageError, Result, KeyValue, DatabaseIterator, WriteBatch as WriteBatchTrait};

pub struct RocksDatabase {
    db: Arc<DB>,
}

impl RocksDatabase {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        
        // Performance optimizations
        opts.set_write_buffer_size(256 * 1024 * 1024); // 256MB
        opts.set_max_write_buffer_number(4);
        opts.set_target_file_size_base(256 * 1024 * 1024); // 256MB
        opts.set_max_open_files(10000);
        opts.set_compaction_style(rocksdb::DBCompactionStyle::Level);
        opts.set_bytes_per_sync(1024 * 1024); // 1MB
        
        // Enable statistics for monitoring
        opts.enable_statistics();
        opts.set_stats_dump_period_sec(600); // 10 minutes
        
        let db = DB::open(&opts, path)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))?;
        
        Ok(Self {
            db: Arc::new(db),
        })
    }
    
    pub fn destroy<P: AsRef<Path>>(path: P) -> Result<()> {
        DB::destroy(&Options::default(), path)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
    }
    
    pub fn flush(&self) -> Result<()> {
        self.db.flush()
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
    }
    
    pub fn compact_range(&self, start: Option<&[u8]>, end: Option<&[u8]>) {
        self.db.compact_range(start, end);
    }
    
    pub fn create_snapshot(&self) -> RocksSnapshot {
        RocksSnapshot {
            snapshot: self.db.snapshot(),
        }
    }
}

impl Database for RocksDatabase {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.db.get(key)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
    }
    
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.put(key, value)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
    }
    
    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.delete(key)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
    }
    
    fn contains(&self, key: &[u8]) -> Result<bool> {
        self.db.key_may_exist(key)
            .then(|| self.db.get(key))
            .transpose()
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
            .map(|v| v.flatten().is_some())
    }
    
    fn batch(&self) -> Box<dyn WriteBatchTrait> {
        Box::new(RocksBatch::new())
    }
    
    fn write_batch(&self, batch: Box<dyn WriteBatchTrait>) -> Result<()> {
        // Downcast the batch to RocksBatch
        let rocks_batch = batch.as_any()
            .downcast_ref::<RocksBatch>()
            .ok_or_else(|| StorageError::InvalidData("Invalid batch type".to_string()))?;
        
        self.db.write(rocks_batch.batch.clone())
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
    }
    
    fn iter(&self) -> Box<dyn DatabaseIterator + '_> {
        Box::new(RocksIterator {
            iter: self.db.iterator(IteratorMode::Start),
        })
    }
    
    fn iter_from(&self, start_key: &[u8]) -> Box<dyn DatabaseIterator + '_> {
        Box::new(RocksIterator {
            iter: self.db.iterator(IteratorMode::From(start_key, Direction::Forward)),
        })
    }
    
    fn iter_prefix(&self, prefix: &[u8]) -> Box<dyn DatabaseIterator + '_> {
        Box::new(RocksPrefixIterator {
            iter: self.db.prefix_iterator(prefix),
        })
    }
}

pub struct RocksBatch {
    batch: RocksWriteBatch,
}

impl RocksBatch {
    pub fn new() -> Self {
        Self {
            batch: RocksWriteBatch::default(),
        }
    }
}

impl WriteBatchTrait for RocksBatch {
    fn put(&mut self, key: &[u8], value: &[u8]) {
        self.batch.put(key, value);
    }
    
    fn delete(&mut self, key: &[u8]) {
        self.batch.delete(key);
    }
    
    fn clear(&mut self) {
        self.batch.clear();
    }
    
    fn len(&self) -> usize {
        self.batch.len()
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct RocksIterator<'a> {
    iter: rocksdb::DBIterator<'a>,
}

impl<'a> DatabaseIterator for RocksIterator<'a> {
    fn next(&mut self) -> Option<Result<KeyValue>> {
        self.iter.next().map(|result| {
            result
                .map_err(|e| StorageError::DatabaseError(e.to_string()))
                .map(|(k, v)| (k.to_vec(), v.to_vec()))
        })
    }
    
    fn seek(&mut self, key: &[u8]) -> Option<Result<KeyValue>> {
        self.iter.seek(key);
        self.next()
    }
}

pub struct RocksPrefixIterator<'a> {
    iter: rocksdb::DBIterator<'a>,
}

impl<'a> DatabaseIterator for RocksPrefixIterator<'a> {
    fn next(&mut self) -> Option<Result<KeyValue>> {
        self.iter.next().map(|result| {
            result
                .map_err(|e| StorageError::DatabaseError(e.to_string()))
                .map(|(k, v)| (k.to_vec(), v.to_vec()))
        })
    }
    
    fn seek(&mut self, key: &[u8]) -> Option<Result<KeyValue>> {
        self.iter.seek(key);
        self.next()
    }
}

pub struct RocksSnapshot {
    snapshot: rocksdb::Snapshot<'static>,
}

impl RocksSnapshot {
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.snapshot.get(key)
            .map_err(|e| StorageError::DatabaseError(e.to_string()))
    }
    
    pub fn iter(&self) -> impl Iterator<Item = Result<KeyValue>> + '_ {
        self.snapshot
            .iterator(IteratorMode::Start)
            .map(|r| {
                r.map_err(|e| StorageError::DatabaseError(e.to_string()))
                    .map(|(k, v)| (k.to_vec(), v.to_vec()))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_rocksdb_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDatabase::open(temp_dir.path()).unwrap();
        
        // Test put and get
        let key = b"test_key";
        let value = b"test_value";
        db.put(key, value).unwrap();
        
        let retrieved = db.get(key).unwrap();
        assert_eq!(retrieved, Some(value.to_vec()));
        
        // Test contains
        assert!(db.contains(key).unwrap());
        assert!(!db.contains(b"non_existent").unwrap());
        
        // Test delete
        db.delete(key).unwrap();
        assert!(!db.contains(key).unwrap());
    }
    
    #[test]
    fn test_batch_operations() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDatabase::open(temp_dir.path()).unwrap();
        
        let mut batch = db.batch();
        for i in 0..100 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            batch.put(key.as_bytes(), value.as_bytes());
        }
        
        db.write_batch(batch).unwrap();
        
        // Verify all keys exist
        for i in 0..100 {
            let key = format!("key_{}", i);
            assert!(db.contains(key.as_bytes()).unwrap());
        }
    }
    
    #[test]
    fn test_iterator() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDatabase::open(temp_dir.path()).unwrap();
        
        // Insert test data
        for i in 0..10 {
            let key = format!("key_{:02}", i);
            let value = format!("value_{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }
        
        // Test iteration
        let mut iter = db.iter();
        let mut count = 0;
        while let Some(result) = iter.next() {
            result.unwrap();
            count += 1;
        }
        assert_eq!(count, 10);
        
        // Test prefix iteration
        db.put(b"prefix_1", b"val1").unwrap();
        db.put(b"prefix_2", b"val2").unwrap();
        db.put(b"other", b"val3").unwrap();
        
        let mut prefix_iter = db.iter_prefix(b"prefix_");
        let mut prefix_count = 0;
        while let Some(result) = prefix_iter.next() {
            result.unwrap();
            prefix_count += 1;
        }
        assert_eq!(prefix_count, 2);
    }
    
    #[test]
    fn test_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDatabase::open(temp_dir.path()).unwrap();
        
        db.put(b"key1", b"value1").unwrap();
        
        let snapshot = db.create_snapshot();
        
        // Modify after snapshot
        db.put(b"key1", b"value2").unwrap();
        db.put(b"key2", b"value2").unwrap();
        
        // Snapshot should see old value
        assert_eq!(snapshot.get(b"key1").unwrap(), Some(b"value1".to_vec()));
        assert_eq!(snapshot.get(b"key2").unwrap(), None);
        
        // Current db should see new values
        assert_eq!(db.get(b"key1").unwrap(), Some(b"value2".to_vec()));
        assert_eq!(db.get(b"key2").unwrap(), Some(b"value2".to_vec()));
    }
}