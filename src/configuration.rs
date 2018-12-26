use std::collections::HashMap;
use serde_derive::{Serialize, Deserialize};
use structopt::StructOpt;
use crate::parse_utils::{LOG_DATE, LOG_LEVEL, MESSAGE};

const DEFAULT_PROFILE_NAME: &str = "default";

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

    #[structopt(short = "D", long = "no-default-profile")]
    no_default_profile: bool,

    /// If true, run quietly, without any progress bars.
    #[structopt(short = "q", long = "quiet")]
    quiet: Option<bool>,

    /// Specifies the maximum length of the message component when written to the output.
    /// Some log lines are extremely long and can generate warnings in LibreOffice or Excel,
    /// this allows them to be trimmed down to something more reasonable.
    #[structopt(short = "m", long = "max-message-length", default_value = "1000000")]
    max_message_length: usize,

    /// If true, dumps an example configuration file, based on the default configuration,
    /// to stdout.
    #[structopt(short = "d", long = "dump-config")]
    pub dump_config: bool,

    /// List of files to process.
    #[structopt(name = "FILE", default_value = "*.log")]
    files: Vec<String>,
}

#[cfg(test)]
impl Default for Arguments {
    fn default() -> Self {
        Arguments {
            profile: DEFAULT_PROFILE_NAME.to_string(),
            no_default_profile: false,
            quiet: Some(false),
            max_message_length: 1000000,
            dump_config: false,
            files: vec!["*.log".to_string()]
        }
    }
}

/// Represents options as read from the configuration file (which is optional,
/// in which case the default is used). This is a single profile; the `Options` struct
/// may contain several of them.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OptionsProfile {
    /// The name of the profile.
    pub name: String,
    pub quiet: bool,

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

fn vec_has_column(column_name: &str, columns: &Vec<String>) -> bool {
    columns.iter().any(|c| c.eq_ignore_ascii_case(column_name))
}

fn vec_add_column(column_name: String, columns: &mut Vec<String>) {
    if !vec_has_column(&column_name, columns) {
        columns.push(column_name);
    }
}

impl OptionsProfile {
    fn blank() -> Self {
        OptionsProfile {
            name: "blank".to_string(),
            quiet: false,
            columns: Vec::new(),
            alternate_column_names: HashMap::new(),
            file_patterns: Vec::new(),
            column_regexes: HashMap::new(),
        }
    }

    fn has_column(&self, column_name: &str) -> bool {
        vec_has_column(column_name, &self.columns) ||
            self.alternate_column_names.values().any(|acns| vec_has_column(column_name, acns))
    }

    fn add_column(&mut self, column_name: String) {
        vec_add_column(column_name, &mut self.columns);
    }

    fn add_alternate_column(&mut self, main_column_name: &str, alternate_column_name: String) {
        let alternate_names = self.alternate_column_names.entry(main_column_name.to_string()).or_default();
        vec_add_column(alternate_column_name, alternate_names);
    }
}


impl Default for OptionsProfile {
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

        p.file_patterns.push("*.log".to_string());
        p.add_alternate_column("AppName", "ApplicationName".to_string());
        p.add_alternate_column("Http.RequestId", "Owin.Request.Id".to_string());
        p.add_alternate_column("Http.RequestQueryString", "Owin.Request.QueryString".to_string());
        p.add_alternate_column("Http.Request.Path", "Owin.Request.Path".to_string());

        p
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Options {
    profiles: HashMap<String, OptionsProfile>
}

impl Default for Options {
    fn default() -> Self {
        let mut options = Options {
            profiles: HashMap::new(),
        };
        let p = OptionsProfile::default();
        options.profiles.insert(p.name.clone(), p);

        options
    }
}

/// Represents the final configuration, being a combination of
///    the options
///    to which the arguments have been applied
///    then the appropriate inputs constructed.
#[derive(Debug, Default)]
pub struct Configuration {
    pub max_message_length: usize,
    pub profile: OptionsProfile,
}

impl Configuration {
    pub fn new(options: &Options, args: &Arguments) -> Self {
        let mut config = Self::default();

        // Determine the baseline profile to which we will apply any overrides.
        // There might not be a default profile in the .lpf.json file.
        config.profile = match args.no_default_profile {
            true => OptionsProfile::blank(),
            false => options.profiles.get(DEFAULT_PROFILE_NAME).map_or(OptionsProfile::default(), |p| p.clone())
        };

        let override_profile = options.profiles.get(&args.profile).expect(&format!("Profile '{}' does not exist", args.profile));
        config.profile.name = override_profile.name.clone();
        config.profile.quiet = override_profile.quiet;

        // for column_name in &override_profile.columns {
        //     p.add_column(column_name.clone());
        // }

        // for (column_name, alternate_names_for_column) in &override_profile.alternate_column_names {
        // }


        // Now apply overrides from the command line arguments.
        if let Some(quiet) = args.quiet {
            config.quiet = quiet;
        }

        config
    }
}

#[cfg(test)]
mod vec_tests {
    use super::*;

    #[test]
    pub fn vec_has_column_for_same_case() {
        let columns = vec!["alpha".to_string()];
        assert!(vec_has_column("alpha", &columns));
    }

    #[test]
    pub fn vec_has_column_for_different_case() {
        let columns = vec!["alpha".to_string()];
        assert!(vec_has_column("ALPHA", &columns));
    }

    #[test]
    pub fn vec_has_column_for_no_match() {
        let columns = vec!["alpha".to_string()];
        assert!(!vec_has_column("beta", &columns));
    }

    #[test]
    pub fn vec_add_column_for_column_not_present() {
        let mut columns = vec!["alpha".to_string()];
        vec_add_column("beta".to_string(), &mut columns);
        assert_eq!(columns, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn vec_add_column_for_column_present() {
        let mut columns = vec!["alpha".to_string()];
        vec_add_column("alpha".to_string(), &mut columns);
        assert_eq!(columns, vec!["alpha".to_string()]);
    }
}

#[cfg(test)]
mod options_profile_tests {
    use super::*;

    #[test]
    pub fn has_column_for_matching_column() {
        let mut p = OptionsProfile::blank();
        p.add_column("alpha".to_string());
        assert!(p.has_column("alpha"));
    }

    #[test]
    pub fn has_column_for_matching_alternate_column() {
        let mut p = OptionsProfile::blank();
        p.alternate_column_names.insert("alpha".to_string(), vec!["beta".to_string()]);
        assert!(p.has_column("beta"));
    }

    #[test]
    pub fn add_column_for_column_that_exists() {
        let mut p = OptionsProfile::blank();
        p.add_column("alpha".to_string());
        p.add_column("alpha".to_string());
        assert_eq!(p.columns, vec!["alpha".to_string()]);
    }

    #[test]
    pub fn add_column_for_column_that_does_not_exist() {
        let mut p = OptionsProfile::blank();
        p.add_column("alpha".to_string());
        p.add_column("beta".to_string());
        assert_eq!(p.columns, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_not_present_adds() {
        let mut p = OptionsProfile::blank();
        p.add_alternate_column("main", "alpha".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_present_and_column_not_present_adds() {
        let mut p = OptionsProfile::blank();
        p.add_alternate_column("main", "alpha".to_string());
        p.add_alternate_column("main", "beta".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_present_and_column_present_does_not_add() {
        let mut p = OptionsProfile::blank();
        p.add_alternate_column("main", "alpha".to_string());
        p.add_alternate_column("main", "alpha".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string()]);
    }
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    pub fn new_sets_quiet_correctly() {
        let options = Options::default();
        let mut args = Arguments::default();

        args.quiet = Some(true);
        let config = Configuration::new(&options, &args);
        assert!(config.quiet);

        args.quiet = Some(false);
        let config = Configuration::new(&options, &args);
        assert!(!config.quiet);
    }

    #[test]
    pub fn new_for_no_default_profile_returns_blank() {
        let options = Options::default();
        let mut args = Arguments::default();
        args.no_default_profile = true;

        let config = Configuration::new(&options, &args);
        // 'blank' gets overwritten with 'default', but is irrelevant to program execution anyway.
        assert_eq!(config.profile.name, "default");
        assert!(config.profile.columns.is_empty());
    }

    #[test]
    pub fn override_profile_name_and_quiet_are_set_correctly() {
        let options = Options::default();
        let mut args = Arguments::default();

        let config = Configuration::new(&options, &args);
        assert_eq!(config.profile.name, "over");
        assert_eq!(config.profile.quiet, "over");
        assert!(config.profile.columns, vec!["col1", "col2"]);
    }
}