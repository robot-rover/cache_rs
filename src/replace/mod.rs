pub mod lru;
pub mod lrudb;
pub mod nmru;

use crate::{
    cache::{Addr, Cache},
    cpu::Cpu,
};

pub trait MakeS {
    fn new(n_ways: usize) -> Self;
}

pub trait Replace<S: MakeS, B: Default>: Sized {
    fn access(cpu: &mut Cpu, cache: &mut Cache<S, B, Self>, addr: Addr) -> AccessResult;
}

pub enum AccessResult {
    Hit,
    Miss,
}
