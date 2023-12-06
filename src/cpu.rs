#[derive(Debug)]
pub struct Cpu {
    pub ip: u64,
    pub instr_idx: u64,
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            ip: 0,
            instr_idx: 0,
        }
    }
}
