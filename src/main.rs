use std::process::{Command, Stdio};

use std::thread;

use toml::Value;
use dirs;

use std::path::Path;
use std::fs::{File, OpenOptions};

use std::time::SystemTime;

use std::io;
use std::io::{BufReader, BufWriter};
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
    time: u64, // timestamp integer, 0 means no limit
    size: u64, // byte, 0 means no limit
}

impl FileConfig {
    fn new(value: &Value, ignore_mode: bool) -> Self {
        if value.get("target").is_none() && value["target"].as_str().unwrap() != "file" {
            panic!("no target found, should be `target=\"file\"`");
        }

        if value.get("file").is_none() || value["file"].get("name").is_none() {
            panic!("no file configure");
        }

        let name = String::from(value["file"]["name"].as_str().unwrap());
        let file = &value["file"];
        let smode = value.get("mode");

        let mode = if smode.is_none() || ignore_mode {
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
                let time = head.parse::<f64>().unwrap();
                (time * 3600.0) as u64
            } else if stime.chars().last().unwrap() == 'd' {
                let head = &stime[..stime.len() - 1];
                let time = head.parse::<f64>().unwrap();
                (time * 3600.0 * 24.0) as u64
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
                let size = head.parse::<f64>().unwrap();
                (size * 1024.0) as u64
            } else if ssize.chars().last().unwrap() == 'M' {
                let head = &ssize[..ssize.len() - 1];
                let size = head.parse::<f64>().unwrap();
                (size * 1024.0 * 1024.0) as u64
            } else if ssize.chars().last().unwrap() == 'G' {
                let head = &ssize[..ssize.len() - 1];
                let size = head.parse::<f64>().unwrap();
                (size * 1024.0 * 1024.0 * 1024.0) as u64
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
            Some(c) => Some(FileConfig::new(c, true)),
        };
        let stdout = match value.get("stdout") {
            None => None,
            Some(c) => Some(FileConfig::new(c, false)),
        };
        let stderr = match value.get("stderr") {
            None => None,
            Some(c) => Some(FileConfig::new(c, false)),
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

struct LogHandler<R: Read, W: Write> {
    config: Option<FileConfig>,
    input: BufReader<R>,
    output: BufWriter<W>,
}

impl<T: Read, W: Write> LogHandler<T, W> {
    fn new(
        config: Option<FileConfig>,
        input: T,
        output: W
    ) -> Result<Self, std::io::Error> {
        Ok(LogHandler {
            config: config,
            input: BufReader::new(input),
            output: BufWriter::new(output),
        })
    }

    fn process(&mut self) -> std::io::Result<()> {
        let mut buf = String::new();

        match self.config {
            None => {
                loop {
                    let len = self.input.read_line(&mut buf)?;
                    if len == 0 {
                        return Ok(());
                    }
                    write!(self.output, "{}", buf)?;
                    buf.clear();
                }
            },
            Some(ref config) => {
                let path = config.name.as_str();

                rotate_files(path, config.num)?;

                let mut file = OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(path)?;

                let mut c_time = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();

                let mut size = 0;

                loop {
                    let len = self.input.read_line(&mut buf)?;
                    if len == 0 {
                        return Ok(());
                    }

                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();

                    if config.time != 0 && now - c_time >= config.time {
                        rotate_files(path, config.num)?;

                        file = OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open(path)?;
                        c_time = now;
                        size = 0
                    }

                    if config.size != 0 && size >= config.size {
                        rotate_files(path, config.num)?;

                        file = OpenOptions::new()
                            .append(true)
                            .create(true)
                            .open(path)?;
                        c_time = now;
                        size = 0
                    }

                    match config.mode {
                        LogMode::TEE => write!(self.output, "{}", buf)?,
                        _ => {},
                    }

                    write!(file, "{}", buf)?;
                    size += len as u64;

                    buf.clear();
                }
            }
        }
    }
}

fn rotate_files(path: &str, num: u32) -> io::Result<()> {
    let mut i = num - 1;

    let mut from = String::new();
    let mut to = String::new();

    while i > 1 {
        from.clear();
        to.clear();

        from.push_str(path);
        from.push('.');
        from.push_str((i - 1).to_string().as_str());

        to.push_str(path);
        to.push('.');
        to.push_str(i.to_string().as_str());

        i -= 1;

        if !Path::new(from.as_str()).is_file() {
            continue;
        }

        // mv log.(i - 1) log.i
        std::fs::rename(from.as_str(), to.as_str())?;
    }

    from.clear();
    to.clear();

    from.push_str(path);

    to.push_str(path);
    to.push_str(".1");

    // mv log log.1
    if Path::new(from.as_str()).is_file() {
        std::fs::rename(from.as_str(), to.as_str())?;
    }

    Ok(())
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

    let mut out_handler = LogHandler::new(config.stdout, stdout, std::io::stdout())?;
    let mut err_handler = LogHandler::new(config.stderr, stderr, std::io::stderr())?;

    let out_thread = thread::spawn(move || {
        match out_handler.process() {
            Err(error) => {
                panic!("exception in out_handler {:?}", error)
            },
            _ => (),
        }
    });

    let err_thread = thread::spawn(move || {
        match err_handler.process() {
            Err(error) => {
                panic!("exception in err_handler {:?}", error)
            },
            _ => (),
        }
    });

    match out_thread.join() {
        Err(error) => {
            panic!("failed to join out_thread {:?}", error)
        },
        _ => (),
    }
    match err_thread.join() {
        Err(error) => {
            panic!("failed to join err_thread {:?}", error)
        },
        _ => (),
    }

    let exit_code = child.wait().expect("child was not running").code().unwrap();

    std::process::exit(exit_code);
}
