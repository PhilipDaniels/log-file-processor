use csv::WriterBuilder;
use indicatif::HumanBytes;
use itertools::Itertools;
use rayon::prelude::*;
use std::fs::{self, File};
use std::io;
use std::time::Instant;
use structopt::StructOpt;

mod arguments;
mod byte_extensions;
mod configuration;
mod inputs;
mod kvp;
mod output;
mod parsed_line;
mod profiles;
use crate::arguments::Arguments;
use crate::configuration::{get_config, Configuration};
use crate::inputs::{InputFile, Inputs};
use crate::parsed_line::{ParseLineResult, ParsedLine};
use crate::profiles::ProfileSet;

/* TODOs
=============================================================================
[ ] Auto-open the consolidated.csv.
[ ] Excel has trouble with the LogDate string.

[ ] Perf: Figure out how to do profiling.
[ ] Perf: Is it faster to write everything to RAM first? We could parallelize that.
[ ] Tools: figure out how to run rustfmt in VS Code.
[ ] Tools: figure out how to run clippy in VS Code

[ ] Allow custom regex extractors for columns.
[ ] Filter: from/to dates
    I have added the raw strings to the Arguments.
    I now need to add to the profile.
    Then do the From for Configuration
    Then do the get_config method
    Finally apply the date filter to the parsed line.
[ ] Filter: column is non-blank, e.g. for call recorder execution time
[ ] Filter: column matches a regex, ANY column matches a regex. DOES NOT MATCH, e.g. to get rid of heartbeats.

[ ] Rewrite using nom!
[ ] Write some macros to help with the ugliness of the tests
[ ] Get a better assertions library

Some sysrefs seen:
    Q2952601
    Q2952601
    Q2967281
    Q2952601
    Q2967281
    Q2975135
    Q2967281
    Q2970508
    Q2967281
*/

fn main() -> Result<(), io::Error> {
    let args = Arguments::from_args();
    println!("Args = {:#?}", args);
    //std::process::exit(0);

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
                Err(e) => panic!("Error opening ~/.lpf.json: {:?}", e),
            }
        }
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
    // 0.985    ...writing main fields with single-threaded '\r' checking (direct to file)
    // 1.506    ...writing main fields and kvps with single-threaded '\r' checking (direct to file)
    // 1.231    ...writing main fields and kvps with multi-threaded '\r' checking using Cow's (direct to file)
    // inlining all the ByteSliceExtensions makes no difference to the speed
    // 1.419    ...as above plus using alternate names for KVPs
    // 2.506    ...include entire line in the message

    let start_time = Instant::now();
    let total_bytes = inputs.total_bytes() as u64;
    let input_count = inputs.len();

    // We need to get all the files into memory at the same time because we
    // want to collect a consolidated set of parsed line (over all the files).
    // The bytes of the files must therefore outlive all the parsed lines.
    let all_files: Vec<(&InputFile, Vec<u8>)> = inputs
        .files
        .par_iter()
        .map(|f| (f, fs::read(&f.path).expect("Can read file")))
        .collect();

    // Process all files in parallel. Accumulate the lines written for each file so
    // that they can be merged and written to a single, sorted, consolidated file.

    let mut all_lines_and_errors: Vec<_> = all_files
        .par_iter()
        .map(|(f, bytes)| {
            let lines = find_lines(bytes);
            println!("Found {} lines", lines.len());

            let parsing_results: Vec<_> = lines
                .par_iter()
                .enumerate()
                .map(|(line_num, &line)| {
                    let mut parsed_line_result = ParsedLine::parse(line);

                    // Attach line number and original source.
                    match parsed_line_result {
                        Ok(ref mut pl) => {
                            pl.line_num = line_num;
                            pl.source = &f.filename_only_as_string
                        }
                        Err(ref mut e) => {
                            e.line_num = line_num;
                            e.source = &f.filename_only_as_string
                        }
                    };

                    parsed_line_result
                })
                .filter(|parsed_line_result| should_output_line(&configuration, parsed_line_result))
                .collect();

            parsing_results
        })
        .flatten()
        .collect();

    // Rust cannot always coerce a reference to an array such as:
    //      &b""
    // to a slice. We can force it to by [..]
    // This should put the errors at the front.
    all_lines_and_errors.par_sort_by_key(|r| match r {
        Ok(ref v) => (v.log_date, v.source, v.line_num),
        Err(ref e) => (&b""[..], e.source, e.line_num),
    });

    let total = all_lines_and_errors.len();
    let error_count = write_output_files(&configuration, &all_lines_and_errors)?;

    let elapsed = start_time.elapsed();
    println!(
        "Processed {} in {} files in {}.{:03} seconds, ok lines = {}, error lines = {}",
        HumanBytes(total_bytes),
        input_count,
        elapsed.as_secs(),
        elapsed.subsec_millis(),
        total - error_count,
        error_count
    );

    Ok(())
}

/// Applies the appropriate filtering to parsed line results.
/// Errors are always passed through, but successfully parsed lines may have a filter applied,
/// for example to match a sysref or a date range.
fn should_output_line(config: &Configuration, parsed_line_result: &ParseLineResult) -> bool {
    match parsed_line_result {
        Err(_) => true,
        Ok(line) => match (config.sysrefs.is_empty(), line.kvps.get_value(b"sysref")) {
            (true, _) => true,
            (false, Some(sr_from_line)) => config.sysrefs.iter().any(|sr| sr == &sr_from_line.as_ref()),
            (false, None) => false
        }
    }
}

/// Look for the \r\n line endings in the file and return a vector of
/// slices, each slice being one line in the log file. Be careful not to be confused
/// by any stray '\r's in the log file.
fn find_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut cr_indexes: Vec<_> = bytes.iter().positions(|&c| c == b'\r').collect();
    cr_indexes.retain(|&idx| idx == bytes.len() - 1 || bytes[idx + 1] == b'\n');
    cr_indexes.insert(0, 0);
    let last_idx = cr_indexes[cr_indexes.len() - 1];
    if last_idx == bytes.len() - 2 || last_idx == bytes.len() - 1 {
        // The last idx is at the end of the file (accounting for "\r\n").
    } else {
        // It isn't. Be sure to include the trailing data in a slice.
        cr_indexes.push(bytes.len() - 1);
    }

    cr_indexes
        .windows(2)
        .map(|window| &bytes[window[0]..window[1]])
        .collect()
}

const EMPTY: [&[u8]; 0] = [];

fn write_output_files(config: &Configuration, results: &[ParseLineResult]) -> Result<usize, io::Error> {
    const SUCCESS_FILE: &str = "consolidated.csv";
    const ERROR_FILE: &str = "errors.csv";

    let mut error_count = 0;
    let mut success_writer = WriterBuilder::new()
        .flexible(true)
        .from_path(SUCCESS_FILE)?;
    let mut error_writer = WriterBuilder::new().flexible(true).from_path(ERROR_FILE)?;

    success_writer.write_record(config.columns.iter())?;
    error_writer.write_field("Source")?;
    error_writer.write_field("LineNum")?;
    error_writer.write_field("Message")?;
    error_writer.write_field("Line")?;
    error_writer.write_record(&EMPTY)?;

    for result in results {
        match result {
            Ok(parsed_line) => write_line(config, &mut success_writer, parsed_line)?,
            Err(parsed_line_error) => {
                error_writer.write_field(parsed_line_error.source)?;
                error_writer.write_field(parsed_line_error.line_num.to_string())?;
                error_writer.write_field(&parsed_line_error.message)?;
                error_writer.write_field(parsed_line_error.line)?;
                error_writer.write_record(&EMPTY)?;
                error_count += 1;
            }
        }
    }

    success_writer.flush()?;
    error_writer.flush()?;

    // Did we need this file?
    if error_count == 0 {
        fs::remove_file(ERROR_FILE)?;
    }

    Ok(error_count)
}

fn write_line(config: &Configuration, writer: &mut csv::Writer<std::fs::File>, line: &ParsedLine) -> Result<(), io::Error> {
    for column in &config.columns {
        match column.as_str() {
            kvp::LOG_DATE => writer.write_field(line.log_date)?,
            kvp::LOG_LEVEL => writer.write_field(line.log_level)?,
            kvp::LOG_SOURCE => writer.write_field(line.source)?,
            kvp::MESSAGE => writer.write_field(&line.message)?,
            _ => {
                if let Some(kvp_value) = line.kvps.get_value(column.as_bytes()) {
                    writer.write_field(&kvp_value)?;
                } else {
                    // Check for the column under any alternative names.
                    let mut did_write = false;
                    if let Some(alternate_names) = config.alternate_column_names.get(column) {
                        for alt_name in alternate_names {
                            if let Some(kvp_value) = line.kvps.get_value(alt_name.as_bytes()) {
                                writer.write_field(&kvp_value)?;
                                did_write = true;
                                break;
                            }
                        }
                    }

                    if !did_write {
                        writer.write_field(b"")?;
                    }
                }
            }
        }
    }

    writer.write_record(&EMPTY)?;

    Ok(())
}
