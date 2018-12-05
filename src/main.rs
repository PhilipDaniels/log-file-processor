use std::fs::File;
use std::io::{BufReader,BufWriter};
use std::time::Instant;
//use std::io::prelude::Write;
use std::thread;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle, HumanBytes, HumanDuration};

mod fast_logfile_iterator;
mod config;
mod inputs;

use crate::config::Config;
use crate::inputs::{Inputs, InputFile};

fn main() {
    let mut config = Config::default();
    let inputs = Inputs::new_from_config(&config);

    if inputs.is_empty() {
        println!("No input to process.");
        return;
    }

    let mp = MultiProgress::new();

    // TODO: Consider cloning input_file here.
    let longest_len = inputs.longest_filename_length();
    for input_file in inputs.input_files {
        let pb = make_progress_bar(longest_len, &mp, &input_file);

        let _ = thread::spawn(move || {
            process_log_file(pb, input_file);
        });
    }

    mp.join().unwrap();
}

fn process_log_file(pb: ProgressBar, input_file: InputFile) {
    let input_file_handle = File::open(&input_file.path).expect("Could not open the input log file");
    let output_file_handle = File::create(&input_file.output_path).expect(&format!("Could not open output file {}", &input_file.output_path));

    let reader = BufReader::new(input_file_handle);
    //let mut writer = BufWriter::new(output_file_handle);

    let start_time = Instant::now();

    let mut bytes_read_so_far = 0;
    for (bytes_read, log_line) in fast_logfile_iterator::FastLogFileIterator::new(reader) {
        bytes_read_so_far += bytes_read;
        pb.inc(bytes_read);
        let msg = format!("{} / {}", HumanBytes(bytes_read_so_far), HumanBytes(input_file.length as u64));
        pb.set_message(&msg);
        let wait = Duration::from_micros(100);
        thread::sleep(wait);
    }

    pb.set_style(make_progress_bar_style(false));
    pb.finish_with_message(&format!("Done - {} in {}", HumanBytes(bytes_read_so_far), HumanDuration(start_time.elapsed())));
}




fn make_progress_bar(longest_filename_length: usize, mp: &MultiProgress, input_file: &InputFile) -> ProgressBar {
    let pb = mp.add(ProgressBar::new(input_file.length as u64));
    pb.set_style(make_progress_bar_style(true));

    let prefix = format!("{:>width$}: ", input_file.filename_only_as_string, width=longest_filename_length + 1);
    pb.set_prefix(&prefix);
    
    // Redraw every 1% of additional progress. Without this, redisplaying
    // the progress bar slows the program down a lot.
    pb.set_draw_delta(input_file.length as u64 / 100); 

    pb
}

fn make_progress_bar_style(with_eta: bool) -> ProgressStyle {
    const TEMPLATE_WITH_ETA   : &str = "{prefix:.bold}▕{bar:50.cyan}▏{msg}  ({eta})";
    const TEMPLATE_WITHOUT_ETA: &str = "{prefix:.bold}▕{bar:50.cyan}▏{msg}";
    ProgressStyle::default_bar().template(if with_eta { TEMPLATE_WITH_ETA } else {TEMPLATE_WITHOUT_ETA }).progress_chars("█▉▊▋▌▍▎▏  ")
}