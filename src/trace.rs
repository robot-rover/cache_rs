use std::{
    ffi, fs,
    io::{self, BorrowedBuf, ErrorKind, Read, Seek},
    mem::MaybeUninit,
    path::PathBuf,
    thread::{self, JoinHandle},
};

use crossbeam::channel::{Receiver, Sender};
use xz2::read::XzDecoder;

// Instruction Format
const NUM_INSTR_DESTINATIONS: usize = 2;
const NUM_INSTR_SOURCES: usize = 4;

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
pub struct Instr {
    pub ip: ffi::c_ulonglong,

    pub is_branch: ffi::c_uchar,
    pub branch_taken: ffi::c_uchar,

    pub destination_registers: [ffi::c_uchar; NUM_INSTR_DESTINATIONS],
    pub source_registers: [ffi::c_uchar; NUM_INSTR_SOURCES],

    pub destination_memory: [ffi::c_ulonglong; NUM_INSTR_DESTINATIONS],
    pub source_memory: [ffi::c_ulonglong; NUM_INSTR_SOURCES],
}

impl Instr {
    pub fn addresses<'a>(&'a self) -> impl Iterator<Item = usize> + 'a {
        std::iter::once(self.ip as usize)
            .chain(
                self.source_memory
                    .iter()
                    .map(|&addr| addr as usize)
                    .filter(|&addr| addr != 0),
            )
            .chain(
                self.destination_memory
                    .iter()
                    .map(|&addr| addr as usize)
                    .filter(|&addr| addr != 0),
            )
    }
}

pub struct Trace {
    pub rec: Receiver<Vec<Instr>>,
    _thread: JoinHandle<()>,
}

impl Trace {
    pub fn read(
        path: PathBuf,
        instr_per_block: usize,
        blocks_per_queue: usize,
    ) -> io::Result<Trace> {
        let stream = fs::File::open(path.clone())?;
        let (sender, receiver) = crossbeam::channel::bounded(blocks_per_queue);

        let t = thread::spawn(move || Trace::run_thread(stream, instr_per_block, sender));

        Ok(Trace {
            rec: receiver,
            _thread: t,
        })
    }

    fn run_thread(stream: fs::File, instr_per_block: usize, queue: Sender<Vec<Instr>>) {
        let mut xz_stream = XzDecoder::new(stream);
        const INSTR_SIZE: usize = std::mem::size_of::<Instr>();
        loop {
            loop {
                let mut buffer = Vec::<Instr>::with_capacity(instr_per_block);
                let num_bytes_written = {
                    let (head, byte_buffer, tail) = unsafe {
                        buffer
                            .spare_capacity_mut()
                            .align_to_mut::<MaybeUninit<u8>>()
                    };
                    assert_eq!(head.len(), 0);
                    assert_eq!(tail.len(), 0);
                    let mut borrowed: BorrowedBuf<'_> = byte_buffer.into();
                    match xz_stream.read_buf_exact(borrowed.unfilled()) {
                        Ok(()) => {}
                        Err(err) if err.kind() == ErrorKind::UnexpectedEof => {}
                        Err(err) => panic!("{}", err),
                    }
                    if borrowed.init_len() == 0 {
                        break;
                    }
                    assert_eq!(borrowed.init_len() % INSTR_SIZE, 0);
                    borrowed.init_len()
                };
                unsafe { buffer.set_len(num_bytes_written / INSTR_SIZE) };

                match queue.send(buffer) {
                    Ok(()) => {}
                    Err(_) => return,
                }
            }

            let mut stream = xz_stream.into_inner();
            stream.seek(io::SeekFrom::Start(0)).unwrap();
            xz_stream = XzDecoder::new(stream);
        }
    }
}
