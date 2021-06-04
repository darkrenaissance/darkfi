use std::path::Path;

use crate::{Error, Result};

use rocksdb::{ColumnFamilyDescriptor, Options, DB, ColumnFamily};

pub enum IteratorMode{
    Start,
    End
}

pub trait Column {
    const NAME: &'static str;
}

pub mod columns {
    pub struct Slabs;
    pub struct Nullifiers;
    pub struct MerkleTree;
}

impl Column for columns::Slabs {
    const NAME: &'static str = "slabs";
}

impl Column for columns::Nullifiers {
    const NAME: &'static str = "nullifiers";
}

impl Column for columns::MerkleTree {
    const NAME: &'static str = "merkletree";
}

pub struct Rocks {
    db: DB,
}

impl Rocks {
    pub fn new(path: &Path) -> Result<Self> {
        // column family options
        let cf_opts = Options::default();

        // default column family
        let default_cf =
            ColumnFamilyDescriptor::new(rocksdb::DEFAULT_COLUMN_FAMILY_NAME, cf_opts.clone());
        // slabs column family
        let slab_cf = ColumnFamilyDescriptor::new(columns::Slabs::NAME, cf_opts.clone());
        // nullifiers column family
        let nullifiers_cf = ColumnFamilyDescriptor::new(columns::Nullifiers::NAME, cf_opts.clone());
        // merkletree column family
        let merkletree_cf = ColumnFamilyDescriptor::new(columns::MerkleTree::NAME, cf_opts);

        // column families
        let cfs = vec![default_cf, slab_cf, nullifiers_cf, merkletree_cf];

        // database options
        let mut opt = Options::default();
        opt.create_if_missing(true);
        opt.create_missing_column_families(true);

        // open database with following options and cf
        let db = DB::open_cf_descriptors(&opt, path, cfs)?;

        Ok(Self { db })
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
        if let None = val {
            return Ok(false);
        };
        Ok(true)
    }

    pub fn iterator(&self, cf: &ColumnFamily, iterator_mode: IteratorMode) -> rocksdb::DBIterator{
        let iterator_mode = match iterator_mode {
            IteratorMode::Start => rocksdb::IteratorMode::Start,
            IteratorMode::End => rocksdb::IteratorMode::End,
        };
        self.db.iterator_cf(cf, iterator_mode)
    }

    pub fn destroy(path: &Path) -> Result<()> {
        DB::destroy(&Options::default(), path)?;
        Ok(())
    }
}
