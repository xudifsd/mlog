use std::process::{Command, Stdio};

use toml::Value;
use dirs;

use std::path::Path;
use std::fs::File;

use std::io;
use std::io::{BufRead, BufReader};
use std::io::prelude::*;

use clap::{Arg, App};

enum LogMode {
    TEE,
    REDIRECT,
}

struct FileConfig {
    mode: LogMode,
    num: i32, // default to 5
    time: u32, // timestamp integer, 0 means no limit
    size: u32, // byte, 0 means no limit
}

impl FileConfig {
    fn new() -> Self {
        FileConfig {mode: LogMode::REDIRECT, num: 5, time: 0, size: 0}
    }
}

struct LogConfig {
    mlog: FileConfig,
    stdout: FileConfig,
    stderr: FileConfig,
}

impl LogConfig {
    fn new() -> Self {
        LogConfig {mlog: FileConfig::new(), stdout: FileConfig::new(), stderr: FileConfig::new()}
    }

    fn parse(&mut self, config: Value) {
        println!("{:?}", config["stdout"]); // TODO
    }
}

fn get_config(config_path: Option<&str>) -> Result<LogConfig, std::io::Error> {
    let file_path = if config_path.is_some() {
        Path::new(config_path.unwrap()).to_path_buf()
    } else {
        let home = dirs::home_dir().expect("Could not find home dir");
        Path::new(&home).join(".mlog")
    };

    let mut f = File::open(file_path.to_str().unwrap()).expect("failed to open");

    let mut buffer = Vec::new();
    f.read_to_end(&mut buffer).expect("failed to read");
    let content = std::str::from_utf8(&buffer).unwrap();

    let value = content.parse::<Value>().unwrap();

    let mut result = LogConfig::new();
    result.parse(value);

    Ok(result)
}

fn main() -> io::Result<()> {
    let matches = App::new("Manage cmd logs for you")
                          .version("0.1")
                          .author("Di Xu <xudifsd@gmail.com>")
                          .arg(Arg::with_name("config")
                               .short("c")
                               .takes_value(true)
                               .help("config file path, default to ~/.mlog"))
                          .arg(Arg::with_name("cmd")
                               .required(true)
                               .multiple(true)
                               .last(true))
                          .get_matches();

    let config = get_config(matches.value_of("config"));

    let cmd = matches.values_of("cmd").map(|vals| vals.collect::<Vec<_>>()).unwrap();

    let child = Command::new(&cmd[0])
        .args(&cmd[1..])
        // do not redirect stdin, let cmd inherented from mlog
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.expect("Could not capture standard output.");
    let stderr = child.stderr.expect("Could not capture standard error.");

    let out_reader = BufReader::new(stdout);
    let err_reader = BufReader::new(stderr);

    out_reader.lines()
        .filter_map(|line| line.ok())
        .for_each(|line| println!("{}", line));

    err_reader.lines()
        .filter_map(|line| line.ok())
        .for_each(|line| eprintln!("err: {}", line));

    Ok(())
}
