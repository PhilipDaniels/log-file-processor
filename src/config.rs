pub struct Config {
    pub input_file_specs: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_file_specs: vec!["*.log".to_string()],
        }
    }
}