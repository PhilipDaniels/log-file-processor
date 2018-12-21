use std::fs;
use std::path::{Path, PathBuf};
use regex::{Regex};
use crate::config::Config;
use crate::regexes;

/// The inputs module represents the set of files to be processed by the program.
/// The top-level struct is 'Inputs'. This is constructed based on the 'Config'.
/// Various simple things are pre-calculated to avoid having to calculate them in
/// the middle of parsing a file.


/// Represents the complete set of inputs to be processed.
#[derive(Default, Clone, Debug)]
pub struct Inputs {
    /// The set of columns to be extracted.
    pub columns: Vec<Column>,
    /// The set of log files to be processed.
    pub input_files: Vec<InputFile>,
}

#[derive(Default, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InputFile {
    pub path: PathBuf,
    pub length: usize,
    pub path_as_string: String,
    pub filename_only_as_string: String,
    pub output_path: String
}

impl InputFile {
    /// Construct a new InputFile object based on a path.
    /// Pre-calculate various things for use later on, so that we
    /// can avoid cluttering the main code with incidentals.
    pub fn new(path: PathBuf) -> Self {
        let path_as_string = path.to_str().expect("Path should be a valid UTF-8 string").to_owned();
        let output_path = format!("{}.csv", &path_as_string);
        let filename_as_string = path.file_name().expect("Path should have a filename component").to_str().unwrap().to_owned();
        let length = fs::metadata(&path).expect("Can get file meta data").len() as usize;

        InputFile {
            path: path,
            length: length,
            path_as_string: path_as_string,
            filename_only_as_string: filename_as_string,
            output_path: output_path
        }
    }
}

/// Represents one column to be extracted. This consists of a name and a regex that will be
/// used to extract that column if it cannot be found as a standard KVP.
#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub regex: Regex
}

pub fn is_date_column(column_name: &str) -> bool {
    let n = column_name.to_lowercase();
    n.ends_with("date") || n.ends_with("datetime")
}

impl Inputs {
    pub fn new_from_config(config: &Config) -> Self {
        use glob::glob;

        let mut i = Inputs::default();

        // Determine available input files.
        for path in &config.input_file_specs {
            for entry in glob(&path).expect("Failed to read glob pattern.") {
                match entry {
                    Ok(path) => if !i.contains_file(&path) {
                        i.input_files.push(InputFile::new(path))
                    },
                    Err(e) => {
                        eprintln!("Could not read glob entry, ignoring. Error is {}", e)
                    }
                }
            }
        }

        i.input_files.sort();

        // Build complete set of columns. Each column is given a regex that
        // will extract it. Some of these are duplicates, but by pre-calculating
        // all these entries we ensure downstream code is simpler, as it can
        // focus on extracting data rather than ensuring it has all the things
        // it needs to do that extraction.
        for column in &config.output_file_spec.columns {
            if i.contains_column(column) {
                eprintln!("The column {} is already specified, ignoring subsequent specification.", column);
                continue;
            }

            let regex = match config.output_file_spec.column_extractors.get(column) {
                Some(custom_pattern) => regexes::make_regex_for_pattern(custom_pattern),
                None                 => regexes::make_regex_for_column(column),
            };
            
            i.columns.push(Column { name: column.to_string(), regex: regex });
        }

        i
    }

    pub fn is_empty(&self) -> bool {
        self.input_files.is_empty()
    }

    pub fn longest_input_name_len(&self) -> usize {
        self.input_files.iter().map(|f| f.filename_only_as_string.len()).max().unwrap()
    }

    fn contains_file(&self, path: &Path) -> bool {
        self.input_files.iter().any(|f| f.path == path)
    }

    fn contains_column(&self, column: &str) -> bool {
        self.columns.iter().any(|c| c.name.to_lowercase() == column.to_lowercase())
    }
}
