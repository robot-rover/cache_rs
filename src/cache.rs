use std::{
    iter,
    ops::{Not, Range},
};

use serde::Serialize;

use crate::{
    cpu::Cpu,
    replace::{AccessResult, Replace, MakeS},
};

#[derive(Debug)]
pub struct Addr {
    pub offset: usize,
    pub set: usize,
    pub tag: usize,
}

#[derive(Debug)]
pub struct BitSection {
    shift: usize,
    mask: usize,
}

impl BitSection {
    fn apply(&self, num: usize) -> usize {
        (num >> self.shift) & self.mask
    }
}

#[derive(Serialize)]
pub struct CacheStats {
    name: String,
    misses: u64,
    hits: u64,
    miss_rate: f64,
    mpki: f64,
    reuse: f64,
    lifetime: f64,
    efficiency: f64,
}

#[derive(Debug)]
pub struct Cache<S: MakeS, B: Default, R: Replace<S, B>> {
    name: String,
    pub blocks: Vec<Block<B>>,
    pub set_data: Vec<S>,
    pub block_size: usize,
    pub n_ways: usize,
    pub n_sets: usize,
    offset_sec: BitSection,
    set_sec: BitSection,
    tag_sec: BitSection,
    pub repl: R,
    hits: u64,
    misses: u64,
}

impl<S: MakeS, B: Default, R: Replace<S, B>> Cache<S, B, R> {
    pub fn new(name: String, block_size: usize, n_sets: usize, n_ways: usize, repl: R) -> Self {
        assert!(n_ways.is_power_of_two());

        assert!(block_size.is_power_of_two());
        let offset_sec = BitSection {
            shift: 0,
            mask: block_size - 1,
        };

        assert!(n_sets.is_power_of_two());
        let set_shift = block_size.ilog2() as usize;
        let set_sec = BitSection {
            shift: set_shift,
            mask: n_sets - 1,
        };

        let tag_shift = n_sets.ilog2() as usize + set_shift;
        let tag_sec = BitSection {
            shift: tag_shift,
            mask: 0usize.not(),
        };

        Cache {
            name,
            blocks: iter::repeat_with(|| Block::default())
                .take(n_sets * n_ways)
                .collect(),
            set_data: iter::repeat_with(|| S::new(n_ways)).take(n_sets).collect(),
            block_size,
            n_ways,
            n_sets,
            offset_sec,
            set_sec,
            tag_sec,
            repl,
            hits: 0,
            misses: 0,
        }
    }
}

pub trait IsCache {
    fn access(&mut self, cpu: &mut Cpu, addr: Addr) -> AccessResult;
    fn split_addr(&self, addr: usize) -> Addr;
    fn get_set(&mut self, set: usize) -> Range<usize>;
    fn hit(&mut self);
    fn miss(&mut self);
    fn clear_stats(&mut self);
    fn make_stats(&self, cpu: &Cpu) -> CacheStats;
}

impl<S: MakeS, B: Default, R: Replace<S, B>> IsCache for Cache<S, B, R> {
    fn access(&mut self, cpu: &mut Cpu, addr: Addr) -> AccessResult {
        R::access(cpu, self, addr)
    }

    fn split_addr(&self, addr: usize) -> Addr {
        let offset = self.offset_sec.apply(addr);
        let set = self.set_sec.apply(addr);
        let tag = self.tag_sec.apply(addr);
        Addr { offset, set, tag }
    }

    fn get_set(&mut self, set: usize) -> Range<usize> {
        set * self.n_ways..(set + 1) * self.n_ways
    }

    fn hit(&mut self) {
        self.hits += 1;
    }

    fn miss(&mut self) {
        self.misses += 1;
    }

    fn clear_stats(&mut self) {
        self.misses = 0;
        self.hits = 0;
        for block in &mut self.blocks {
            block.live_dur = 0;
            block.dead_dur = 0;
            block.alloc_count = if block.alloc_count > 0 { 1 } else { 0 };
            block.access_count = 0;
        }
    }

    fn make_stats(&self, cpu: &Cpu) -> CacheStats {
        let total_alloc: f64 = self.blocks.iter().map(|b| b.alloc_count as f64).sum();
        let total_dead: f64 = self.blocks.iter().map(|b| b.dead_dur as f64).sum();
        let total_live: f64 = self.blocks.iter().map(|b| b.live_dur as f64).sum();
        let total_both: f64 = total_dead + total_live;

        let total_access = (self.misses + self.hits) as f64;

        let miss_rate = self.misses as f64 / total_access;
        let mpki = self.misses as f64 / cpu.instr_idx as f64;
        let reuse = total_access / total_alloc;
        let lifetime = total_both / total_alloc;
        let efficiency = total_live / total_both;

        CacheStats {
            name: self.name.clone(),
            miss_rate,
            mpki,
            reuse,
            lifetime,
            efficiency,
            misses: self.misses,
            hits: self.hits,
        }
    }
}

#[derive(Debug, Default)]
pub struct Block<B: Default> {
    pub valid: bool,
    pub tag: usize,

    // Stats
    live_dur: u64,
    dead_dur: u64,
    alloc_count: u64,
    access_count: u64,

    // In Flight Stats
    alloc_time: u64,
    access_time: u64,

    // Replace Data
    pub repl_block: B,
}

impl<B: Default> Block<B> {
    pub fn apply(&mut self, addr: Addr) {
        self.valid = true;
        self.tag = addr.tag;
    }

    pub fn alloc(&mut self, cpu: &Cpu) {
        self.alloc_time = cpu.instr_idx;
        self.access_time = cpu.instr_idx;
        self.alloc_count += 1;
    }

    pub fn read(&mut self, cpu: &Cpu) {
        self.access_time = cpu.instr_idx;
        self.access_count += 1;
    }

    pub fn evict(&mut self, cpu: &Cpu) {
        self.live_dur += self.access_time - self.alloc_time;
        self.dead_dur += cpu.instr_idx - self.access_time;
    }
}
