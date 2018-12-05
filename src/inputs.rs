use std::fs;
use std::path::{Path, PathBuf};
use crate::config::Config;

#[derive(Default, Clone, Debug)]
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
        let output_path = format!("{}.out", &path_as_string);
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

#[derive(Default, Clone, Debug)]
pub struct Inputs {
    pub input_files: Vec<InputFile>,
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

        i
    }

    pub fn is_empty(&self) -> bool {
        self.input_files.is_empty()
    }

    pub fn longest_filename_length(&self) -> usize {
        self.input_files.iter().map(|f| f.filename_only_as_string.len()).max().unwrap()
    }

    fn contains_file(&self, path: &Path) -> bool {
        self.input_files.iter().any(|f| f.path == path)
    }
}