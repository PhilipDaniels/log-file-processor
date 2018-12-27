use std::collections::HashMap;
use serde_derive::{Serialize, Deserialize};
use structopt::StructOpt;
use crate::parse_utils::{LOG_DATE, LOG_LEVEL, MESSAGE};

const DEFAULT_PROFILE_NAME: &str = "default";
const DEFAULT_MAX_MESSAGE_LENGTH: usize = 1000000;


/// Represents command-line arguments.
#[derive(StructOpt, Debug)] 
pub struct Arguments {
    /// The name of the profile to read from the configuration file.
    /// Profiles are additive - first the default profile is applied, then this profile,
    /// if any, is applied on top to produce the effective profile. This keeps most custom
    /// defined profiles very short. To completely suppress the default profile, use
    /// the `no-default-profile` flag.
    #[structopt(short = "p", long = "profile", default_value = "default")]
    profile: String,

    /// Suppresses loading of the default profile, meaning that the profile you
    /// name will be the only one applied.
    #[structopt(short = "D", long = "no-default-profile")]
    no_default_profile: bool,

    /// If true, run quietly, without any progress bars.
    #[structopt(short = "q", long = "quiet")]
    quiet: Option<bool>,

    /// Specifies the maximum length of the message component when written to the output.
    /// Some log lines are extremely long and can generate warnings in LibreOffice or Excel,
    /// this allows them to be trimmed down to something more reasonable.
    #[structopt(short = "m", long = "max-message-length")]
    max_message_length: Option<usize>,

    /// If true, dumps an example configuration file, based on the default configuration,
    /// to stdout.
    #[structopt(short = "d", long = "dump-config")]
    pub dump_config: bool,

    /// List of files to process. Defaults to "*.log".
    #[structopt(name = "FILE")]
    files: Vec<String>,
}

#[cfg(test)]
impl Default for Arguments {
    fn default() -> Self {
        Arguments {
            profile: DEFAULT_PROFILE_NAME.to_string(),
            no_default_profile: false,
            quiet: None,
            max_message_length: None,
            dump_config: false,
            files: vec![]
        }
    }
}

/// Represents a complete set of configuration options.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Configuration {
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

fn vec_has_entry(column_name: &str, vec: &Vec<String>) -> bool {
    vec.iter().any(|c| c.eq_ignore_ascii_case(column_name))
}

fn vec_add_entry(column_name: String, vec: &mut Vec<String>) {
    if !vec_has_entry(&column_name, vec) {
        vec.push(column_name);
    }
}

impl Configuration {
    fn blank() -> Self {
        Configuration {
            name: "blank".to_string(),
            quiet: false,
            max_message_length: DEFAULT_MAX_MESSAGE_LENGTH,
            columns: Vec::new(),
            alternate_column_names: HashMap::new(),
            file_patterns: Vec::new(),
            column_regexes: HashMap::new(),
        }
    }

    fn has_column(&self, column_name: &str) -> bool {
        vec_has_entry(column_name, &self.columns) ||
            self.alternate_column_names.values().any(|acns| vec_has_entry(column_name, acns))
    }

    fn add_column(&mut self, column_name: String) {
        vec_add_entry(column_name, &mut self.columns);
    }

    fn add_alternate_column(&mut self, main_column_name: &str, alternate_column_name: String) {
        let alternate_names = self.alternate_column_names.entry(main_column_name.to_string()).or_default();
        vec_add_entry(alternate_column_name, alternate_names);
    }

    fn add_file_pattern(&mut self, file_pattern: String) {
        vec_add_entry(file_pattern, &mut self.file_patterns);
    }
}

impl Default for Configuration {
    fn default() -> Self {
        let mut p = Self::blank();
        p.name = DEFAULT_PROFILE_NAME.to_string();

        p.add_column(LOG_DATE.to_string());
        p.add_column(LOG_LEVEL.to_string());
        p.add_column("MachineName".to_string());
        p.add_column("AppName".to_string());
        p.add_column("PID".to_string());
        p.add_column("TID".to_string());
        p.add_column("SysRef".to_string());
        p.add_column("Action".to_string());
        p.add_column("Source".to_string());
        p.add_column("CorrelationKey".to_string());
        p.add_column("CallRecorderExecutionTime".to_string());
        p.add_column("Http.RequestId".to_string());
        p.add_column("Http.RequestQueryString".to_string());
        p.add_column("Http.Request.Path".to_string());
        p.add_column("UserName".to_string());
        p.add_column("UserIdentity".to_string());
        p.add_column(MESSAGE.to_string());

        p.add_alternate_column("AppName", "ApplicationName".to_string());
        p.add_alternate_column("Http.RequestId", "Owin.Request.Id".to_string());
        p.add_alternate_column("Http.RequestQueryString", "Owin.Request.QueryString".to_string());
        p.add_alternate_column("Http.Request.Path", "Owin.Request.Path".to_string());

        p
    }
}

/// The `Options` is just a hash-map of Configuration structs as loaded
/// from the `~/.lpf.json` configuration file.
#[derive(Serialize, Deserialize, Debug)]
pub struct Options {
    configs: HashMap<String, Configuration>
}

impl Default for Options {
    fn default() -> Self {
        let mut options = Options {
            configs: HashMap::new(),
        };
        let p = Configuration::default();
        options.configs.insert(p.name.clone(), p);

        options
    }
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
mod vec_tests {
    use super::*;

    #[test]
    pub fn vec_has_column_for_same_case() {
        let columns = vec!["alpha".to_string()];
        assert!(vec_has_entry("alpha", &columns));
    }

    #[test]
    pub fn vec_has_column_for_different_case() {
        let columns = vec!["alpha".to_string()];
        assert!(vec_has_entry("ALPHA", &columns));
    }

    #[test]
    pub fn vec_has_column_for_no_match() {
        let columns = vec!["alpha".to_string()];
        assert!(!vec_has_entry("beta", &columns));
    }

    #[test]
    pub fn vec_add_column_for_column_not_present() {
        let mut columns = vec!["alpha".to_string()];
        vec_add_entry("beta".to_string(), &mut columns);
        assert_eq!(columns, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn vec_add_column_for_column_present() {
        let mut columns = vec!["alpha".to_string()];
        vec_add_entry("alpha".to_string(), &mut columns);
        assert_eq!(columns, vec!["alpha".to_string()]);
    }
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    pub fn has_column_for_matching_column() {
        let mut p = Configuration::blank();
        p.add_column("alpha".to_string());
        assert!(p.has_column("alpha"));
    }

    #[test]
    pub fn has_column_for_matching_alternate_column() {
        let mut p = Configuration::blank();
        p.alternate_column_names.insert("alpha".to_string(), vec!["beta".to_string()]);
        assert!(p.has_column("beta"));
    }

    #[test]
    pub fn add_column_for_column_that_exists() {
        let mut p = Configuration::blank();
        p.add_column("alpha".to_string());
        p.add_column("alpha".to_string());
        assert_eq!(p.columns, vec!["alpha".to_string()]);
    }

    #[test]
    pub fn add_column_for_column_that_does_not_exist() {
        let mut p = Configuration::blank();
        p.add_column("alpha".to_string());
        p.add_column("beta".to_string());
        assert_eq!(p.columns, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_not_present_adds() {
        let mut p = Configuration::blank();
        p.add_alternate_column("main", "alpha".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_present_and_column_not_present_adds() {
        let mut p = Configuration::blank();
        p.add_alternate_column("main", "alpha".to_string());
        p.add_alternate_column("main", "beta".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_present_and_column_present_does_not_add() {
        let mut p = Configuration::blank();
        p.add_alternate_column("main", "alpha".to_string());
        p.add_alternate_column("main", "alpha".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string()]);
    }
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