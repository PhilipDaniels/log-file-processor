use std::collections::HashMap;
use crate::arguments::Arguments;
use crate::profiles::{Configuration, Options};

pub const DEFAULT_PROFILE_NAME: &str = "default";
pub const DEFAULT_MAX_MESSAGE_LENGTH: usize = 1000000;

#[derive(Debug)]
pub struct Configuration2 {
    /// The name of the profile.
    pub name: String,
    pub quiet: bool,
    pub max_message_length: usize,

    /// A simple list of column names, these will become the headers in the output file.
    pub columns: Vec<String>,

    /// A sparse map specifying alternate names for a column. The nominal column name
    /// is the key of the HashMap, it is this column name which should appear in the 
    /// columns collection. If a value for a column cannot be found under its preferred
    /// name, then the vector is checked for any alternate names and a lookup is
    /// attempted for them. This allows for instance, a column called "AppName" to locate
    /// a value using "AppName" or "ApplicationName".
    pub alternate_column_names: HashMap<String, Vec<String>>,

    /// The files to process.
    pub file_patterns: Vec<String>,

    /// A sparse map of ColumnName -> Regex, regular expressions to be used to extract
    /// each column. If a column has no entry in here, then it is retrieved from the
    /// extracted KVPs or using a default regex to probe the message text itself.
    pub column_regexes: HashMap<String, String>,
}


/// Represents the final configuration, being a combination of
///    the options (as loaded from file)
///    to which the arguments have been applied
///    then the appropriate inputs constructed.
pub fn get_config(options: &Options, args: &Arguments) -> Configuration {
    // Determine the baseline configuration to which we will apply any overrides.
    // If there is a profile named "default" in the .lpf.json file we use it - this allows
    // the user to customize the default profile - otherwise we just generate one in code.
    let mut config = match args.no_default_profile {
        true => Configuration::blank(),
        false => options.configs.get(DEFAULT_PROFILE_NAME).map_or(Configuration::default(), |p| p.clone())
    };

    if args.profile != DEFAULT_PROFILE_NAME {
        let override_profile = options.configs.get(&args.profile)
            .expect(&format!("Profile '{}' does not exist", args.profile));

        config.name = override_profile.name.clone();
        config.quiet = override_profile.quiet;

        for column_name in &override_profile.columns {
            config.add_column(column_name.clone());
        }

        for (main_column_name, alternate_names_for_column) in &override_profile.alternate_column_names {
            for alt_name in alternate_names_for_column {
                config.add_alternate_column(main_column_name, alt_name.to_string());
            }
        }

        for pat in &override_profile.file_patterns {
            config.add_file_pattern(pat.to_string());
        }
    }

    // Now apply overrides from the command line arguments.
    if let Some(quiet) = args.quiet {
        config.quiet = quiet;
    }
    if let Some(max_message_length) = args.max_message_length {
        config.max_message_length = max_message_length;
    }
    for pat in &args.files {
        config.add_file_pattern(pat.to_string());
    }

    // Default if no profile or command line specifies a file pattern.
    // Means we will process everything in the current directory.
    if config.file_patterns.is_empty() {
        config.add_file_pattern("*.log".to_string());
    }

    config
}

#[cfg(test)]
mod get_config_tests {
    use super::*;

    fn make_options_with_override() -> Options {
        let mut options = Options::default();
        let mut p = make_override_configuration();
        options.configs.insert(p.name.clone(), p);
        options
    } 

    fn make_override_configuration() -> Configuration {
        let mut config = Configuration::blank();
        config.name = "over".to_string();
        config.quiet = true;
        config.add_column("col1".to_string());
        config.add_column("col2".to_string());
        config.add_alternate_column("PID", "ProcessId".to_string());
        config.add_alternate_column("PID", "ProcId".to_string());
        config.add_alternate_column("TID", "ThreadId".to_string());
        config.add_file_pattern("case*.log".to_string());
        config
    }

    /// Checks that all the default columns are in a column collection.
    fn has_default_columns(columns: &[String]) {
        let def = Configuration::default();
        for col in &def.columns {
            assert!(columns.contains(col));
        }
    }

    #[test]
    pub fn sets_command_line_arguments_quiet_correctly() {
        let options = Options::default();
        let mut args = Arguments::default();

        args.quiet = Some(true);
        let config = get_config(&options, &args);
        assert!(config.quiet);

        args.quiet = Some(false);
        let config = get_config(&options, &args);
        assert!(!config.quiet);
    }

    #[test]
    pub fn sets_command_line_arguments_max_message_length_correctly() {
        let options = Options::default();
        let mut args = Arguments::default();

        args.max_message_length = Some(20);
        let config = get_config(&options, &args);
        assert_eq!(config.max_message_length, 20);

        args.max_message_length = None;
        let config = get_config(&options, &args);
        assert_eq!(config.max_message_length, DEFAULT_MAX_MESSAGE_LENGTH);
    }

    #[test]
    pub fn for_no_default_profile_returns_blank() {
        let options = Options::default();
        let mut args = Arguments::default();
        args.no_default_profile = true;

        let config = get_config(&options, &args);
        assert_eq!(config.name, "blank");
        assert!(config.columns.is_empty());
    }

    #[test]
    pub fn override_profile_name_and_quiet_are_set_correctly() {
        let options = make_options_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&options, &args);
        assert_eq!(config.name, "over");
        assert_eq!(config.quiet, true);
    }

    #[test]
    pub fn override_profile_adds_columns() {
        let options = make_options_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&options, &args);
        
        assert!(config.columns.contains(&"col1".to_string()));
        assert!(config.columns.contains(&"col2".to_string()));
        has_default_columns(&config.columns);
    }

    #[test]
    pub fn override_profile_adds_alternate_columns() {
        let options = make_options_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&options, &args);
        
        assert!(config.alternate_column_names["PID"].contains(&"ProcessId".to_string()));
        assert!(config.alternate_column_names["PID"].contains(&"ProcId".to_string()));
        assert!(config.alternate_column_names["TID"].contains(&"ThreadId".to_string()));
    }

    #[test]
    pub fn override_profile_adds_file_patterns() {
        let options = make_options_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&options, &args);
        
        assert_eq!(config.file_patterns, vec!["case*.log"]);
    }

    #[test]
    pub fn for_no_file_patterns_in_args_or_config_adds_default() {
        let mut p = make_override_configuration();
        p.file_patterns.clear();
        let mut options = Options::default();
        options.configs.insert(p.name.clone(), p);

        let mut args = Arguments::default();
        args.profile = "over".to_string();
        args.files.clear();

        let config = get_config(&options, &args);
        
        assert_eq!(config.file_patterns, vec!["*.log"]);
    }
}