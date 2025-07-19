use crate::{Database, DatabaseIterator, KeyValue, Result, StorageError, WriteBatch};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// In-memory database implementation using BTreeMap
#[derive(Debug, Clone)]
pub struct MemoryDatabase {
    data: Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
}

impl MemoryDatabase {
    /// Create a new empty in-memory database
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
    
    /// Get the number of entries in the database
    pub fn len(&self) -> usize {
        self.data.read().unwrap().len()
    }
    
    /// Check if the database is empty
    pub fn is_empty(&self) -> bool {
        self.data.read().unwrap().is_empty()
    }
    
    /// Clear all entries from the database
    pub fn clear(&self) {
        self.data.write().unwrap().clear();
    }
}

impl Default for MemoryDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl Database for MemoryDatabase {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.read().unwrap().get(key).cloned())
    }
    
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.data.write().unwrap().insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    
    fn delete(&self, key: &[u8]) -> Result<()> {
        self.data.write().unwrap().remove(key);
        Ok(())
    }
    
    fn batch(&self) -> Box<dyn WriteBatch> {
        Box::new(MemoryBatch::new())
    }
    
    fn write_batch(&self, batch: Box<dyn WriteBatch>) -> Result<()> {
        let batch = batch.as_any()
            .downcast_ref::<MemoryBatch>()
            .ok_or_else(|| StorageError::InvalidData("Invalid batch type".to_string()))?;
        
        let mut data = self.data.write().unwrap();
        
        for op in &batch.operations {
            match op {
                BatchOp::Put(key, value) => {
                    data.insert(key.clone(), value.clone());
                }
                BatchOp::Delete(key) => {
                    data.remove(key);
                }
            }
        }
        
        Ok(())
    }
    
    fn iter(&self) -> Box<dyn DatabaseIterator + '_> {
        let data = self.data.read().unwrap();
        let entries: Vec<_> = data.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Box::new(MemoryIterator::new(entries))
    }
    
    fn iter_from(&self, start_key: &[u8]) -> Box<dyn DatabaseIterator + '_> {
        let data = self.data.read().unwrap();
        let entries: Vec<_> = data.range(start_key.to_vec()..)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Box::new(MemoryIterator::new(entries))
    }
    
    fn iter_prefix(&self, prefix: &[u8]) -> Box<dyn DatabaseIterator + '_> {
        let data = self.data.read().unwrap();
        let entries: Vec<_> = data.iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Box::new(MemoryIterator::new(entries))
    }
}

/// Batch operations for memory database
#[derive(Debug)]
enum BatchOp {
    Put(Vec<u8>, Vec<u8>),
    Delete(Vec<u8>),
}

/// Memory database batch implementation
#[derive(Debug)]
struct MemoryBatch {
    operations: Vec<BatchOp>,
}

impl MemoryBatch {
    fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }
    
    /// Helper method for downcasting
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WriteBatch for MemoryBatch {
    fn put(&mut self, key: &[u8], value: &[u8]) {
        self.operations.push(BatchOp::Put(key.to_vec(), value.to_vec()));
    }
    
    fn delete(&mut self, key: &[u8]) {
        self.operations.push(BatchOp::Delete(key.to_vec()));
    }
    
    fn clear(&mut self) {
        self.operations.clear();
    }
    
    fn len(&self) -> usize {
        self.operations.len()
    }
}

/// Memory database iterator
struct MemoryIterator {
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    position: usize,
}

impl MemoryIterator {
    fn new(entries: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        Self {
            entries,
            position: 0,
        }
    }
}

impl DatabaseIterator for MemoryIterator {
    fn next(&mut self) -> Option<Result<KeyValue>> {
        if self.position < self.entries.len() {
            let entry = self.entries[self.position].clone();
            self.position += 1;
            Some(Ok(entry))
        } else {
            None
        }
    }
    
    fn seek(&mut self, key: &[u8]) -> Option<Result<KeyValue>> {
        // Find the first entry with key >= the search key
        self.position = self.entries.iter()
            .position(|(k, _)| k.as_slice() >= key)
            .unwrap_or(self.entries.len());
        
        self.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_database_basic() {
        let db = MemoryDatabase::new();
        
        // Test put and get
        db.put(b"key1", b"value1").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(b"value1".to_vec()));
        
        // Test update
        db.put(b"key1", b"value2").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(b"value2".to_vec()));
        
        // Test delete
        db.delete(b"key1").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), None);
        
        // Test non-existent key
        assert_eq!(db.get(b"nonexistent").unwrap(), None);
    }
    
    #[test]
    fn test_memory_database_batch() {
        let db = MemoryDatabase::new();
        
        let mut batch = db.batch();
        batch.put(b"key1", b"value1");
        batch.put(b"key2", b"value2");
        batch.delete(b"key3");
        
        assert_eq!(batch.len(), 3);
        
        db.write_batch(batch).unwrap();
        
        assert_eq!(db.get(b"key1").unwrap(), Some(b"value1".to_vec()));
        assert_eq!(db.get(b"key2").unwrap(), Some(b"value2".to_vec()));
        assert_eq!(db.get(b"key3").unwrap(), None);
    }
    
    #[test]
    fn test_memory_database_iterator() {
        let db = MemoryDatabase::new();
        
        db.put(b"a", b"1").unwrap();
        db.put(b"b", b"2").unwrap();
        db.put(b"c", b"3").unwrap();
        
        let mut iter = db.iter();
        let (k1, v1) = iter.next().unwrap().unwrap();
        assert_eq!(k1, b"a");
        assert_eq!(v1, b"1");
        
        let (k2, v2) = iter.next().unwrap().unwrap();
        assert_eq!(k2, b"b");
        assert_eq!(v2, b"2");
        
        let (k3, v3) = iter.next().unwrap().unwrap();
        assert_eq!(k3, b"c");
        assert_eq!(v3, b"3");
        
        assert!(iter.next().is_none());
    }
    
    #[test]
    fn test_memory_database_iter_from() {
        let db = MemoryDatabase::new();
        
        db.put(b"a", b"1").unwrap();
        db.put(b"b", b"2").unwrap();
        db.put(b"c", b"3").unwrap();
        db.put(b"d", b"4").unwrap();
        
        let mut iter = db.iter_from(b"b");
        let (k1, v1) = iter.next().unwrap().unwrap();
        assert_eq!(k1, b"b");
        assert_eq!(v1, b"2");
        
        let (k2, v2) = iter.next().unwrap().unwrap();
        assert_eq!(k2, b"c");
        assert_eq!(v2, b"3");
    }
    
    #[test]
    fn test_memory_database_iter_prefix() {
        let db = MemoryDatabase::new();
        
        db.put(b"prefix1", b"1").unwrap();
        db.put(b"prefix2", b"2").unwrap();
        db.put(b"other", b"3").unwrap();
        db.put(b"prefix3", b"4").unwrap();
        
        let mut iter = db.iter_prefix(b"prefix");
        let mut count = 0;
        while let Some(Ok((key, _))) = iter.next() {
            assert!(key.starts_with(b"prefix"));
            count += 1;
        }
        assert_eq!(count, 3);
    }
    
    #[test]
    fn test_contains() {
        let db = MemoryDatabase::new();
        
        db.put(b"key", b"value").unwrap();
        assert!(db.contains(b"key").unwrap());
        assert!(!db.contains(b"nonexistent").unwrap());
    }
}