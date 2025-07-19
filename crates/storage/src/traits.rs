use crate::{Result, KeyValue};
use std::sync::Arc;

/// Core database operations trait
pub trait Database: Send + Sync {
    /// Get a value by key
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    
    /// Put a key-value pair
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    
    /// Delete a key
    fn delete(&self, key: &[u8]) -> Result<()>;
    
    /// Check if a key exists
    fn contains(&self, key: &[u8]) -> Result<bool> {
        Ok(self.get(key)?.is_some())
    }
    
    /// Create a new batch for atomic writes
    fn batch(&self) -> Box<dyn WriteBatch>;
    
    /// Execute a batch of operations atomically
    fn write_batch(&self, batch: Box<dyn WriteBatch>) -> Result<()>;
    
    /// Create an iterator over the database
    fn iter(&self) -> Box<dyn DatabaseIterator + '_>;
    
    /// Create an iterator starting from a specific key
    fn iter_from(&self, start_key: &[u8]) -> Box<dyn DatabaseIterator + '_>;
    
    /// Create an iterator with a key prefix
    fn iter_prefix(&self, prefix: &[u8]) -> Box<dyn DatabaseIterator + '_>;
}

/// Batch operations for atomic writes
pub trait WriteBatch: Send {
    /// Add a put operation to the batch
    fn put(&mut self, key: &[u8], value: &[u8]);
    
    /// Add a delete operation to the batch
    fn delete(&mut self, key: &[u8]);
    
    /// Clear all operations in the batch
    fn clear(&mut self);
    
    /// Get the number of operations in the batch
    fn len(&self) -> usize;
    
    /// Check if the batch is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Helper method for downcasting
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Database iterator trait
pub trait DatabaseIterator: Send {
    /// Move to the next item
    fn next(&mut self) -> Option<Result<KeyValue>>;
    
    /// Seek to a specific key
    fn seek(&mut self, key: &[u8]) -> Option<Result<KeyValue>>;
}

/// Database factory trait for creating database instances
pub trait DatabaseFactory: Send + Sync {
    /// The database type this factory creates
    type Database: Database;
    
    /// Create a new database instance
    fn create(&self, path: &str) -> Result<Arc<Self::Database>>;
    
    /// Open an existing database
    fn open(&self, path: &str) -> Result<Arc<Self::Database>>;
    
    /// Check if a database exists at the given path
    fn exists(&self, path: &str) -> bool;
    
    /// Destroy a database at the given path
    fn destroy(&self, path: &str) -> Result<()>;
}

/// Transaction support for databases
pub trait TransactionalDatabase: Database {
    /// Begin a new transaction
    fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>>;
}

/// Database transaction trait
pub trait DatabaseTransaction: Database {
    /// Commit the transaction
    fn commit(self: Box<Self>) -> Result<()>;
    
    /// Rollback the transaction
    fn rollback(self: Box<Self>) -> Result<()>;
}

/// Extension trait for typed access to database
pub trait TypedDatabase: Database {
    /// Get a value and deserialize it
    fn get_typed<T: serde::de::DeserializeOwned>(&self, key: &[u8]) -> Result<Option<T>> {
        match self.get(key)? {
            Some(bytes) => {
                let value = bincode::deserialize(&bytes)
                    .map_err(|e| crate::StorageError::SerializationError(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
    
    /// Serialize and put a value
    fn put_typed<T: serde::Serialize>(&self, key: &[u8], value: &T) -> Result<()> {
        let bytes = bincode::serialize(value)
            .map_err(|e| crate::StorageError::SerializationError(e.to_string()))?;
        self.put(key, &bytes)
    }
}

/// Implement TypedDatabase for all types that implement Database
impl<T: Database + ?Sized> TypedDatabase for T {}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Mock implementation for testing trait definitions
    struct MockDatabase;
    struct MockBatch;
    struct MockIterator;
    
    impl Database for MockDatabase {
        fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
        
        fn put(&self, _key: &[u8], _value: &[u8]) -> Result<()> {
            Ok(())
        }
        
        fn delete(&self, _key: &[u8]) -> Result<()> {
            Ok(())
        }
        
        fn batch(&self) -> Box<dyn WriteBatch> {
            Box::new(MockBatch)
        }
        
        fn write_batch(&self, _batch: Box<dyn WriteBatch>) -> Result<()> {
            Ok(())
        }
        
        fn iter(&self) -> Box<dyn DatabaseIterator + '_> {
            Box::new(MockIterator)
        }
        
        fn iter_from(&self, _start_key: &[u8]) -> Box<dyn DatabaseIterator + '_> {
            Box::new(MockIterator)
        }
        
        fn iter_prefix(&self, _prefix: &[u8]) -> Box<dyn DatabaseIterator + '_> {
            Box::new(MockIterator)
        }
    }
    
    impl WriteBatch for MockBatch {
        fn put(&mut self, _key: &[u8], _value: &[u8]) {}
        fn delete(&mut self, _key: &[u8]) {}
        fn clear(&mut self) {}
        fn len(&self) -> usize { 0 }
        fn as_any(&self) -> &dyn std::any::Any { self }
    }
    
    impl DatabaseIterator for MockIterator {
        fn next(&mut self) -> Option<Result<KeyValue>> {
            None
        }
        
        fn seek(&mut self, _key: &[u8]) -> Option<Result<KeyValue>> {
            None
        }
    }
    
    #[test]
    fn test_database_trait() {
        let db = MockDatabase;
        assert!(db.get(b"test").unwrap().is_none());
        assert!(db.put(b"test", b"value").is_ok());
        assert!(db.delete(b"test").is_ok());
        assert!(!db.contains(b"test").unwrap());
    }
}