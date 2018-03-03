extern crate rand;

use std::fs::File;
use std::io::BufWriter;
use std::io::prelude::Write;
use rand::Rng;

fn main() {
    let num_lines = std::env::args().nth(1).expect("Pass number of log lines as the only command line parameter.");
    let num_lines = num_lines.parse::<usize>().expect("Expect a number greater than 0 as the only command line parameter.");
    println!("Generating input.log with {} lines", num_lines);

    let f = File::create("input.log").expect("Could not create the input.log file");
    let mut writer = BufWriter::new(f);
    let mut rng = rand::thread_rng();

    // The files I have from work have a \n before Source, but the parser has been adjusted to
    // cope with or without such a character there.
    for i in 0..num_lines {
        write!(writer,
            "2018-01-23 09:12:32.{0:07} | MachineName=name.of.computer | AppName=Something.Host | pid={1} | \
tid={2} | {3} | {4} Source={5} {6} {7} {8} {9}\r\n",
            i,
            i % 20,
            100 + (i % 3),
            get_log_level(&mut rng),
            get_message(&mut rng),
            get_source(&mut rng),
            get_kvps(&mut rng),
            get_correlation_key(&mut rng),
            get_kvps(&mut rng),
            get_call_recorder_execution_time(&mut rng)
            ).unwrap();
    }
}

fn get_log_level(rnd: &mut rand::ThreadRng) -> &'static str {
    match rnd.gen_range(0, 5) {
        0 => "[INFO_]",
        1 => "[VRBSE]",
        2 => "[WARNG]",
        3 => "[ERROR]",
        4 => "[FATAL]",
        _ => panic!("This is to shut the compiler up, should never happen.")
    }
}

fn get_message(rnd: &mut rand::ThreadRng) -> &'static str {
    match rnd.gen_range(0, 5) {
        0 => "Some random message",
        1 => "Another message",
        2 => "A message of no importance",
        3 => "A really important message that you should pay attention to",
        4 => "A message you can ignore",
        _ => panic!("This is to shut the compiler up, should never happen.")
    }
}

fn get_source(rnd: &mut rand::ThreadRng) -> &'static str {
    match rnd.gen_range(0, 5) {
        0 => "CleverComponent",
        1 => "DumbComponent",
        2 => "FooController",
        3 => "SpecialManager",
        4 => "Blob",
        _ => panic!("This is to shut the compiler up, should never happen.")
    }
}

fn get_kvps(rnd: &mut rand::ThreadRng) -> &'static str {
    match rnd.gen_range(0, 5) {
        0 => "Foo=200",
        1 => "Bar=Whatever\nSomething=Else",
        2 => "Bar=Whatever\nSomething=Else Extra=Spicy",
        3 => "Inverse=True Ref=123",
        4 => "Inverse=False Ref=ABCDEF\nSomething=Nicaragua Extra=Sweet",
        _ => panic!("This is to shut the compiler up, should never happen.")
    }
}

fn get_correlation_key(rnd: &mut rand::ThreadRng) -> &'static str {
    match rnd.gen_range(0, 5) {
        0...2 => "CorrelationKey=6b9e8da8-8f84-46e1-9c7d-66f6ad1ad9d7",
        _ => ""
    }
}

fn get_call_recorder_execution_time(rnd: &mut rand::ThreadRng) -> String {
    let time = rnd.gen_range(12, 1250);
    match rnd.gen_range(0, 5) {
        0...2 => format!("CallRecorderExecutionTime={}", time),
        _ => "".to_string()
    }
}
