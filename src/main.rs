use std::path::PathBuf;
use std::fs;
use std::fs::File;
use std::io::{BufReader,BufWriter};
use std::io::prelude::Write;
use std::thread;
use std::time::Duration;
use glob::glob;
use rayon::prelude::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

mod fast_logfile_iterator;

struct Config {
    input_file_specs: Vec<String>,
    input_files: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_file_specs: vec!["*.log".to_string()],
            input_files: vec![]
        }
    }
}

fn main() {
    let mut config = Config::default();

    // Determine available input files.
    for path in &config.input_file_specs {
        for entry in glob(&path).expect("Failed to read glob pattern.") {
            match entry {
                Ok(path) => if !config.input_files.contains(&path) {
                    config.input_files.push(path)
                },
                Err(e) => {}
            }
        }
    }
    
    if config.input_files.is_empty() {
        println!("Did not detect any input files.");
    }

    let mp = MultiProgress::new();

    // Long calculation is running. Bars are created but do not paint.
    let total_bytes: u64 = config.input_files.par_iter()
        .map(|f| process_log_file(&mp, f))
        .sum();
    
    // This appears first.
    println!("Rayon iteration complete");

    // Then the bars appear (and update themselves 0..100%) when we get to here.
    // But by this point we are done!
    mp.join().unwrap();
}

fn process_log_file(mp: &MultiProgress, path: &PathBuf) -> u64 {
    let path_string = path.to_str().expect("Path should be a valid UTF-8 string").to_owned();
    let filename_only_string = path.file_name().expect("Path should have a filename component").to_str().unwrap().to_owned();

    // Create a progress bar for this file corresponding to the number of bytes in the file.
    let input_len = fs::metadata(&path).expect("Can get file meta data").len();
    let pb = mp.add(ProgressBar::new(input_len));
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!("{{prefix:.bold}}▕{{bar:.{}}}▏{{msg}}", "yellow"))
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_prefix(&filename_only_string);
    
    // Redraw every 1% of additional progress. Without this, redisplaying
    // the progress bar slows the program down a lot.
    pb.set_draw_delta(input_len / 10000); 
    
    let in_file = File::open(&path).expect("Could not open the input log file");
    let out_path = format!("{}.out", path_string);
    let out_file = File::create(&out_path).expect(&format!("Could not open output file {}", &out_path));

    let reader = BufReader::new(in_file);
    //let mut writer = BufWriter::new(out_file);

    let mut total_bytes_read = 0;
    for (bytes_read, log_line) in fast_logfile_iterator::FastLogFileIterator::new(reader) {
        total_bytes_read += bytes_read;
        pb.inc(bytes_read);
        //let wait = Duration::from_millis(1);
        //thread::sleep(wait);
    }

    pb.finish();

    total_bytes_read
}
