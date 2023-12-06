use std::collections::VecDeque;

use crate::{
    cache::{Addr, Cache, IsCache},
    cpu::Cpu,
};

use super::{AccessResult, MakeS, Replace};

pub struct Lru {}

impl Lru {
    pub fn new() -> Self {
        Lru {}
    }
}

impl Replace<LruSetData, ()> for Lru {
    fn access(cpu: &mut Cpu, cache: &mut Cache<LruSetData, (), Self>, addr: Addr) -> AccessResult {
        let set_range = cache.get_set(addr.set);
        // println!("Addr: {addr:#?}");
        let set_slice = &mut cache.blocks[set_range];
        let lru_queue = &mut cache.set_data[addr.set].ru_order;
        // let vacant_blocks = set_slice.iter().filter(|b| !b.valid).count();
        // First, look for a hit
        let hit = set_slice
            .iter_mut()
            .enumerate()
            .find(|(_way, b)| b.valid && b.tag == addr.tag);

        if let Some((hit_way, block)) = hit {
            lru_queue.remove(
                lru_queue
                    .iter()
                    .cloned()
                    .enumerate()
                    .find(|(_idx, way)| *way as usize == hit_way)
                    .unwrap()
                    .0,
            );
            lru_queue.push_front(hit_way as u16);
            block.read(cpu);
            AccessResult::Hit
        } else {
            // if vacant_blocks < cache.n_ways {
            //     println!("Vacant: {vacant_blocks}");
            // }
            // Its a miss, lets allocate space for it
            let (victim_way, victim) = if let Some(vacant_block) =
                set_slice.iter_mut().enumerate().find(|(_way, b)| !b.valid)
            {
                // Empty Block, YAY
                vacant_block
            } else {
                // No empty blocks, evict
                let lru_way = lru_queue.pop_back().unwrap() as usize;
                let victim_block = &mut set_slice[lru_way];
                victim_block.evict(cpu);
                (lru_way, victim_block)
            };
            lru_queue.push_front(victim_way as u16);
            victim.apply(addr);
            victim.alloc(cpu);

            AccessResult::Miss
        }
    }
}

#[derive(Debug, Default)]
pub struct LruSetData {
    ru_order: VecDeque<u16>,
}

impl MakeS for LruSetData {
    fn new(n_ways: usize) -> Self {
        LruSetData {
            ru_order: VecDeque::with_capacity(n_ways),
        }
    }
}
