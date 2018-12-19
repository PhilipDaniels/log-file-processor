use std::collections::HashMap;
use crate::regexes;

/// The config module represents the program's configuration. It is intended to be
/// the home of the command line and config file parsing. The top-level struct is
/// 'Config'.
///
/// Before execution begins, the Config is processed into 'Inputs' (see the inputs
/// module) which represents the actual set of files to be processed.

pub const LOG_DATE: &str = "LogDate";
pub const LOG_LEVEL: &str = "LogLevel";
pub const MESSAGE: &str = "Message";

impl Default for OutputFileSpecification {
    fn default() -> Self {
        let mut ofs = OutputFileSpecification {
            column_extractors: HashMap::new(),
            columns: vec![
                LOG_DATE.to_string(),
                LOG_LEVEL.to_string(),
                "PID".to_string(),
                "TID".to_string(),
                "MachineName".to_string(),
                "AppName".to_string(),
                "SysRef".to_string(),
                "Action".to_string(),
                "CorrelationKey".to_string(),
                "CallRecorderExecutionTime".to_string(),
                MESSAGE.to_string(),
                // "Source".to_string(),
                // "SourceInstance".to_string(),
                // "SourceInfo".to_string(),
                // "Owin.Request.QueryString".to_string(),
                // "Owin.Request.Path".to_string(),
            ],
        };

        ofs.column_extractors.insert("AppName".to_string(), regexes::make_kvp_pattern_with_alternate_key("AppName", "ApplicationName"));
        ofs.column_extractors.insert("LogLevel".to_string(), r###"\[(VRBSE)\]|\[(DEBUG)\]|\[(INFO_)\]|\[(WARNG)\]|\[(ERROR)\]|\[(FATAL)\]"###.to_string());
        ofs.column_extractors.insert("LogDate".to_string(), regexes::make_log_date_pattern());
        ofs
    }
}

/// This is the top-level struct for this module.
#[derive(Clone, Debug)]
pub struct Config {
    pub input_file_specs: Vec<String>,
    pub output_file_spec: OutputFileSpecification
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_file_specs: vec!["*.log".to_string()],
            output_file_spec: OutputFileSpecification::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutputFileSpecification {
    /// A simple list of column names, these will become the headers
    /// in the output file.
    pub columns: Vec<String>,
    /// A sparse map of ColumnName -> Regex, regular expressions to be used to extract
    /// each column. If a column has no entry in here, a default regex is used that knows
    /// how to extract a a Key-Value pair from one of our log files 
    pub column_extractors: HashMap<String, String>,
}
