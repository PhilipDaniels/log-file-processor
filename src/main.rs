use std::fs::File;
use std::io::{BufReader};
use std::time::Instant;
use std::thread;
use std::io::{self, Write};
use std::sync::{Arc};
use csv::{Writer, WriterBuilder};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle, HumanBytes, HumanDuration};
use structopt::StructOpt;

mod arguments;
mod fast_logfile_iterator;
mod configuration;
mod inputs;
mod kvps;
mod output;
mod parse_utils;
mod parsed_line;
mod profiles;
use crate::arguments::Arguments;
use crate::fast_logfile_iterator::FastLogFileIterator;
use crate::profiles::ProfileSet;
use crate::configuration::{get_config, Configuration};
use crate::parsed_line::ParsedLine;
use crate::inputs::{Inputs, InputFile};

/* TODOs
=============================================================================
[O] If a column is not in KVPs, attempt to extract from the message.
[ ] I had to change make_output_record return type from "&'p str" to String. Can it be a Cow instead?
    Problem is the CSV writer cannot handle it.
[ ] If column is a date/datetime, attempt to reformat the raw string.
    Use Chrono for DateTimes. https://rust-lang-nursery.github.io/rust-cookbook/datetime/parse.html#parse-string-into-datetime-struct
[ ] Sort (the contents of) the output files.
[ ] Perf: Test inlining performance.
[ ] Perf: More parallelism while processing an individual file.
[ ] Allow custom regex extractors for columns.
[ ] Filter: from/to dates
[ ] Filter: sysref
[ ] Filter: column is non-blank, e.g. for call recorder execution time
[ ] Filter: column matches a regex, ANY column matches a regex. DOES NOT MATCH, e.g. to get rid of heartbeats.
[ ] Bug: we have some bad parsing in some files. It might just be because I have corrupted the file with an editor.
       Need to get the original files again.
*/

fn main() -> Result<(), io::Error> {
    let args = Arguments::from_args();
    //println!("Args = {:#?}", args);

    if args.dump_config {
        let profiles = ProfileSet::default();
        let json = serde_json::to_string_pretty(&profiles)?;
        println!("{}", json);
        return Ok(());
    }

    let profiles = match dirs::home_dir() {
        Some(mut path) => {
            path.push(".lpf.json");
            match File::open(path) {
                Ok(f) => serde_json::from_reader(f)?,
                Err(ref e) if e.kind() == io::ErrorKind::NotFound => ProfileSet::default(),
                Err(e) => panic!("Error opening ~/.lpf.json: {:?}", e)
            }
        },
        None => {
            eprintln!("Cannot locate home directory, using default configuration.");
            ProfileSet::default()
        }
    };

    let configuration = get_config(&profiles, &args);
    let inputs = Inputs::new_from_config(&configuration);

    if inputs.is_empty() {
        eprintln!("No input to process.");
        return Ok(());
    }

    //println!("profiles = {:#?}", profiles);
    //println!("configuration = {:#?}", configuration);

    let start_time = Instant::now();

    // TODO: What we would really like is to have N threads AT MOST processing at
    // any one time. Say, N = 4, for example. Then we create new threads as existing
    // ones complete. Rayon would set N for us, but I can't get it to work with
    // the MultiProgress bar.
    let mp = MultiProgress::new();
    let longest_len = inputs.longest_input_name_len();
    let total_bytes = inputs.total_bytes() as u64;
    let input_count = inputs.len();

    let configuration = Arc::new(configuration);
    let mut all_lines = vec![];
    for input_file in inputs.files {
        let pb = make_progress_bar(longest_len, &mp, &input_file);
        let conf = Arc::clone(&configuration);
        let join_handle = thread::spawn(move || process_log_file(conf, input_file, pb));
        let mut lines = join_handle.join().unwrap();
        all_lines.append(&mut lines);
    }

    mp.join().unwrap();
    let elapsed = start_time.elapsed();
    println!("Processed {} in {} files in {}.{:03} seconds",
        HumanBytes(total_bytes), input_count,
        elapsed.as_secs(), elapsed.subsec_millis());
    Ok(())
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

fn process_log_file(config: Arc<Configuration>, input_file: InputFile, pb: ProgressBar) -> Vec<ParsedLine> {
    let start_time = Instant::now();

    let (parsed_lines, bytes_read) = get_parsed_lines(&config, &input_file, &pb);
    write_to_file(&config, &input_file, &parsed_lines).expect("Writing to file should succeed");

    pb.set_style(make_progress_bar_style(BarStyle::FileCompleted));
    pb.finish_with_message(&format!("Done - {} in {}", HumanBytes(bytes_read), HumanDuration(start_time.elapsed())));

    parsed_lines
}

fn get_parsed_lines(config: &Configuration, input_file: &InputFile, pb: &ProgressBar) -> (Vec<ParsedLine>, u64) {
    let input_file_handle = File::open(&input_file.path).expect("Could not open the input log file");
    let reader = BufReader::new(input_file_handle);

    let mut lines = vec![];
    let mut bytes_read_so_far = 0;
    for (bytes_read, log_line) in FastLogFileIterator::new(reader) {
        bytes_read_so_far += bytes_read;
        pb.inc(bytes_read);
        let msg = format!("{} / {}", HumanBytes(bytes_read_so_far), HumanBytes(input_file.length as u64));
        pb.set_message(&msg);

        match ParsedLine::new(&log_line)
        {
            Ok(pl) => if parsed_line_passes_filter(&pl, &config) { lines.push(pl) },
            Err(e) => {},//eprintln!("Error parsing line {}", log_line), This messes up the progress bars.
        };
    }

    (lines, bytes_read_so_far)
}

fn parsed_line_passes_filter(parsed_line: &ParsedLine, config: &Configuration) -> bool {
    true
}

fn write_to_file(config: &Configuration, input_file: &InputFile, parsed_lines: &[ParsedLine]) -> io::Result<()> {
    let mut writer = WriterBuilder::new()
        .flexible(true)
        .from_path(&input_file.output_path)
        .expect("Cannot create CSV writer");

    writer.write_record(config.columns.iter()).expect("Can write headings");

    for parsed_line in parsed_lines {
        let data = output::make_output_record(config, &parsed_line);
        //writer.write_record(&data)?;
    }

    Ok(())
}


// fn get_output_records(config: &Configuration, parsed_lines: &[ParsedLine]) -> Vec<String> {
//     let mut writer = WriterBuilder::new()
//         .flexible(true)
//         .from_writer(vec![]);

//     for parsed_line in parsed_lines {
//         // columns is a Vec<String>, one entry for each column.
//         let columns = output::make_output_record(config, &parsed_line);
//         writer.write_record(&columns).expect("Writing a CSV record should always succeed.");
//     }

// //    let data = output::make_output_record(config, &parsed_line);
//   //  writer.write_record(&data).expect("Writing a CSV record should always succeed.");

//     ()
// }

/*
fn process_log_file(config: Arc<Configuration>, input_file: InputFile, pb: ProgressBar) -> Vec<u8> {
    let start_time = Instant::now();

    let input_file_handle = File::open(&input_file.path).expect("Could not open the input log file");
    let reader = BufReader::new(input_file_handle);

    // TODO: get parsed lines as a vector
    // let parsed_lines = get_all_as_vec();
    // let filtered_lines: vec = parsed_lines.iter().filter(|line| line.logdate > ...).collect();
    // filtered_lines.sort();
    // write out filtered_lines


    // Write everything to an in-memory vector first.


    let mut bytes_read_so_far = 0;
    for (bytes_read, log_line) in FastLogFileIterator::new(reader) {
        process_line(&config, log_line, &mut writer);
        bytes_read_so_far += bytes_read;
        pb.inc(bytes_read);
        let msg = format!("{} / {}", HumanBytes(bytes_read_so_far), HumanBytes(input_file.length as u64));
        pb.set_message(&msg);
    }

    // Now write everything out to file in one go.
    let mut output_file_handle = File::create(&input_file.output_path)
        .expect(&format!("Could not open output file {}", &input_file.output_path));
    let vec = writer.into_inner().unwrap();
    output_file_handle.write(&vec)
        .expect(&format!("Could not write to output file {}", &input_file.output_path));

    pb.set_style(make_progress_bar_style(BarStyle::FileCompleted));
    pb.finish_with_message(&format!("Done - {} in {}", HumanBytes(bytes_read_so_far), HumanDuration(start_time.elapsed())));

    vec
}

fn process_line(config: &Configuration, line: String,  writer: &mut Writer<Vec<u8>>) {
    let parsed_line = ParsedLine::new(&line);

    if parsed_line.is_err() {
        let data = vec![""];
        writer.write_record(&data).expect("Writing a CSV record should always succeed.");
        return;
    }

    let parsed_line = parsed_line.unwrap();
    let data = output::make_output_record(config, &parsed_line);
    writer.write_record(&data).expect("Writing a CSV record should always succeed.");
}
*/