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
use std::process::{exit, Command};
use std::sync::mpsc::channel;
use std::time::Duration;
use structopt::StructOpt;

#[derive(Debug, Clone)]
struct RebuildConfig {
    command: String,
    args: Vec<String>,
}

fn do_rebuild(config: RebuildConfig) {
    if let Err(why) = Command::new(config.command).args(config.args).spawn() {
        println!("Error: Failed to spawn command: {}", why);
    };
}

#[derive(StructOpt, Debug)]
#[structopt(about = "Run command automatically when specified file is updated.")]
struct Opt {
    #[structopt(name = "filename", help = "Filename to watch", required = true)]
    filename: String,
    #[structopt(
        name = "command",
        help = "Command to execute when the file updated",
        min_values = 1,
        required = true,
    )]
    command: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    let rebuild_config = RebuildConfig {
        command: String::from(&opt.command[0]),
        args: opt.command.into_iter().skip(1).collect::<Vec<_>>(),
    };

    let (tx, rx) = channel();

    let mut watcher = match RecommendedWatcher::new(tx, Duration::from_secs(2)) {
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
            Ok(DebouncedEvent::Write(_)) => do_rebuild(rebuild_config.clone()),
            Ok(DebouncedEvent::Remove(_)) => {
                println!("Error: Target file removed; exiting...");
                exit(1);
            }
            Ok(_) => continue,
            Err(why) => eprintln!("Warning: Error watcing filesystem: {}", why),
        }
    }
}
