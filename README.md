# DBFS - the DeBugger From Scratch

DBFS is a very basic debugger written in Rust, it uses the `ptrace` syscall to monitor a child process.
It aims to implement debugger features like breakpoints, watchpoints, stepping from scratch without using a library for a pedagogical reason.
Other aspects like reading of ELF /  DWARF format or handling user input may use libraries.

DBFS works only on *nix systems and x86.

## Usage

### Launching

The executable takes the program to debug as an argument `dbfs <program_to_debug>`.

Example `dbfs ./a.out`

### Commands

#### Add a breakpoint

Once the debgger is launched, you can add a breakpoint using `breakpoint <breakpoint address>`.

Example `> breakpoint  0x555555555151`

#### Run the program

Run the program with the `run` command.

#### Get the registers state

If a breakpoint has been reached, you can get the general purpose registers with `info registers`.

#### Continue to next breakpoint

Once a breakpoint has been reached, you can use `continue` to resume execution of the program until the next breakpoint or until the program exits.

