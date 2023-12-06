use serde::Deserialize;

use crate::{
    cache::{Cache, IsCache},
    replace::{lru::Lru, lrudb::Lrudb, nmru::Nmru},
};

#[derive(Deserialize)]
struct CacheConfig {
    name: String,
    sets: usize,
    ways: usize,
    repl: String,
}

#[derive(Deserialize)]
pub struct Config {
    block_size: usize,
    caches: Vec<CacheConfig>,
}

impl Config {
    pub fn to_caches(self) -> Vec<Box<dyn IsCache>> {
        let block_size = self.block_size;
        self.caches
            .into_iter()
            .map(|cc| match cc.repl.as_str() {
                "nmru" => Box::new(Cache::new(
                    cc.name,
                    block_size,
                    cc.sets,
                    cc.ways,
                    Nmru::new(),
                )) as Box<dyn IsCache>,
                "lru" => Box::new(Cache::new(
                    cc.name,
                    block_size,
                    cc.sets,
                    cc.ways,
                    Lru::new(),
                )) as Box<dyn IsCache>,
                "lrudb" => Box::new(Cache::new(
                    cc.name,
                    block_size,
                    cc.sets,
                    cc.ways,
                    Lrudb::new(),
                )) as Box<dyn IsCache>,
                _ => panic!("Unrecognized replacement policy: {}", &cc.repl),
            })
            .collect()
    }
}
