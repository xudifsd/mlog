use std::process::{Command, Stdio, ChildStdout, ChildStderr};

use std::thread;

use toml::Value;
use dirs;

use std::path::Path;
use std::fs::File;

use std::io;
use std::io::{BufRead, BufReader};
use std::io::prelude::*;

use clap::{Arg, App};

#[derive(Debug)]
enum LogMode {
    TEE,
    REDIRECT,
}

#[derive(Debug)]
struct FileConfig {
    mode: LogMode,
    name: String,
    num: u32, // default to 5, must larger than 0
    time: u32, // timestamp integer, 0 means no limit
    size: u32, // byte, 0 means no limit
}

impl FileConfig {
    fn new(value: &Value) -> Self {
        if value.get("target").is_none() && value["target"].as_str().unwrap() != "file" {
            panic!("no target found, should be `target=\"file\"`");
        }

        if value.get("file").is_none() || value["file"].get("name").is_none() {
            panic!("no file configure");
        }

        let name = String::from(value["file"]["name"].as_str().unwrap());
        let file = &value["file"];
        let smode = value.get("mode");

        let mode = if smode.is_none() {
            LogMode::REDIRECT
        } else if smode.unwrap().as_str().unwrap() == "redirect" {
            LogMode::REDIRECT
        } else if smode.unwrap().as_str().unwrap() == "tee" {
            LogMode::TEE
        } else {
            panic!("unknown mode {}", smode.unwrap());
        };

        let snum = file.get("num");
        let num = if snum.is_none() {
            5
        } else {
            let snum = snum.unwrap().as_integer().unwrap();
            if snum < 0 {
                panic!("file.num can not be less than 0");
            }
            snum as u32
        };

        let stime = file.get("time");
        let time = if stime.is_none() {
            0
        } else {
            let stime = stime.unwrap().as_str().unwrap();
            if stime.len() == 0 {
                panic!("unknown file.time");
            } else if stime.chars().last().unwrap() == 'h' {
                let head = &stime[..stime.len() - 1];
                let time = head.parse::<f32>().unwrap();
                (time * 3600.0) as u32
            } else if stime.chars().last().unwrap() == 'd' {
                let head = &stime[..stime.len() - 1];
                let time = head.parse::<f32>().unwrap();
                (time * 3600.0 * 24.0) as u32
            } else {
                panic!("unknown file.time {}", stime);
            }
        };

        let ssize = file.get("size");
        let size = if ssize.is_none() {
            0
        } else {
            let ssize = ssize.unwrap().as_str().unwrap();
            if ssize.len() == 0 {
                panic!("unknown file.size");
            } else if ssize.chars().last().unwrap() == 'K' {
                let head = &ssize[..ssize.len() - 1];
                let size = head.parse::<i32>().unwrap();
                (size * 1024) as u32
            } else if ssize.chars().last().unwrap() == 'M' {
                let head = &ssize[..ssize.len() - 1];
                let size = head.parse::<i32>().unwrap();
                (size * 1024 * 1024) as u32
            } else if ssize.chars().last().unwrap() == 'G' {
                let head = &ssize[..ssize.len() - 1];
                let size = head.parse::<i32>().unwrap();
                (size * 1024 * 1024 * 1024) as u32
            } else {
                panic!("unknown file.size {}", ssize);
            }
        };

        FileConfig {mode: mode, name: name, num: num, time: time, size: size}
    }
}

#[derive(Debug)]
struct LogConfig {
    mlog: Option<FileConfig>,
    stdout: Option<FileConfig>,
    stderr: Option<FileConfig>,
}

impl LogConfig {
    fn new(value: &Value) -> Self {
        let mlog = match value.get("mlog") {
            None => None,
            Some(c) => Some(FileConfig::new(c)),
        };
        let stdout = match value.get("stdout") {
            None => None,
            Some(c) => Some(FileConfig::new(c)),
        };
        let stderr = match value.get("stderr") {
            None => None,
            Some(c) => Some(FileConfig::new(c)),
        };

        LogConfig { mlog: mlog, stdout: stdout, stderr: stderr}
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

    Ok(LogConfig::new(&value))
}

struct LogHandler<T: Read> {
    config: Option<FileConfig>,
    input: BufReader<T>,
    // output: File, // TODO, implement log rotation
}

impl<T: Read> LogHandler<T> {
    fn new(config: Option<FileConfig>, input: T) -> Result<Self, std::io::Error> {
        Ok(LogHandler {config: config, input: BufReader::new(input)})
    }
}

impl LogHandler<ChildStdout> {
    fn process(&mut self) -> std::io::Result<()> {
        let mut buf = String::new();

        loop {
            let len = self.input.read_line(&mut buf)?;
            if len == 0 {
                return Ok(());
            }
            eprint!("{}", buf);
            buf.clear();
        }
    }
}

impl LogHandler<ChildStderr> {
    fn process(&mut self) -> std::io::Result<()> {
        let mut buf = String::new();

        loop {
            let len = self.input.read_line(&mut buf)?;
            if len == 0 {
                return Ok(());
            }
            eprint!("{}", buf);
            buf.clear();
        }
    }
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

    let config = get_config(matches.value_of("config"))?;

    let cmd = matches.values_of("cmd").map(|vals| vals.collect::<Vec<_>>()).unwrap();

    let mut child = Command::new(&cmd[0])
        .args(&cmd[1..])
        // do not redirect stdin, let cmd inherented from mlog
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("Could not capture standard output.");
    let stderr = child.stderr.take().expect("Could not capture standard error.");

    let mut out_handler = LogHandler::new(config.stdout, stdout)?;
    let mut err_handler = LogHandler::new(config.stderr, stderr)?;

    let out_thread = thread::spawn(move || {
        out_handler.process();
    });
    let err_thread = thread::spawn(move || {
        err_handler.process();
    });

    out_thread.join();
    err_thread.join();

    let exit_code = child.wait().expect("child was not running").code().unwrap();

    std::process::exit(exit_code);

    Ok(())
}
