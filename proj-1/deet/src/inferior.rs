use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::process::Child;
use std::process::Command;
use std::os::unix::process::CommandExt;
use std::mem::size_of;

use crate::dwarf_data;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

pub struct Inferior {
    child: Child,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, break_points: &mut Vec<usize>) -> Option<Inferior> {
        let mut command = Command::new(target);
        command.args(args);
        unsafe {
            command.pre_exec(child_traceme);
        }
        let child = command.spawn().ok()?;
        let mut inferior = Inferior { child };
        for break_point in break_points.into_iter() {
            if let Err(e) = inferior.write_byte(*break_point, 0xcc) {
                println!("Error setting breakpoint: {}", e);
            }
        }
        break_points.clear();
        inferior.wait(None).ok()?;
        Some(inferior)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn continue_run(&mut self, break_points: &mut Vec<usize>) -> Result<Status, nix::Error> {
        for break_point in break_points.into_iter() {
            if let Err(e) = &self.write_byte(*break_point, 0xcc) {
                println!("Error setting breakpoint: {}", e);
            }
        }
        break_points.clear();
        ptrace::cont(self.pid(), None)?;
        self.wait(None)
    }

    pub fn kill(&mut self) {
        println!("Killing running inferior (pid {})", self.pid());
        if let Err(e) = Child::kill(&mut self.child) {
            println!("kill process error: {}", e);
        }
    }

    pub fn print_backtrace(&self, debug_data: &dwarf_data::DwarfData) -> Result<(), nix::Error> {
        if let Ok(reg) = ptrace::getregs(self.pid()) {
            let mut instruction_ptr = reg.rip as usize;
            let mut base_ptr = reg.rbp as usize;
            loop {
                let function_name = debug_data.get_function_from_addr(instruction_ptr).expect("wrong addr");
                let path_name = debug_data.get_line_from_addr(instruction_ptr).expect("wrong addr");
                println!("{} {}", function_name, path_name);
                if function_name == "main" { break; }
                instruction_ptr = ptrace::read(self.pid(), (base_ptr + 8) as ptrace::AddressType)? as usize;
                base_ptr = ptrace::read(self.pid(), base_ptr as ptrace::AddressType)? as usize;
            }
        }
        Ok(())
    }

    fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}
