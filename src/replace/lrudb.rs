use std::{collections::VecDeque, cmp::{self, Ordering}, ops::Range};

use crate::{
    cache::{Addr, Cache, IsCache},
    cpu::Cpu,
};

use super::{AccessResult, Replace, MakeS};

fn dual_slice_mut<'a, T>(data: &'a mut [T], first: Range<usize>, second: Range<usize>) -> (&'a mut [T], &'a mut [T]) {
    let (left, right, swap) = match Ord::cmp(&first.start, &second.start) {
        Ordering::Less => (first, second, false),
        Ordering::Greater => (second, first, true),
        Ordering::Equal => panic!(),
    };

    assert!(left.end <= right.start);
    let (left_at_end, right_at_start) = data.split_at_mut(right.start);
    let left_sl = &mut left_at_end[left];
    let right_sl = &mut right_at_start[..(right.end - right.start)];
    if swap {
        (right_sl, left_sl)
    } else {
        (left_sl, right_sl)
    }
}

type BlockTrace = u32;
type PredCounter = i8;
const TABLE_SIZE: usize = 1 << 15; // 32K Table


pub struct Lrudb {
    pred_table: Vec<PredCounter>,
}

impl Lrudb {
    pub fn new() -> Self {
        Lrudb {pred_table: vec![0; TABLE_SIZE] }
    }
}

fn move_to_front(queue: &mut VecDeque<u16>, val: u16) {
    queue.remove(queue.iter().cloned().enumerate().find(|(_idx, way)| *way == val).unwrap().0);
    queue.push_front(val);
}

impl Replace<LrudbSetData, LrudbBlockData> for Lrudb {
    fn access(cpu: &mut Cpu, cache: &mut Cache<LrudbSetData, LrudbBlockData, Self>, addr: Addr) -> AccessResult {
        let set_range = cache.get_set(addr.set);
        let adjacent_set = addr.set ^ (1<<3);
        let adj_set_range = cache.get_set(adjacent_set);
        let (set_slice, adj_set_slice) = dual_slice_mut(&mut cache.blocks, set_range, adj_set_range);
        // println!("Addr: {addr:#?}");
        let [lru, adj_lru] = &mut cache.set_data.get_many_mut([addr.set, adjacent_set]).unwrap();
        let lru_queue = &mut lru.ru_order;
        let adj_lru_queue = &mut adj_lru.ru_order;

        // let vacant_blocks = set_slice.iter().filter(|b| !b.valid).count();
        // First, look for a hit
        let hit = set_slice
            .iter_mut()
            .enumerate()
            .find(|(_way, b)| b.valid && !b.repl_block.receiver && b.tag == addr.tag);

        let hit = if let Some(hit) = hit {
            Some(hit)
        } else {
            // Next, check the adjacent set
            let adj_hit = adj_set_slice.iter_mut().enumerate()
                .find(|(_way, b)| b.valid && b.repl_block.receiver && b.tag == addr.tag);
            if let Some((adj_way, adj_block)) = adj_hit {
                let main_victim_way = *lru_queue.back().unwrap() as usize;
                let main_victim = &mut set_slice[main_victim_way];
                move_to_front(adj_lru_queue, adj_way as u16);
                std::mem::swap(&mut main_victim.tag, &mut adj_block.tag);
                std::mem::swap(&mut main_victim.repl_block.trace, &mut adj_block.repl_block.trace);
                Some((main_victim_way, main_victim))
            } else {
                None
            }
        };

        // Try for a main set hit
        if let Some((hit_way, block)) = hit {
            move_to_front(lru_queue, hit_way as u16);
            block.read(cpu);
            block.repl_block.access_block(&mut cache.repl.pred_table);
            block.repl_block.update_trace(cpu.ip as usize, &mut cache.repl.pred_table);
            AccessResult::Hit
        } else {

            // Its a miss, lets allocate space for it
            let (victim_way, victim) = if let Some(vacant_block) =
                set_slice.iter_mut().enumerate().find(|(_way, b)| !b.valid)
            {
                // Empty Block, YAY
                vacant_block
            } else {
                // No empty blocks, evict
                let lru_way = lru_queue.pop_back().unwrap() as usize;
                let main_victim_block = &mut set_slice[lru_way];

                let (adj_victim_way, adj_victim) = if let Some(adj) = adj_set_slice.iter_mut().enumerate().find(|(_way, b)| !b.valid) {
                    adj
                } else if let Some((adj_way, adj_block)) = adj_set_slice.iter_mut().enumerate().find(|(_way, b)| b.repl_block.dead) {
                    adj_block.evict(cpu);
                    adj_block.repl_block.replace_block(&mut cache.repl.pred_table);
                    (adj_way, adj_block)
                } else {
                    let adj_lru_way = adj_lru_queue.pop_back().unwrap() as usize;
                    let adj_block = &mut adj_set_slice[adj_lru_way];
                    adj_block.evict(cpu);
                    adj_block.repl_block.replace_block(&mut cache.repl.pred_table);
                    (adj_lru_way, adj_block)
                };
                adj_lru_queue.push_front(adj_victim_way as u16);
                adj_victim.valid = true;
                adj_victim.tag = main_victim_block.tag;
                adj_victim.repl_block.receiver = true;

                (lru_way, main_victim_block)
            };
            lru_queue.push_front(victim_way as u16);
            victim.apply(addr);
            victim.repl_block.receiver = false;
            victim.alloc(cpu);
            victim.repl_block.update_trace(cpu.ip as usize, &mut cache.repl.pred_table);

            AccessResult::Miss
        }
    }
}

#[derive(Debug, Default)]
pub struct LrudbSetData {
    ru_order: VecDeque<u16>,
}

impl MakeS for LrudbSetData {
    fn new(n_ways: usize) -> Self {
        LrudbSetData { ru_order: VecDeque::with_capacity(n_ways) }
    }
}

pub struct LrudbBlockData {
    trace: BlockTrace,
    dead: bool,
    receiver: bool
}

impl LrudbBlockData {
    fn update_trace(&mut self, pc: usize, pred_table: &mut [PredCounter]) {
        const LOW_15_MASK: u32 = (1 << 15) - 1;

        let to_add = (LOW_15_MASK & pc as u32) ^ (LOW_15_MASK & (pc as u32 >> 15));
        self.trace = (self.trace + to_add) & LOW_15_MASK;
        self.dead = pred_table[self.trace as usize] >= 0;
    }

    fn access_block(&mut self, pred_table: &mut [PredCounter]) {
        pred_table[self.trace as usize] = cmp::min(1, pred_table[self.trace as usize] + 1);
    }

    fn replace_block(&mut self, pred_table: &mut [PredCounter]) {
        pred_table[self.trace as usize] = cmp::max(-2, pred_table[self.trace as usize] - 1);
        self.trace = 0;
    }
}

impl Default for LrudbBlockData {
    fn default() -> Self {
        Self { trace: 0, dead: false, receiver: false }
    }
}