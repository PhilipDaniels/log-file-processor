use std::fs::{File, read, write};
use std::time::Instant;
use std::io::{self, Write};
use csv::{Writer, WriterBuilder};
use indicatif::{HumanBytes};
use itertools::Itertools;
use structopt::StructOpt;
use rayon::prelude::*;
use rayon::iter::Either;

mod arguments;
mod byte_extensions;
mod configuration;
mod inputs;
mod kvps;
mod output;
mod parse_utils;
mod parsed_line;
mod parsed_line2;
mod profiles;
use crate::arguments::Arguments;
use crate::profiles::ProfileSet;
use crate::configuration::{get_config};
use crate::parsed_line::ParsedLine;
use crate::inputs::{Inputs, InputFile};
use crate::parsed_line2::{ParsedLine2, ParsedLineError, ParseLineResult};

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


    // Time to simply read and write the file
    // Threading        Ordering        Read Only   Read & Write
    // =========        ========        =========   ============
    // Single           Small to Large  0.140
    // Single           Large to small  0.140
    // Rayon            Small to large  0.066
    // Rayon            Large to small  0.066       0.143
    // Rayon            Unsorted        0.066

    // Time     What
    // ====     ====
    // 0.143    Raw read & write whole file
    // 0.143    ...plus find the line endings

    let start_time = Instant::now();
    let total_bytes = inputs.total_bytes() as u64;
    let input_count = inputs.len();

    // We need to get all the files into memory at the same time because we
    // want to collect a consolidated set of parsed line (over all the files).
    // The bytes of the files must therefore outlive all the parsed lines.
    let all_files: Vec<(&InputFile, Vec<u8>)> = inputs.files.par_iter()
        .map(|f| (f, read(&f.path).expect("Can read file")))
        .collect();

    // Process all files in parallel. Accumulate the lines written for each file so
    // that they can be merged and written to a single, sorted, consolidated file.
    let mut all_lines = vec![];
    all_files.par_iter()
        .map(|(f, bytes)| {
            let lines = find_lines(bytes);
            println!("Found {} lines", lines.len());

            // Now we have the lines, we can parse each one in parallel. Line numbers help with error messages.
            let (successfully_parsed_lines, errors): (Vec<_>, Vec<_>) =
                lines.par_iter().enumerate()
                    .map(|(line_num, &line)| parse_line(line_num, line))
                    .filter(filter_parsed_line)
                    .partition_map(|parsed_line_result| match parsed_line_result {
                        Ok(parsed_line) => Either::Left(parsed_line),
                        Err(err) => Either::Right(err)
                    });

            // Sort by log time.
            // Optionally write each file to its own output CSV file.
            //write(&f.output_path, &whole_file_bytes).unwrap();

            // Then send back the parsed lines to the outer loop.
            (*f, successfully_parsed_lines, errors)
        })
        .collect_into_vec(&mut all_lines);

    // TODO: Write all_lines to consolidated.csv.

    let elapsed = start_time.elapsed();
    println!("Processed {} in {} files in {}.{:03} seconds",
         HumanBytes(total_bytes),
         input_count,
         elapsed.as_secs(),
         elapsed.subsec_millis());

    Ok(())


    // let configuration = Arc::new(configuration);
    // let mut all_lines = vec![];
    // for input_file in inputs.files {
    //     let conf = Arc::clone(&configuration);
    //     let join_handle = thread::spawn(move || process_log_file(conf, input_file));
    //     let mut lines = join_handle.join().unwrap();
    //     all_lines.append(&mut lines);
    // }
}

/// Returns the Result<T,E> of parsing a line.
fn parse_line(line_num: usize, line: &[u8]) -> ParseLineResult {
    ParsedLine2::new(line_num, line)
}


/// Applies the appropriate filtering to parsed line results.
/// Errors are always passed through, but successfully parsed lines may have a filter applied,
/// for example to match a sysref or a date range.
fn filter_parsed_line(parsed_line: &ParseLineResult) -> bool {
    parsed_line.is_err() || true
}

/// Look for the \r\n line endings in the file and return a vector of
/// slices, each slice being one line in the log file. Be careful not to be confused
/// by any stray '\r's in the log file.
fn find_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut cr_indexes: Vec<_> = bytes.iter().positions(|&c| c == b'\r').collect();
    cr_indexes.retain(|&idx| idx == bytes.len() -1 || bytes[idx + 1] == b'\n');
    cr_indexes.insert(0, 0);
    let last_idx = cr_indexes[cr_indexes.len() - 1];
    if last_idx == bytes.len() - 2 || last_idx == bytes.len() - 1 {
        // The last idx is at the end of the file (accounting for "\r\n").
    } else {
        // It isn't. Be sure to include the trailing data in a slice.
        cr_indexes.push(bytes.len() - 1);
    }

    cr_indexes.windows(2)
        .map(|window| &bytes[window[0]..window[1]])
        .collect()
}


// fn process_log_file(config: Arc<Configuration>, input_file: InputFile) -> Vec<ParsedLine> {
//     let start_time = Instant::now();

//     let (parsed_lines, bytes_read) = get_parsed_lines(&config, &input_file);
//     write_to_file(&config, &input_file, &parsed_lines).expect("Writing to file should succeed");

//     parsed_lines
// }

// fn get_parsed_lines(config: &Configuration, input_file: &InputFile) -> (Vec<ParsedLine>, u64) {
//     let input_file_handle = File::open(&input_file.path).expect("Could not open the input log file");
//     let reader = BufReader::new(input_file_handle);

//     let mut lines = vec![];
//     let mut bytes_read_so_far = 0;
//     for (bytes_read, log_line) in FastLogFileIterator::new(reader) {
//         bytes_read_so_far += bytes_read;

//         match ParsedLine::new(&log_line)
//         {
//             Ok(pl) => if parsed_line_passes_filter(&pl, &config) { lines.push(pl) },
//             Err(e) => {},//eprintln!("Error parsing line {}", log_line), This messes up the progress bars.
//         };
//     }

//     (lines, bytes_read_so_far)
// }

// fn parsed_line_passes_filter(parsed_line: &ParsedLine, config: &Configuration) -> bool {
//     true
// }

// fn write_to_file(config: &Configuration, input_file: &InputFile, parsed_lines: &[ParsedLine]) -> io::Result<()> {
//     let mut writer = WriterBuilder::new()
//         .flexible(true)
//         .from_path(&input_file.output_path)
//         .expect("Cannot create CSV writer");

//     writer.write_record(config.columns.iter()).expect("Can write headings");

//     for parsed_line in parsed_lines {
//         let data = output::make_output_record(config, &parsed_line);
//         writer.write_record(&data)?;
//     }

//     Ok(())
// }


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