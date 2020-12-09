/*
 * Copyright (C) 2020  Koki Fukuda
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

extern crate notify;
extern crate structopt;

use notify::{DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::process::{exit, Command};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use structopt::StructOpt;

#[derive(Debug, Clone)]
enum ProceedIf {
    Any,
    Success,
    Failure,
}

#[derive(Debug)]
enum CommandParseError {
    EmptyCommand,
}

#[derive(Debug, Clone)]
struct SimpleCommand {
    command: String,
    args: Vec<String>,
    proceed_if: ProceedIf,
}

impl SimpleCommand {
    fn new(
        command_line: &[String],
        proceed_if: ProceedIf,
    ) -> Result<SimpleCommand, CommandParseError> {
        if command_line.is_empty() {
            return Err(CommandParseError::EmptyCommand);
        }

        let command = String::from(&command_line[0]);
        let args = command_line[1..command_line.len()].to_vec();

        Ok(SimpleCommand {
            command,
            args,
            proceed_if,
        })
    }

    fn set_filename(&mut self, path: &str) {
        for i in 0..self.args.len() {
            self.args[i] = self.args[i].replace("{}", path);
        }
    }

    fn execute(&self) -> bool {
        match Command::new(&self.command).args(&self.args).status() {
            Ok(status) => match self.proceed_if {
                ProceedIf::Any => true,
                ProceedIf::Success => status.success(),
                ProceedIf::Failure => !status.success(),
            },
            Err(why) => {
                eprintln!("Error: Failed to execute command: {}", why);
                false
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RebuildConfig {
    commands: Vec<SimpleCommand>,
    verbatim: bool,
}

impl RebuildConfig {
    fn new(cmdline: Vec<String>, verbatim: bool) -> Result<RebuildConfig, CommandParseError> {
        let mut commands = Vec::<SimpleCommand>::new();

        let mut single_command = Vec::<String>::new();
        for arg in cmdline.iter() {
            if arg == ";" {
                match SimpleCommand::new(&single_command, ProceedIf::Any) {
                    Ok(cmd) => commands.push(cmd),
                    Err(e) => return Err(e),
                }
                single_command.truncate(0);
            } else if arg == "&&" {
                match SimpleCommand::new(&single_command, ProceedIf::Success) {
                    Ok(cmd) => commands.push(cmd),
                    Err(e) => return Err(e),
                }
                single_command.truncate(0);
            } else if arg == "||" {
                match SimpleCommand::new(&single_command, ProceedIf::Failure) {
                    Ok(cmd) => commands.push(cmd),
                    Err(e) => return Err(e),
                }
                single_command.truncate(0);
            } else {
                single_command.push(arg.into())
            }
        }
        if !single_command.is_empty() {
            match SimpleCommand::new(&single_command, ProceedIf::Any) {
                Ok(cmd) => commands.push(cmd),
                Err(e) => return Err(e),
            }
        }

        Ok(RebuildConfig { commands, verbatim })
    }

    fn set_filename(&self, path: PathBuf) -> RebuildConfig {
        let mut out = self.clone();

        if !self.verbatim {
            let path = path.as_os_str().to_string_lossy().into_owned();
            for i in 0..out.commands.len() {
                out.commands[i].set_filename(&path);
            }
        }

        out
    }
}

fn rebuild_sync(config: RebuildConfig) {
    for cmd in config.commands.iter() {
        if !cmd.execute() {
            break;
        }
    }
}

enum ThreadHandleMessage {
    Handle(JoinHandle<()>),
    Finish,
}

fn do_rebuild(
    config: RebuildConfig,
    run_async: bool,
    thread_handle_sender: &Sender<ThreadHandleMessage>,
) {
    if run_async {
        thread_handle_sender
            .send(ThreadHandleMessage::Handle(thread::spawn(move || {
                rebuild_sync(config)
            })))
            .unwrap();
    } else {
        rebuild_sync(config);
    }
}

fn prepare_manager_thread(receiver: Receiver<ThreadHandleMessage>) -> JoinHandle<()> {
    thread::spawn(move || loop {
        match receiver.recv() {
            Ok(ThreadHandleMessage::Handle(handle)) => handle.join().unwrap(),
            Ok(ThreadHandleMessage::Finish) => break,
            _ => (),
        }
    })
}

#[derive(StructOpt, Debug)]
#[structopt(about = "Run command automatically when specified file is updated.")]
struct Opt {
    #[structopt(long = "verbatim", help = "Don't replace '{}' with changed filename")]
    verbatim: bool,
    #[structopt(
        long = "do-while",
        help = "Executes command once before start watching"
    )]
    init: bool,
    #[structopt(long = "async", help = "Runs command asynchronously")]
    run_async: bool,
    #[structopt(name = "filename", help = "Filename to watch", required = true)]
    filename: String,
    #[structopt(
        name = "command",
        help = "Command to execute when the file updated",
        min_values = 1,
        required = true
    )]
    command: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    let rebuild_config = match RebuildConfig::new(opt.command, opt.verbatim) {
        Ok(config) => config,
        Err(_) => {
            eprintln!("Syntax error: empty command isn't allowed");
            exit(1);
        }
    };

    let (thread_tx, thread_rx) = channel();
    let manager_join_handle = prepare_manager_thread(thread_rx);

    if opt.init {
        let path = PathBuf::from(&opt.filename);
        do_rebuild(rebuild_config.set_filename(path), opt.run_async, &thread_tx);
    }

    let (tx, rx) = channel();

    let mut watcher = match RecommendedWatcher::new(tx, Duration::from_millis(500)) {
        Ok(watcher) => watcher,
        Err(why) => {
            eprintln!("Error: Failed to initialize watcher: {}", why);
            exit(1);
        }
    };

    if let Err(why) = watcher.watch(opt.filename, RecursiveMode::NonRecursive) {
        eprintln!("Error: Failed to establish watch: {}", why);
        exit(1);
    };

    loop {
        match rx.recv() {
            Ok(DebouncedEvent::Write(path)) => {
                do_rebuild(rebuild_config.set_filename(path), opt.run_async, &thread_tx);
            }
            Ok(DebouncedEvent::Remove(_)) => {
                println!("Error: Target file removed; exiting...");
                break;
            }
            Ok(_) => continue,
            Err(why) => eprintln!("Warning: Error watcing filesystem: {}", why),
        }
    }

    if opt.run_async {
        println!("Waiting for commands to finish...");
    }
    thread_tx.send(ThreadHandleMessage::Finish).unwrap();
    manager_join_handle.join().unwrap();
}
