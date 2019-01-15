use std::fs;
use std::path::{Path, PathBuf};
use crate::configuration::Configuration;

/// The inputs module represents the set of files to be processed by the program.
/// The top-level struct is 'Inputs'. This is constructed based on the 'Configuration'.
/// Various simple things are pre-calculated to avoid having to calculate them in
/// the middle of parsing a file.


/// Represents the complete set of inputs to be processed.
#[derive(Default, Clone, Debug)]
pub struct Inputs {
    /// The set of log files to be processed.
    pub files: Vec<InputFile>,
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
            path,
            length,
            path_as_string,
            filename_only_as_string: filename_as_string,
            output_path,
        }
    }
}

impl Inputs {
    pub fn new_from_config(config: &Configuration) -> Self {
        use glob::glob;

        let mut i = Inputs::default();

        // Determine available input files.
        for path in &config.file_patterns {
            for entry in glob(&path).expect("Failed to read glob pattern.") {
                match entry {
                    Ok(path) => if !i.contains_file(&path) {
                        i.files.push(InputFile::new(path))
                    },
                    Err(e) => {
                        eprintln!("Could not read glob entry, ignoring. Error is {}", e)
                    }
                }
            }
        }

        // Put largest file first. Idea is that it will be the limiting time
        // for the whole program to complete.
        i.files.sort_by_key(|ifile| -(ifile.length as i64));
        i
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    // pub fn longest_input_name_len(&self) -> usize {
    //     self.files.iter().map(|f| f.filename_only_as_string.len()).max().unwrap()
    // }

    fn contains_file(&self, path: &Path) -> bool {
        self.files.iter().any(|f| f.path == path)
    }

    pub fn total_bytes(&self) -> usize {
        self.files.iter().map(|f| f.length).sum()
    }
}
