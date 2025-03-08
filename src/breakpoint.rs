use nix::{
    sys::{ptrace, wait::waitpid},
    unistd::Pid,
};

use crate::utils::{read_data_fixed, write_data};

/// A representation of a software breakpoint on i386/x86_64
pub struct Breakpoint {
    pub thread: Pid,
    pub addr: usize,
    saved_data: [u8; 1],
}

impl Breakpoint {
    /// Creates a Software breakpoint in the thread pid
    ///
    /// This writes the breakpoint to the thread's memory
    pub fn create(addr: usize, thread: Pid) -> Option<Self> {
        let mut breakpoint = Self {
            thread,
            addr,
            saved_data: [0],
        };
        breakpoint.write();

        Some(breakpoint)
    }

    /// Writes the breakpoint to thread
    ///
    /// The original data at the breakpoin's location is saved, then the breakpoint is writter.
    /// The breakpoint is a trap instruction (int3 = 0xcc)
    pub fn write(self: &mut Self) -> Option<()> {
        self.saved_data = read_data_fixed(self.thread, self.addr)?;
        write_data(self.thread, self.addr, &[0xcc]).ok()
    }

    /// Restores the original data in the thread
    ///
    /// This write the original program data in place of the breakpoint
    pub fn restore_data(self: &Self) -> Option<()> {
        write_data(self.thread, self.addr, &self.saved_data).ok()
    }

    /// Restores the thread's instruction pointer to the breakpoint location
    ///
    /// This write the rip register so that the next instruction executed
    /// is the one located at the breakpoint
    pub fn restore_rip(self: &Self) -> Option<()> {
        let mut regs = ptrace::getregs(self.thread).ok()?;
        regs.rip = self.addr as _;
        ptrace::setregs(self.thread, regs).ok()
    }

    /// Continue running the program after the breakpoint has been hit and restored.
    ///
    /// To continue running the program, it is stepped by one instruction then the trap is rewritten
    ///
    pub fn run(self: &mut Self) -> Option<()> {
        ptrace::step(self.thread, None).ok()?;
        waitpid(self.thread, None).ok()?;
        self.write()
    }
}
