use cache::{
    Cache,
    CacheRead,
    CacheWrite,
    Storage,
};
use errors::*;
use futures_cpupool::CpuPool;
use lmdb::{Database, Environment, Transaction, WriteFlags};
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::result::{Result as SResult};
use std::time::Instant;


#[derive(Clone)]
pub struct LMDBCache {
    env: Arc<Environment>,
    pool: CpuPool,
    path: String,
    db: Arc<Database>,
}

trait ResultExt<T, E> {
    fn ensure<F>(self, f: F) -> SResult<T, E>
        where F: FnOnce();

    fn transform<F, U, R>(self, f: F) -> SResult<U, R>
        where F: FnOnce(SResult<T, E>) -> SResult<U, R>;
}

impl<T, E> ResultExt<T, E> for SResult<T, E> {
    fn ensure<F: FnOnce()>(self, f: F) -> SResult<T, E> {
        f();
        self
    }

    fn transform<F, U, R>(self, f: F) -> SResult<U, R>
        where F: FnOnce(SResult<T, E>) -> SResult<U, R>
    {
        f(self)
    }
}

impl LMDBCache {
    pub fn new(path: &Path, map_size: u64, cpu_pool: &CpuPool) -> Result<LMDBCache> {
        let env = Environment::new()
            .set_map_size(map_size as usize)
            .open(path)?;

        let db = env.open_db(None)?;

        Ok(LMDBCache {
            pool: cpu_pool.clone(),
            env: Arc::new(env),
            db: Arc::new(db),
            path: path.to_str().expect("path converts to string").to_owned(),
        })
    }

    fn get(&self, k: Vec<u8>) -> Result<Cache> {
        let tx = self.env.begin_ro_txn()?;

        let res =
            tx.get(*self.db, &k)
                .map_err(|e| e.into())
                .and_then(|vb| CacheRead::from(Cursor::new(Vec::from(vb))).map(Cache::Hit));

        tx.commit()?;

        res
    }

    fn put(&self, k: Vec<u8>, entry: CacheWrite) -> Result<Duration> {
        let start = Instant::now();
        let d = entry.finish()?;
        let mut tx = self.env.begin_rw_txn()?;

        let res = {
            tx.put(*self.db, &k, &d, WriteFlags::empty())
        };

        let rv =
            match res {
                ok @ Ok(()) => { tx.commit()?; ok },
                err @ Err(_) => { tx.abort(); err },
            };

        rv
            .map(|_| start.elapsed())
            .map_err(|e| e.into())
    }
}

impl Storage for LMDBCache {
    fn get(&self, key: &str) -> SFuture<Cache> {
        let kb = Vec::from(key);
        let me = self.clone();
        Box::new(
            self.pool.spawn_fn(move || {
                me.get(kb)
            })
        )
    }

    fn put(&self, key: &str, entry: CacheWrite) -> SFuture<Duration> {
        let k = Vec::from(key);
        let me = self.clone();
        Box::new(
            self.pool.spawn_fn(move || {
                me.put(k, entry)
            })
        )
    }

    fn location(&self) -> String { self.path.to_owned() }
    fn current_size(&self) -> Option<u64> { None }
    fn max_size(&self) -> Option<u64> { None }
}
