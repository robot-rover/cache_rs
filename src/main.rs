#![feature(read_buf)]
#![feature(get_many_mut)]

mod cache;
mod config;
mod cpu;
mod replace;
mod trace;

use std::fs;

use cache::IsCache;
use cpu::Cpu;
use trace::{Instr, Trace};

use crate::config::Config;

fn main() {
    let mut args = pico_args::Arguments::from_env();
    let n_warm: u64 = args
        .opt_value_from_str("-w")
        .expect("-w should be an integer")
        .unwrap_or(50_000_000);
    let n_instr: u64 = args
        .opt_value_from_str("-i")
        .expect("-i should be an integer")
        .unwrap_or(100_000_000);
    let heartbeat_int: u64 = args
        .opt_value_from_str("-h")
        .expect("-h should be an integer")
        .unwrap_or(0);

    let config_str: String = if let Some(config_str) = args.opt_value_from_str("--config").unwrap()
    {
        config_str
    } else {
        let config_path: String = args
            .opt_value_from_str("-p")
            .unwrap()
            .expect("Must provide a config with --config <json> or -p <path>");
        fs::read_to_string(config_path).expect("Could not find config file")
    };
    let config: Config = serde_json::from_str(&config_str).unwrap();
    let mut caches = config.to_caches();
    let mut cpu = Cpu::new();

    let stats_path: String = args
        .opt_value_from_str("--json")
        .unwrap()
        .expect("Must provide output path with --json");
    let mut next_heartbeat = heartbeat_int;

    let trace_path: String = args
        .opt_value_from_str("-t")
        .unwrap()
        .expect("Must provide a trace with -t");
    let inst_per_block: usize = args
        .opt_value_from_str("--buffer-size")
        .expect("--buffer-size must be an integer")
        .unwrap_or(1024 * 16);
    let blocks_per_queue: usize = args
        .opt_value_from_str("--queue-size")
        .expect("--queue-size must be an integer")
        .unwrap_or(32);

    let trace = Trace::read(trace_path.into(), inst_per_block, blocks_per_queue).unwrap();

    let mut warmup = n_warm > 0;
    let mut goal = if warmup { n_warm } else { n_instr };

    loop {
        let instr_block = trace.rec.recv().unwrap();
        operate(&mut cpu, &mut caches, &instr_block);
        if heartbeat_int != 0 && cpu.instr_idx > next_heartbeat {
            println!("Instr: {}", cpu.instr_idx);
            while next_heartbeat < cpu.instr_idx {
                next_heartbeat += heartbeat_int;
            }
        }

        if cpu.instr_idx > goal {
            if warmup {
                caches.iter_mut().for_each(|c| c.clear_stats());
                goal = cpu.instr_idx + n_instr;
                warmup = false;
                println!("Finished Warmup!")
            } else {
                break;
            }
        }
    }
    println!("Ran {} instructions", cpu.instr_idx);

    let stats = caches
        .iter()
        .map(|c| c.make_stats(&cpu))
        .collect::<Vec<_>>();

    let stats_file = fs::File::create(stats_path).expect("Cannot open output file");
    serde_json::to_writer_pretty(stats_file, &stats).unwrap();
}

fn operate(cpu: &mut Cpu, caches: &mut Vec<Box<dyn IsCache>>, instrs: &Vec<Instr>) {
    for instr in instrs {
        cpu.ip = instr.ip;
        for addr in instr.addresses() {
            for cache in caches.iter_mut() {
                match cache.access(cpu, cache.split_addr(addr)) {
                    replace::AccessResult::Hit => {
                        cache.hit();
                        break;
                    }
                    replace::AccessResult::Miss => cache.miss(),
                }
            }
        }
        cpu.instr_idx += 1;
    }
}
