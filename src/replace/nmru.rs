use crate::{
    cache::{Addr, Cache, IsCache},
    cpu::Cpu,
};

use super::{AccessResult, Replace, MakeS};

pub struct Nmru {
    rng: fastrand::Rng,
}

impl Nmru {
    pub fn new() -> Self {
        Nmru {
            rng: fastrand::Rng::new(),
        }
    }
}

impl Replace<NmruSetData, ()> for Nmru {
    fn access(cpu: &mut Cpu, cache: &mut Cache<NmruSetData, (), Self>, addr: Addr) -> AccessResult {
        let set_range = cache.get_set(addr.set);
        // println!("Addr: {addr:#?}");
        let set_slice = &mut cache.blocks[set_range];
        // let vacant_blocks = set_slice.iter().filter(|b| !b.valid).count();
        // First, look for a hit
        let hit = set_slice
            .iter_mut()
            .enumerate()
            .find(|(_way, b)| b.valid && b.tag == addr.tag);

        if let Some((way, block)) = hit {
            cache.set_data[addr.set].mru_way = way;
            block.read(cpu);
            AccessResult::Hit
        } else {
            // if vacant_blocks < cache.n_ways {
            //     println!("Vacant: {vacant_blocks}");
            // }
            // Its a miss, lets allocate space for it
            let (way, victim) = if let Some(vacant_block) =
                set_slice.iter_mut().enumerate().find(|(_way, b)| !b.valid)
            {
                // Empty Block, YAY
                vacant_block
            } else {
                // No empty blocks, evict
                let mru_way = cache.set_data[addr.set].mru_way;
                let mut victim_way = cache.repl.rng.usize(0..(cache.n_ways - 1));
                if victim_way >= mru_way {
                    victim_way += 1;
                }
                let victim_block = &mut set_slice[victim_way];
                victim_block.evict(cpu);
                (victim_way, victim_block)
            };
            cache.set_data[addr.set].mru_way = way;
            victim.apply(addr);
            victim.alloc(cpu);

            AccessResult::Miss
        }
    }
}

#[derive(Debug)]
pub struct NmruSetData {
    mru_way: usize,
}

impl MakeS for NmruSetData {
    fn new(_n_ways: usize) -> Self {
        NmruSetData { mru_way: 0 }
    }
}
