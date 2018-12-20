use std::collections::{HashMap, HashSet};
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

/// This is the top-level struct for this module.
#[derive(Clone, Debug)]
pub struct Config {
    pub input_file_specs: Vec<String>,
    pub output_file_spec: OutputFileSpecification
}

#[derive(Clone, Debug)]
pub struct OutputFileSpecification {
    /// A simple list of column names, these will become the headers in the output file.
    pub columns: Vec<String>,

    /// A sparse map specifying alternate names for a column. The nominal column name
    /// is the key of the HashMap, it is this column name which should appear in the 
    /// columns collection. If a value for a column cannot be found under its preferred
    /// name, then the vector is checked for any alternate names and a lookup is
    /// attempted for them. This allows for instance, a column called "AppName" to locate
    /// a value using "AppName" or "ApplicationName".
    pub alternate_column_names: HashMap<String, Vec<String>>,

    /// A sparse map of ColumnName -> Regex, regular expressions to be used to extract
    /// each column. If a column has no entry in here, a default regex is used that knows
    /// how to extract a a Key-Value pair from one of our log files 
    pub column_extractors: HashMap<String, String>,
}

impl Default for OutputFileSpecification {
    fn default() -> Self {
        let mut ofs = OutputFileSpecification {
            column_extractors: HashMap::new(),
            alternate_column_names: HashMap::new(),

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

        ofs.alternate_column_names.insert("AppName".to_string(), vec!["ApplicationName".to_string()]);

        // ofs.column_extractors.insert("AppName".to_string(), regexes::make_kvp_pattern_with_alternate_key("AppName", "ApplicationName"));
        // ofs.column_extractors.insert("LogLevel".to_string(), r###"\[(VRBSE)\]|\[(DEBUG)\]|\[(INFO_)\]|\[(WARNG)\]|\[(ERROR)\]|\[(FATAL)\]"###.to_string());
        // ofs.column_extractors.insert("LogDate".to_string(), regexes::make_log_date_pattern());
        ofs
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_file_specs: vec!["*.log".to_string()],
            output_file_spec: OutputFileSpecification::default()
        }
    }
}
