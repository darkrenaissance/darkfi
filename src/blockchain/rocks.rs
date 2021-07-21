use async_std::sync::Arc;
use std::marker::PhantomData;
use std::path::PathBuf;

use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::{Error, Result};

use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, DB};

pub enum IteratorMode {
    Start,
    End,
}

pub trait Column {
    const NAME: &'static str;
}

pub mod columns {
    pub struct Slabs;
    pub struct Nullifiers;
    pub struct MerkleRoots;
    pub struct CashierKeys;
}

impl Column for columns::Slabs {
    const NAME: &'static str = "slabs";
}

impl Column for columns::Nullifiers {
    const NAME: &'static str = "nullifiers";
}

impl Column for columns::MerkleRoots {
    const NAME: &'static str = "merkleroots";
}
impl Column for columns::CashierKeys {
    const NAME: &'static str = "cashierkeys";
}
pub struct Rocks {
    db: DB,
}

impl Rocks {
    pub fn new(path: &PathBuf) -> Result<Arc<Self>> {
        // column family options
        let cf_opts = Options::default();

        // default column family
        let default_cf =
            ColumnFamilyDescriptor::new(rocksdb::DEFAULT_COLUMN_FAMILY_NAME, cf_opts.clone());
        // slabs column family
        let slab_cf = ColumnFamilyDescriptor::new(columns::Slabs::NAME, cf_opts.clone());
        // nullifiers column family
        let nullifiers_cf = ColumnFamilyDescriptor::new(columns::Nullifiers::NAME, cf_opts.clone());
        // merkleroots column family
        let merkleroots_cf = ColumnFamilyDescriptor::new(columns::MerkleRoots::NAME, cf_opts.clone());
        // cashierkeypair column family
        let cashierkeys_cf = ColumnFamilyDescriptor::new(columns::CashierKeys::NAME, cf_opts);

        // column families
        let cfs = vec![default_cf, slab_cf, nullifiers_cf, merkleroots_cf, cashierkeys_cf];

        // database options
        let mut opt = Options::default();
        opt.create_if_missing(true);
        opt.create_missing_column_families(true);

        // open database with following options and cf
        let db = DB::open_cf_descriptors(&opt, path, cfs)?;

        Ok(Arc::new(Self { db }))
    }

    pub fn cf_handle<C>(&self) -> Result<&ColumnFamily>
    where
        C: Column,
    {
        self.db
            .cf_handle(C::NAME)
            .ok_or(Error::RocksdbError("unknown column".to_string()))
    }

    pub fn put_cf(&self, cf: &ColumnFamily, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.db.put_cf(cf, key, value)?;
        Ok(())
    }

    pub fn get_cf(&self, cf: &ColumnFamily, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let val = self.db.get_cf(cf, key)?;
        Ok(val)
    }

    pub fn key_exist_cf(&self, cf: &ColumnFamily, key: Vec<u8>) -> Result<bool> {
        let val = self.db.get_cf(cf, key)?;
        Ok(val.is_some())
    }

    pub fn iterator(&self, cf: &ColumnFamily, iterator_mode: IteratorMode) -> rocksdb::DBIterator {
        let iterator_mode = match iterator_mode {
            IteratorMode::Start => rocksdb::IteratorMode::Start,
            IteratorMode::End => rocksdb::IteratorMode::End,
        };
        self.db.iterator_cf(cf, iterator_mode)
    }

    pub fn destroy(path: &PathBuf) -> Result<()> {
        DB::destroy(&Options::default(), path)?;
        Ok(())
    }
}

pub struct RocksColumn<T: Column> {
    rocks: Arc<Rocks>,
    column: PhantomData<T>,
}

impl<T: Column> RocksColumn<T> {
    pub fn new(rocks: Arc<Rocks>) -> RocksColumn<T> {
        RocksColumn {
            rocks,
            column: PhantomData,
        }
    }
    fn cf_handle(&self) -> Result<&ColumnFamily> {
        self.rocks.cf_handle::<T>()
    }

    pub fn put(&self, key: impl Encodable, value: impl Encodable) -> Result<()> {
        let key = serialize(&key);
        let value = serialize(&value);
        let cf = self.cf_handle()?;
        self.rocks.put_cf(cf, key, value)?;
        Ok(())
    }

    pub fn get(&self, key: impl Encodable) -> Result<Option<Vec<u8>>> {
        let key = serialize(&key);
        let cf = self.cf_handle()?;
        let val = self.rocks.get_cf(cf, key)?;
        Ok(val)
    }

    pub fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<Option<D>> {
        let value = self.get(key)?;
        match value {
            Some(v) => {
                let v: D = deserialize(&v)?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    pub fn key_exist(&self, key: impl Encodable) -> Result<bool> {
        let key = serialize(&key);
        let cf = self.cf_handle()?;
        let val = self.rocks.key_exist_cf(cf, key)?;
        Ok(val)
    }

    pub fn iterator(&self, iterator_mode: IteratorMode) -> Result<rocksdb::DBIterator> {
        let cf = self.cf_handle()?;
        let iter = self.rocks.iterator(cf, iterator_mode);
        Ok(iter)
    }
}
