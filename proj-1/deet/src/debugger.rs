use crate::debugger_command::DebuggerCommand;
use crate::inferior::Inferior;
use crate::inferior::Status;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use crate::dwarf_data::{DwarfData, Error as DwarfError};

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    breakpoints: Vec<usize>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            breakpoints: Vec::new(),
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.kill();
                    }
                    if let Some(inferior) = Inferior::new(&self.target, &args) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        // You may use self.inferior.as_mut().unwrap() to get a mutable reference
                        // to the Inferior object
                        match self.inferior.as_mut().unwrap().continue_run() {
                            Ok(message) => {
                                match message {
                                    Status::Exited(num) => {
                                        println!("Child exited (status {})", num);
                                    },
                                    Status::Signaled(signal) => {
                                        println!("Child signaled (signal {})", signal);
                                    },
                                    Status::Stopped(signal, size) => {
                                        println!("Child stopped (signal {})", signal);
                                        println!("Stopped at {} {}", 
                                            self.debug_data.get_function_from_addr(size).expect("wrong addr"),
                                            self.debug_data.get_line_from_addr(size).expect("wrong addr")
                                        );
                                    }
                                }
                            }
                            Err(e) => { println!("{e}"); }
                        }
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Quit => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.kill();
                    }
                    return;
                }
                DebuggerCommand::Continue => {
                    match self.inferior.as_mut() {
                        Some(inferior) => {
                            match inferior.continue_run() {
                                Ok(message) => {
                                    if let Status::Exited(num) = message {
                                        println!("Continue: Child exited (status {})", num);
                                    }
                                }
                                Err(e) => { println!("{e}"); }
                            }
                        }
                        None => {
                            println!("No process running.");
                        }
                    }
                }
                DebuggerCommand::Backtrace => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.print_backtrace(&self.debug_data).ok();
                    }
                }
                DebuggerCommand::Break(addr) => {
                    if addr[0] != "*" {
                        println!("wrong address format");
                        continue;
                    }
                    let addr = parse_address(&addr[1..]);
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}

fn parse_address(addr: &str) -> Option<usize> {
    let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
        &addr[2..]
    } else {
        &addr
    };
    usize::from_str_radix(addr_without_0x, 16).ok()
}