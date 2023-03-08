/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use sled::{Config, Db, IVec};

#[derive(Debug, PartialEq)]
struct MyBullshitError;

struct SledCache {
    // A sequence to execute
    sequence: Vec<(IVec, Option<IVec>)>
}

impl SledCache {
    fn new() -> Self {
        Self {sequence: vec![]}
    }
    
    // Check if a key is cached
    fn get<K: AsRef<[u8]> + Copy>(&self, key: K) -> Option<IVec> 
    where 
        K: Into<IVec>
    {
        let key = key.into();
        // TODO: optimize this
        for pair in &self.sequence {
            if pair.0 == key {
                return pair.1.clone()
            }
        }
        None
    }
    
    // Set a key to a new value
    fn push<K>(&mut self, key: K, value: Option<IVec>)
    where
        K: Into<IVec>,
    {
        self.sequence.push((key.into(), value));
    }
    
    // Set a key to a new value at front of sequence
    fn insert<K>(&mut self, key: K, value: Option<IVec>)
    where
        K: Into<IVec>,
    {
        self.sequence.insert(0, (key.into(), value));
    }
}

struct SledOverlay {
    // Actual sled storage
    sled: Db,    
    // Cached writes sequence
    // NOTE: this should be per Tree
    writes: SledCache,
}

impl SledOverlay {
    fn new(sled: Db) -> Self {
        Self {sled, writes: SledCache::new()}
    }
    
    // Check if a key is cached for changes, otherwise get it
    // from sled directly
    fn get<K: AsRef<[u8]> + Copy>(&self, key: K) -> Result<Option<IVec>, sled::Error> 
    where 
        K: Into<IVec>
    {
        if let Some(v) = self.writes.get(&key.into()) {
            return Ok(Some(v.clone()))
        }
        
        self.sled.get(key)
    }
    
    // Set a key to a new value
    fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: Into<IVec>,
        V: Into<IVec>,
    {
        self.writes.push(key, Some(value.into()));
    }
    
    // Apply all writes and flush sled db
    fn flush(&mut self) -> Result<(), sled::Error> {
        let mut rollback = SledCache::new();
        for (k, v_opt) in &self.writes.sequence {
            if let Some(v) = v_opt {
                let old = self.sled.insert(k, v)?;
                rollback.insert(k, old);
            } else {
                let old = self.sled.remove(k)?;
                rollback.insert(k, old);
            }
        }
        
        self.sled.flush()?;
        self.writes = SledCache::new();
        
        Ok(())
    }
    
    // Apply all writes, rollback to previous and flush sled db
    fn flush_rollback(&mut self) -> Result<(), sled::Error> {
        // Rollback keeps track of writes in reverse order
        let mut rollback = SledCache::new();
        let res = {
            for (k, v_opt) in &self.writes.sequence {
                if let Some(v) = v_opt {
                    let old = self.sled.insert(k, v)?;
                    rollback.insert(k, old);
                } else {
                    let old = self.sled.remove(k)?;
                    rollback.insert(k, old);
                }
            }
            
            MyBullshitError
        };
        
        // Execute rollback in case of error
        if res == MyBullshitError {
            for (k, v_opt) in &rollback.sequence {
                if let Some(v) = v_opt {
                    let _old = self.sled.insert(k, v)?;
                } else {
                    let _old = self.sled.remove(k)?;
                }
            }
        }
        
        self.sled.flush()?;
        self.writes = SledCache::new();
        
        Ok(())
    }
}

// This examples showcases a serial apply of writes
// where if one fails we can detect it and rollback.
fn main() -> Result<(), sled::Error> {
    // Initialize database overlay
    let config = Config::new().temporary(true);
    let db = config.open()?;
    let mut overlay = SledOverlay::new(db.clone());
    
    // Insert some values to cache
    overlay.insert("key_a", "val_a");
    overlay.insert("key_b", "val_b");
    overlay.insert("key_c", "val_c");
    
    assert_eq!(overlay.get("key_a")?, Some("val_a".into()));
    assert_eq!(overlay.get("key_b")?, Some("val_b".into()));
    assert_eq!(overlay.get("key_c")?, Some("val_c".into()));
    
    // Verify they are not in sled
    assert_eq!(db.get(b"key_a")?, None);
    assert_eq!(db.get(b"key_b")?, None);
    assert_eq!(db.get(b"key_c")?, None);
    
    // Now we write them to sled
    overlay.flush()?;
    
    // Verify sled contains keys
    assert_eq!(db.get("key_a")?, Some("val_a".into()));
    assert_eq!(db.get("key_b")?, Some("val_b".into()));
    assert_eq!(db.get("key_c")?, Some("val_c".into()));
    
    // Perform the same steps, but assume an error occured
    // during flashing, so the sled performs a rollback.
    let config = Config::new().temporary(true);
    let db = config.open()?;
    let mut overlay = SledOverlay::new(db.clone());
    
    // Insert some values to cache
    overlay.insert("key_a", "val_a");
    overlay.insert("key_b", "val_b");
    overlay.insert("key_c", "val_c");
    
    assert_eq!(overlay.get("key_a")?, Some("val_a".into()));
    assert_eq!(overlay.get("key_b")?, Some("val_b".into()));
    assert_eq!(overlay.get("key_c")?, Some("val_c".into()));
    
    // Verify they are not in sled
    assert_eq!(db.get(b"key_a")?, None);
    assert_eq!(db.get(b"key_b")?, None);
    assert_eq!(db.get(b"key_c")?, None);
    
    // Now we write them to sled
    overlay.flush_rollback()?;
    
    // Verify they are not in sled
    assert_eq!(db.get(b"key_a")?, None);
    assert_eq!(db.get(b"key_b")?, None);
    assert_eq!(db.get(b"key_c")?, None);

    Ok(())
}
