use std::process::{Command, Stdio};

use std::io::{BufRead, BufReader};

use clap::{Arg, App};

fn main() {
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

    let _config = matches.value_of("config"); // TODO read config

    let cmd = matches.values_of("cmd").map(|vals| vals.collect::<Vec<_>>()).unwrap();

    let child = Command::new(&cmd[0])
        .args(&cmd[1..])
        // do not redirect stdin, let cmd inherented from mlog
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start");

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
}
