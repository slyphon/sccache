use cache::{
    Cache,
    CacheRead,
    CacheWrite,
    Storage,
};
use errors::*;
use futures_cpupool::CpuPool;
use lmdb;
use lmdb::Database;
use lmdb::DatabaseFlags;
use lmdb::Environment;
use lmdb::EnvironmentBuilder;
use std::cell::RefCell;
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;


#[derive(Clone)]
pub struct LMDBCache {
    env: Arc<Environment>,
    pool: CpuPool,
    path: String,
    db: Arc<Database>,
}


impl LMDBCache {
    pub fn new(path: &Path, map_size: usize, cpu_pool: &CpuPool) -> Result<LMDBCache> {
        let mut env = Environment::new()
            .set_map_size(map_size)
            .open(path)?;

        let db = env.open_db(None)?;

        Ok(LMDBCache {
            pool: cpu_pool.clone(),
            env: Arc::new(env),
            db: Arc::new(db),
            path: path.to_str().expect("path converts to string").to_owned(),
        })
    }
}

trait ResultExt<T> {
    fn transform<F, U>(self, f: F) -> Result<U>
        where F: FnOnce(Result<T>) -> Result<U>;
}

impl<T> ResultExt<T> for Result<T> {
    fn transform<F, U>(self, f: F) -> Result<U>
        where F: FnOnce(Result<T>) -> Result<U>
    {
        f(self)
    }
}

impl Storage for LMDBCache {
    fn get(&self, key: &str) -> SFuture<Cache> {
        let key = key.to_owned();
        let kb = key.as_bytes();
        let me = self.clone();
        self.pool.spawn_fn(move || {
            let tx = me.env.begin_ro_txn()?;
            tx.get(*me.db, kb)
                .map(|vbytes| CacheRead::from(Cursor::new(Vec::from(vbytes))))
                .map(Cache::Hit)
                .transform(|r| {
                    match r {
                        ok @ Ok(_) => ok,
                        Err(lmdb::Error::NotFound) => Ok(Cache::Miss),
                        err @ Err(_) => err,
                    }
                })
        })

    }

    fn put(&self, key: &str, entry: CacheWrite) -> SFuture<Duration> {
        unimplemented!()
    }

    fn location(&self) -> String { self.path }
    fn current_size(&self) -> Option<u64> { None }
    fn max_size(&self) -> Option<u64> { None }
}
