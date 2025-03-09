mod breakpoint;
mod utils;

use std::{
    env::{self, Args},
    ffi::CString,
    io::{Write, stdin, stdout},
    process::exit,
};

use breakpoint::Breakpoint;
use nix::{
    errno::Errno,
    sys::{
        personality::{self, Persona},
        ptrace::{self},
        signal::{Signal, raise},
        wait::{WaitStatus, waitpid},
    },
    unistd::{ForkResult, Pid, execvp, fork},
};

/// Launches the tracee `program` and returns its Pid.
/// ASLR is disabled for the tracee and the traces asks to be traced.
/// For the tracer, this function guarantees that execve has already been called in the tracee.
fn launch_program(program: &str) -> Result<Pid, Errno> {
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child, .. }) => {
            waitpid(child, None).unwrap();
            ptrace::setoptions(child, ptrace::Options::PTRACE_O_TRACEEXEC).unwrap();
            ptrace::cont(child, None).unwrap();
            waitpid(child, None).unwrap();
            Ok(child)
        }
        Ok(ForkResult::Child) => {
            ptrace::traceme().unwrap();
            personality::set(Persona::ADDR_NO_RANDOMIZE).unwrap();
            raise(Signal::SIGSTOP).unwrap();
            execvp(&CString::new(program).unwrap(), &[] as &[CString])?;
            exit(1); // Unreachable
        }
        Err(errno) => Err(errno),
    }
}

enum BreakpointArg {
    Address(usize),
    LineNumber(String, usize),
    Symbol(String),
}

impl BreakpointArg {
    fn parse(arg: &str) -> Option<BreakpointArg> {
        if arg.starts_with("0x") {
            let addr = arg.trim_start_matches("0x");
            if let Ok(addr) = usize::from_str_radix(addr, 16) {
                return Some(BreakpointArg::Address(addr));
            }
        }
        todo!()
    }

    fn to_address(self: &Self) -> usize {
        match self {
            BreakpointArg::Address(addr) => *addr,
            _ => todo!(),
        }
    }
}

fn wait_and_check(
    waitstatus: &WaitStatus,
    child: &mut Option<Pid>,
    breakpoints: &mut Vec<Breakpoint>,
    hit_breakpoint_index: &mut Option<usize>,
) {
    let pid = child.unwrap();
    match waitstatus {
        nix::sys::wait::WaitStatus::Exited(_, exitcode) => {
            println!("Program exited with exit code {exitcode}");
            *child = None;
            breakpoints.clear();
        }
        nix::sys::wait::WaitStatus::Stopped(_, signal) => {
            if *signal == Signal::SIGTRAP {
                breakpoints.iter().for_each(|bp| bp.restore_data().unwrap());
                let regs = ptrace::getregs(pid).unwrap();
                if let Some(index) = breakpoints
                    .iter()
                    .position(|bp| bp.addr == (regs.rip - 1) as _)
                {
                    // We've hit the breakpoint at index
                    println!(
                        "Reached breakpoint {} at {:#x}",
                        index + 1,
                        breakpoints[index].addr
                    );
                    breakpoints.get_mut(index).unwrap().restore_rip().unwrap();
                    *hit_breakpoint_index = Some(index);
                    return;
                }
                println!("Program interrupted at {:#x}", regs.rip);
                return;
            }
            println!("Program stopped : {waitstatus:#?}");
        }
        nix::sys::wait::WaitStatus::StillAlive => {
            panic!("Program never stopped")
        }
        other => {
            println!("Program stopped : {other:#?}");
        }
    }
}

fn prompt_force_close(pid: Pid) {
    let mut buf = String::new();
    loop {
        println!(
            "\nProcess {pid} is still running, are you sure you want to quit ?\nThis will kill process {pid}\n\nQuit ? (y/n)"
        );
        stdin().read_line(&mut buf).unwrap();
        match buf.as_str().trim() {
            "y" => {
                ptrace::kill(pid).unwrap();
                exit(0);
            }
            "n" => {
                return;
            }
            _ => {
                buf.clear();
            }
        }
    }
}

fn main_loop(mut args: Args) {
    let program = args.next().unwrap();

    println!("Debugging {program}");

    let mut breakpoints = Vec::new();
    let mut breakpoints_args = Vec::new();
    let mut child = None;
    let mut hit_breakpoint_index = None;

    loop {
        print!("> ");
        stdout().flush().unwrap();
        let mut buffer = String::new();
        stdin().read_line(&mut buffer).unwrap();
        let mut words = buffer.split_whitespace();

        let command = words.next();

        let command = match command {
            Some(command) => command,
            None => {
                match child {
                    Some(pid) => {
                        prompt_force_close(pid);
                        continue;
                    }
                    None => exit(0),
                };
            }
        };

        match command {
            "breakpoint" => {
                let arg = words.next();
                if let None = arg {
                    println!("Usage: breakpoint <arg>");
                    continue;
                }
                let arg = arg.expect("never fails");
                if let Some(arg) = BreakpointArg::parse(arg) {
                    breakpoints_args.push(arg);
                    println!("Breakpoint {} added", breakpoints_args.len());
                } else {
                    println!("Invalid breakpoint '{arg}'");
                }
            }
            "run" => {
                if child.is_some() {
                    println!("Program already running");
                    continue;
                }
                match launch_program(&program) {
                    Ok(pid) => {
                        breakpoints = breakpoints_args
                            .iter()
                            .map(|el| {
                                let breakpoint = Breakpoint::create(el.to_address(), pid).unwrap();
                                breakpoint
                            })
                            .collect();
                        child = Some(pid);
                        ptrace::cont(pid, None).unwrap();
                        let waitstatus = waitpid(pid, None).unwrap();
                        wait_and_check(
                            &waitstatus,
                            &mut child,
                            &mut breakpoints,
                            &mut hit_breakpoint_index,
                        );
                    }
                    Err(errno) => println!("Error launching '{program}' : {}", errno.desc()),
                }
            }

            "continue" => match child {
                Some(pid) => {
                    if let Some(index) = hit_breakpoint_index {
                        breakpoints.iter_mut().enumerate().for_each(|(i, bp)| {
                            if i != index {
                                bp.write().unwrap()
                            }
                        });
                        breakpoints.get_mut(index).unwrap().run().unwrap();
                        hit_breakpoint_index = None
                    } else {
                        breakpoints.iter_mut().for_each(|bp| bp.write().unwrap());
                    }
                    ptrace::cont(pid, None).unwrap();
                    let waitstatus = waitpid(pid, None).unwrap();
                    wait_and_check(
                        &waitstatus,
                        &mut child,
                        &mut breakpoints,
                        &mut hit_breakpoint_index,
                    );
                }
                None => {
                    println!("No program running");
                }
            },
            "info" => {
                let arg = words.next();
                if let None = arg {
                    println!("Usage: breakpoint <arg>");
                    continue;
                }
                let arg = arg.expect("never fails");
                match arg {
                    "registers" => match child {
                        Some(pid) => {
                            let regs = ptrace::getregs(pid).unwrap();
                            println!("{:#x?}", regs);
                        }
                        None => {
                            println!("No program running");
                        }
                    },
                    other => {
                        println!("No info for '{other}'");
                    }
                }
            }
            "stepi" => match child {
                Some(pid) => {
                    let waitstatus;
                    if let Some(index) = hit_breakpoint_index {
                        breakpoints.iter_mut().enumerate().for_each(|(i, bp)| {
                            if i != index {
                                bp.write().unwrap()
                            }
                        });
                        waitstatus = breakpoints.get_mut(index).unwrap().run().unwrap();
                        hit_breakpoint_index = None
                    } else {
                        breakpoints.iter_mut().for_each(|bp| bp.write().unwrap());
                        ptrace::step(pid, None).unwrap();
                        waitstatus = waitpid(pid, None).unwrap();
                    }
                    wait_and_check(
                        &waitstatus,
                        &mut child,
                        &mut breakpoints,
                        &mut hit_breakpoint_index,
                    );
                }
                None => {
                    println!("No program running");
                }
            },
            other => {
                println!("Unknown command '{other}'");
            }
        }
    }
}

fn main() {
    let mut args = env::args();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} <program to trace> [<args>...]",
            args.next().unwrap()
        );
        return;
    }

    args.next().unwrap();
    main_loop(args);
}
