use std::fs::File;
use std::io::{BufReader};
use std::time::Instant;
use std::thread;
use std::env;
use csv::{Writer, WriterBuilder};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle, HumanBytes, HumanDuration};
use regex::{Captures};

mod fast_logfile_iterator;
mod config;
mod inputs;
mod kvps;
mod output;
mod parsed_line;
mod regexes;
use crate::config::{Config};
use crate::inputs::{Inputs, InputFile, Column, is_date_column};
use crate::parsed_line::ParsedLine;

/* TODOs
=============================================================================
- Squash!
- If a column is not in KVPs, attempt to extract from the message.
- If column is a date/datetime, attempt to reformat the raw string.
- Sort (the contents of) the output files.
- Perf: Test inlining performance. 
- Perf: Test swapping the 'limit' checks.
- Perf: More parallelism while processing an individual file.
- Allow alternate names for columns (AppName, ApplicationName)
- Allow custom regex extractors for columns.
- Filter: from/to dates
- Filter: sysref
- Filter: column is non-blank, e.g. for call recorder execution time
- Filter: column matches a regex, ANY column matches a regex. DOES NOT MATCH, e.g. to get rid of heartbeats.
- Option: trim message to N chars.
- Option: quiet mode. No progress bars.
- Bug: we have some bad parsing in some files. It might just be because I have corrupted the file with an editor.
       Need to get the original files again.
*/

fn main() {
    let mut config = Config::default();

    // Temp code: allow a set of files on the command line to override the default.
    let args: Vec<_> = env::args().collect();
    if args.len() > 1 {
        config.input_file_specs.clear();
        config.input_file_specs.extend(args[1..].iter().map(|arg| arg.to_string()));
    }
    let inputs = Inputs::new_from_config(&config);

    if inputs.is_empty() {
        eprintln!("No input to process.");
        return;
    }

    let start_time = Instant::now();

    // TODO: What we would really like is to have N threads AT MOST processing at 
    // any one time. Say, N = 4, for example. Then we create new threads as existing
    // ones complete. Rayon would set N for us, but I can't get it to work with
    // the MultiProgress bar.
    let mp = MultiProgress::new();

    let longest_len = inputs.longest_input_name_len();
    for input_file in &inputs.input_files {
        let input_file = input_file.clone();
        let pb = make_progress_bar(longest_len, &mp, &input_file);
        let columns = inputs.columns.clone();
        let _ = thread::spawn(move || {
            process_log_file(pb, input_file, &columns);
        });
    }

    mp.join().unwrap();

    let total_bytes = inputs.input_files.iter().map(|f| f.length as u64).sum();
    println!("Processed {} in {} files in {}",
        HumanBytes(total_bytes),
        inputs.input_files.len(),
        HumanDuration(start_time.elapsed()));
}

fn make_progress_bar(longest_filename_length: usize, mp: &MultiProgress, input_file: &InputFile) -> ProgressBar {
    let pb = mp.add(ProgressBar::new(input_file.length as u64));
    pb.set_style(make_progress_bar_style(BarStyle::FileInProgress));

    let prefix = format!("{:>width$}: ", input_file.filename_only_as_string, width=longest_filename_length + 1);
    pb.set_prefix(&prefix);
    
    // Redraw every 1% of additional progress. Without this, redisplaying
    // the progress bar slows the program down a lot.
    pb.set_draw_delta(input_file.length as u64 / 100); 

    pb
}

enum BarStyle {
    FileInProgress,
    FileCompleted,
}

fn make_progress_bar_style(bar_style: BarStyle) -> ProgressStyle {
    ProgressStyle::default_bar().template(
        match bar_style {
            BarStyle::FileInProgress => "{prefix:.bold}▕{bar:50.white}▏{msg}  ({eta})",
            BarStyle::FileCompleted =>  "{prefix:.bold}▕{bar:50.green}▏{msg}",
        }
    ).progress_chars("█▉▊▋▌▍▎▏  ")
}

fn process_log_file(pb: ProgressBar, input_file: InputFile, columns: &[Column]) {
    let start_time = Instant::now();

    let input_file_handle = File::open(&input_file.path).expect("Could not open the input log file");
    let reader = BufReader::new(input_file_handle);

    let mut writer = WriterBuilder::new()
        .flexible(true)
        .from_path(&input_file.output_path)
        .expect(&format!("Could not open output file {}", &input_file.output_path));

    writer.write_record(columns.iter().map(|c| &c.name)).expect("Can write headings");

    let mut bytes_read_so_far = 0;
    for (bytes_read, log_line) in fast_logfile_iterator::FastLogFileIterator::new(reader) {
        process_line(columns, log_line, &mut writer);
        bytes_read_so_far += bytes_read;
        pb.inc(bytes_read);
        let msg = format!("{} / {}", HumanBytes(bytes_read_so_far), HumanBytes(input_file.length as u64));
        pb.set_message(&msg);
    }

    pb.set_style(make_progress_bar_style(BarStyle::FileCompleted));
    pb.finish_with_message(&format!("Done - {} in {}", HumanBytes(bytes_read_so_far), HumanDuration(start_time.elapsed())));
}

fn process_line(columns: &[Column], line: String,  writer: &mut Writer<File>) {
    let parsed_line = ParsedLine::new(&line);

    if parsed_line.is_err() {
        let data = vec![""];
        writer.write_record(&data).expect("Writing a CSV record should always succeed.");
        return;
    }

    let parsed_line = parsed_line.unwrap();
    let data = output::make_output_record(&parsed_line, columns);

    writer.write_record(&data).expect("Writing a CSV record should always succeed.");
}




fn extract_date(captures: Captures, capture_names: &[Option<&str>]) -> String {
    // Typical values for capture_names are:
    //      KVP regex : [None, None, None, None, None]
    //      Date regex: [None, Some("year"), Some("month"), Some("day"), Some("hour"), Some("minutes"), Some("seconds"), Some("fractions"), Some("year2"), Some("month2"), Some("day2")]

    // We consider the following combinations to be valid extractions.
    //      (year, month, day)
    //      (year, month, day, hour, minutes, seconds)
    //      (year, month, day, hour, minutes, seconds, fractions)
    // Anything else we consider to be a bad match.

    let year = extract_date_part("year", &captures, capture_names);
    if year.is_empty() { return "".to_string() };

    let month = extract_date_part("month", &captures, capture_names);
    if month.is_empty() { return "".to_string() };

    let day = extract_date_part("day", &captures, capture_names);
    if day.is_empty() { return "".to_string() };

    let hour = extract_date_part("hour", &captures, capture_names);
    if hour.is_empty() {
        return format!("{}-{}-{}", year, month, day);
    };

    let minutes = extract_date_part("minutes", &captures, capture_names);
    if minutes.is_empty() {
        return format!("{}-{}-{}", year, month, day);
    };

    let seconds = extract_date_part("seconds", &captures, capture_names);
    if seconds.is_empty() {
        return format!("{}-{}-{}", year, month, day);
    };

    let fractions = extract_date_part("fractions", &captures, capture_names);
    if fractions.is_empty() {
        format!("{}-{}-{} {}:{}:{}", year, month, day, hour, minutes, seconds)
    } else {
        format!("{}-{}-{} {}:{}:{}.{}", year, month, day, hour, minutes, seconds, fractions)
    }
}

fn extract_date_part<'t>(part: &str, captures: &'t Captures, capture_names: &[Option<&str>]) -> &'t str {
    for name in capture_names {
        if name.is_none() {
            continue;
        }

        let match_name = name.as_ref().unwrap();
        if match_name.starts_with(part) {
            let the_match = captures.name(match_name);
            match the_match {
                Some(m) => return m.as_str(),
                None => panic!("Because we have a match name, this should never be called.")
            }
        }
    }

    ""
}

fn extract_kvp<'t>(captures: Captures<'t>) -> &'t str {
    let first_valid_sub_match = captures.iter().skip(1).skip_while(|c| c.is_none()).nth(0).unwrap();
    match first_valid_sub_match {
        Some(m) => return m.as_str(),
        None => return ""
    }
}

// Cleanup the text. Doing this here keeps regexes simpler.
// The '.' deals with people using full-stops in log messages.
fn cleanup_slice(text: &str) -> &str {
    text.trim_matches(|c| c == '.' || char::is_whitespace(c))
}
