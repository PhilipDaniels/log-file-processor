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
mod kvp;
mod output;
mod parsed_line;
mod profiles;
use crate::arguments::Arguments;
use crate::profiles::ProfileSet;
use crate::configuration::{get_config};
use crate::inputs::{Inputs, InputFile};
use crate::parsed_line::{ParsedLine, ParsedLineError, ParseLineResult};

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
    // 0.158    ...plus extract_log_date, alternatively...
    // 0.148    ...plus extract_log_date_fast
    // 0.166    ...plus extract leading KVPs
    // 0.276    ...plus extract trailing KVPs and message
    // 0.303    ...plus collect everything into one big vector of results
    // 0.444    ...plus sorting everything
    // 0.400    ...but sorting using Rayon's par_sort or par_sort_by_key is faster

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

    let mut all_lines_and_errors: Vec<_> =
        all_files.par_iter()
            .map(|(f, bytes)| {
                let lines = find_lines(bytes);
                println!("Found {} lines", lines.len());

                let parsing_results : Vec<_> = lines.par_iter()
                    .enumerate()
                    .map(|(line_num, &line)| {
                        let mut parsed_line_result = ParsedLine::new(line);

                        // Attach line number and original source.
                        match parsed_line_result {
                            Ok(ref mut pl) => {
                                pl.line_num = line_num;
                                pl.source = &f.filename_only_as_string
                            },
                            Err(ref mut e) => {
                                e.line_num = line_num;
                                e.source = &f.filename_only_as_string
                            },
                        };

                        parsed_line_result
                    })
                    .filter(filter_parsed_line)
                    .collect();

                parsing_results
            })
            .flatten()
            .collect();

    // Rust cannot always coerce a reference to an array such as:
    //      &b""
    // to a slice. We can force it to by [..]
    // This should put the errors at the front.
    all_lines_and_errors.par_sort_by_key(|r| {
        match r {
            Ok(ref v) => (v.log_date, v.source, v.line_num),
            Err(ref e) => (&b""[..], e.source, e.line_num)
        }
    });

    // Doing this increases the time from 0.3 seconds to 0.35 seconds!
    let total = all_lines_and_errors.len();
    let error_count = all_lines_and_errors.iter().filter(|r| r.is_err()).count();
    // let (mut successes, mut failures): (Vec<_>, Vec<_>) = all_lines_and_errors
    //      .into_iter()
    //      .partition_map(|r| {
    //          match r {
    //              Ok(v) => Either::Left(v),
    //              Err(v) => Either::Right(v),
    //          }
    //      });

    // successes.par_sort_by_key(|a| (a.log_date, a.source, a.line_num));
    // failures.par_sort_by_key(|a| (a.source, a.line_num));

    let elapsed = start_time.elapsed();
    println!("Processed {} in {} files in {}.{:03} seconds, ok lines = {}, error lines = {}",
         HumanBytes(total_bytes),
         input_count,
         elapsed.as_secs(),
         elapsed.subsec_millis(),
         total - error_count,
         error_count
         );

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