use std::{
    cmp::{self, Ordering},
    collections::VecDeque,
    ops::Range,
};

use crate::{
    cache::{Addr, Cache, IsCache},
    cpu::Cpu,
};

use super::{AccessResult, MakeS, Replace};

fn dual_slice_mut<'a, T>(
    data: &'a mut [T],
    first: Range<usize>,
    second: Range<usize>,
) -> (&'a mut [T], &'a mut [T]) {
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
        Lrudb {
            pred_table: vec![0; TABLE_SIZE],
        }
    }
}

fn move_to_front(queue: &mut VecDeque<u16>, val: usize) {
    let val = val as u16;
    queue.remove(
        queue
            .iter()
            .cloned()
            .enumerate()
            .find(|(_idx, way)| *way == val)
            .unwrap()
            .0,
    );
    queue.push_front(val);
}

fn use_lru(queue: &mut VecDeque<u16>) -> usize {
    let lru = queue.pop_back().unwrap();
    queue.push_front(lru);
    lru as usize
}

impl Replace<LrudbSetData, LrudbBlockData> for Lrudb {
    fn access(
        cpu: &mut Cpu,
        cache: &mut Cache<LrudbSetData, LrudbBlockData, Self>,
        addr: Addr,
    ) -> AccessResult {
        let set_range = cache.get_set(addr.set);
        let adjacent_set = addr.set ^ (1 << 3);
        let adj_set_range = cache.get_set(adjacent_set);
        let (set_slice, adj_set_slice) =
            dual_slice_mut(&mut cache.blocks, set_range, adj_set_range);
        let [lru, adj_lru] = &mut cache
            .set_data
            .get_many_mut([addr.set, adjacent_set])
            .unwrap();
        let main_queue = &mut lru.ru_order;
        let adj_queue = &mut adj_lru.ru_order;

        // First, look for a main set hit
        let hit = set_slice
            .iter_mut()
            .enumerate()
            .find(|(_way, b)| b.valid && !b.repl_block.receiver && b.tag == addr.tag)
            .map(|(way, _b)| way);

        // Next, check for an adjacent set hit
        let hit = hit.or_else(|| {
            adj_set_slice
                .iter_mut()
                .enumerate()
                .find(|(_way, b)| b.valid && b.repl_block.receiver && b.tag == addr.tag)
                .map(|(adj_way, adj_block)| {
                    // Receiver block in adj set matches
                    let main_victim_way = *main_queue.back().unwrap() as usize;
                    let main_victim = &mut set_slice[main_victim_way];
                    move_to_front(adj_queue, adj_way);
                    std::mem::swap(main_victim, adj_block);
                    std::mem::swap(&mut main_victim.block_stats, &mut adj_block.block_stats);

                    main_victim.repl_block.receiver = false;
                    adj_block.repl_block.receiver = !adj_block.repl_block.receiver;
                    main_victim.valid;
                    main_victim_way
                })
        });

        // Try for a main set hit
        if let Some(hit_way) = hit {
            move_to_front(main_queue, hit_way);
            let block = &mut set_slice[hit_way];
            block.read(cpu);
            block.repl_block.access_block(&mut cache.repl.pred_table);
            block
                .repl_block
                .update_trace(cpu.ip as usize, &mut cache.repl.pred_table);
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
                let lru_way = main_queue.pop_back().unwrap() as usize;
                let main_victim_block = &mut set_slice[lru_way];

                let adj_victim = if let Some(adj) = adj_set_slice
                    .iter_mut()
                    .enumerate()
                    .find(|(_way, b)| !b.valid)
                {
                    adj_queue.push_front(adj.0 as u16);
                    adj.1
                } else if let Some((adj_way, adj_block)) = adj_set_slice
                    .iter_mut()
                    .enumerate()
                    .find(|(_way, b)| b.repl_block.dead)
                {
                    move_to_front(adj_queue, adj_way);
                    adj_block.evict(cpu);
                    adj_block
                        .repl_block
                        .replace_block(&mut cache.repl.pred_table);
                    adj_block
                } else {
                    let adj_lru_way = use_lru(adj_queue);
                    if adj_lru_way > adj_set_slice.len() {
                        println!("{}", adj_lru_way);
                    }
                    let adj_block = &mut adj_set_slice[adj_lru_way];
                    adj_block.evict(cpu);
                    adj_block
                        .repl_block
                        .replace_block(&mut cache.repl.pred_table);
                    adj_block
                };
                let main_stats = main_victim_block.block_stats;
                let adj_stats = adj_victim.block_stats;
                *adj_victim = std::mem::take(main_victim_block);
                adj_victim.valid = true;
                adj_victim.repl_block.receiver = true;
                adj_victim.block_stats = adj_stats;
                main_victim_block.block_stats = main_stats;

                (lru_way, main_victim_block)
            };
            main_queue.push_front(victim_way as u16);
            victim.apply(addr);
            victim.repl_block.receiver = false;
            victim.alloc(cpu);
            victim
                .repl_block
                .update_trace(cpu.ip as usize, &mut cache.repl.pred_table);

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
        LrudbSetData {
            ru_order: VecDeque::with_capacity(n_ways),
        }
    }
}

#[derive(Debug)]
pub struct LrudbBlockData {
    trace: BlockTrace,
    dead: bool,
    receiver: bool,
}

impl LrudbBlockData {
    fn update_trace(&mut self, pc: usize, pred_table: &mut [PredCounter]) {
        const LOW_15_MASK: u32 = (1 << 15) - 1;

        let to_add = (LOW_15_MASK & pc as u32) ^ (LOW_15_MASK & (pc as u32 >> 15));
        self.trace = (self.trace + to_add) & LOW_15_MASK;
        self.dead = pred_table[self.trace as usize] < 0;
    }

    fn access_block(&mut self, pred_table: &mut [PredCounter]) {
        pred_table[self.trace as usize] = cmp::min(3, pred_table[self.trace as usize] + 1);
    }

    fn replace_block(&mut self, pred_table: &mut [PredCounter]) {
        pred_table[self.trace as usize] = cmp::max(-4, pred_table[self.trace as usize] - 1);
        self.trace = 0;
    }
}

impl Default for LrudbBlockData {
    fn default() -> Self {
        Self {
            trace: 0,
            dead: false,
            receiver: false,
        }
    }
}
