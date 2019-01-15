use std::collections::{HashMap};
use regex::{Regex, RegexBuilder};
use crate::arguments::Arguments;
use crate::profiles::{Profile, ProfileSet, vec_add_entry};

pub const DEFAULT_PROFILE_NAME: &str = "default";
pub const DEFAULT_MAX_MESSAGE_LENGTH: usize = 1_000_000;

#[derive(Debug)]
pub struct Configuration {
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
    pub column_regexes: HashMap<String, Regex>,

    /// List of sysrefs to filter by. Can be empty, in which case no filtering is done.
    /// If non-empty, then the line must have one of these sysrefs to be written to
    /// the output.
    pub sysrefs: Vec<String>,
}

/// Makes a regex that extracts key-value pairs of the form
///    Key=Value
///    or
///    Key="Some value in double quotes"
fn make_kvp_pattern(key_name: &str) -> String {
    format!(r###"\W{0}="(.*?)"|\W{0}=(\S*)"###, regex::escape(key_name))
}

fn make_case_insensitive_regex_for_pattern(pattern: &str) -> Regex {
    RegexBuilder::new(pattern).case_insensitive(true).build().unwrap()
}

impl From<Profile> for Configuration {
    fn from(p: Profile) -> Self {
        let mut config = Configuration {
            name: p.name,
            quiet: p.quiet.unwrap_or(false),
            max_message_length: p.max_message_length.unwrap_or(DEFAULT_MAX_MESSAGE_LENGTH),
            columns: p.columns,
            alternate_column_names: p.alternate_column_names,
            file_patterns: p.file_patterns,
            column_regexes: HashMap::new(),
            sysrefs: vec![],
        };

        // Insert any custom regexes.
        for (column_name, pattern) in p.column_regexes {
            config.add_column_regex(column_name, &pattern);
        }

        // For all columns that don't have a custom regex, use a standard KVP one.
        // We need a separate regex for each column because the name of the column
        // is included in the regex pattern.
        let cols = config.columns.clone();
        for column in cols {
            if !config.column_regexes.contains_key(&column) {
                let pattern = make_kvp_pattern(&column);
                config.add_column_regex(column, &pattern);
            }
        }

        config
    }
}

impl Configuration {
    pub fn add_column<S>(&mut self, column_name: S)
        where S: Into<String>
    {
        vec_add_entry(column_name, &mut self.columns);
    }

    pub fn add_alternate_column<S>(&mut self, main_column_name: &str, alternate_column_name: S)
        where S: Into<String>
    {
        let alternate_names = self.alternate_column_names.entry(main_column_name.to_string()).or_default();
        vec_add_entry(alternate_column_name, alternate_names);
    }

    pub fn add_file_pattern<S>(&mut self, file_pattern: S)
        where S: Into<String>
    {
        vec_add_entry(file_pattern, &mut self.file_patterns);
    }

    pub fn add_column_regex<S>(&mut self, column_name: S, pattern: &str)
        where S: Into<String>
    {
        let regex = make_case_insensitive_regex_for_pattern(pattern);
        self.column_regexes.insert(column_name.into(), regex);
    }
}

/// Represents the final configuration, being a combination of
///    the profiles (as loaded from file)
///    to which the arguments have been applied
///    then the appropriate inputs constructed.
pub fn get_config(profiles: &ProfileSet, args: &Arguments) -> Configuration {
    // Determine the baseline profile to which we will apply any overrides.
    // If there is a profile named "default" in the .lpf.json file we use it - this allows
    // the user to customize the default profile - otherwise we just generate one in code.
    let profile = if args.no_default_profile {
        Profile::blank()
    } else {
        profiles.get(DEFAULT_PROFILE_NAME).map_or(Profile::default(), |p| p.clone())
    };

    let mut config = Configuration::from(profile);

    if args.profile != DEFAULT_PROFILE_NAME {
        let override_profile = profiles.get(&args.profile).unwrap_or_else(|| panic!("Profile '{}' does not exist", args.profile));

        config.name = override_profile.name.clone();
        if let Some(quiet) = override_profile.quiet {
            config.quiet = quiet;
        }

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

        for (column_name, pattern) in &override_profile.column_regexes {
            config.add_column_regex(column_name.clone(), &pattern);
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
    config.sysrefs.extend(args.sysrefs.clone());

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

    fn make_profiles_with_override() -> ProfileSet {
        let mut profiles = ProfileSet::default();
        let p = make_override_profile();
        profiles.insert(p);
        profiles
    }

    fn make_override_profile() -> Profile {
        let mut profile = Profile::blank();
        profile.name = "over".to_string();
        profile.quiet = Some(true);
        profile.add_column("col1");
        profile.add_column("col2");
        profile.add_alternate_column("PID", "ProcessId");
        profile.add_alternate_column("PID", "ProcId");
        profile.add_alternate_column("TID", "ThreadId");
        profile.add_file_pattern("case*.log");
        profile
    }

    /// Checks that all the default columns are in a column collection.
    fn has_default_columns(columns: &[String]) {
        let def = Profile::default();
        for col in &def.columns {
            assert!(columns.contains(col));
        }
    }

    #[test]
    pub fn sets_command_line_arguments_quiet_correctly() {
        let profiles = ProfileSet::default();
        let mut args = Arguments::default();

        args.quiet = Some(true);
        let config = get_config(&profiles, &args);
        assert!(config.quiet);

        args.quiet = Some(false);
        let config = get_config(&profiles, &args);
        assert!(!config.quiet);
    }

    #[test]
    pub fn sets_command_line_arguments_max_message_length_correctly() {
        let profiles = ProfileSet::default();
        let mut args = Arguments::default();

        args.max_message_length = Some(20);
        let config = get_config(&profiles, &args);
        assert_eq!(config.max_message_length, 20);

        args.max_message_length = None;
        let config = get_config(&profiles, &args);
        assert_eq!(config.max_message_length, DEFAULT_MAX_MESSAGE_LENGTH);
    }

    #[test]
    pub fn for_no_default_profile_returns_blank() {
        let profiles = ProfileSet::default();
        let mut args = Arguments::default();
        args.no_default_profile = true;

        let config = get_config(&profiles, &args);
        assert_eq!(config.name, "blank");
        assert!(config.columns.is_empty());
    }

    #[test]
    pub fn override_profile_name_and_quiet_are_set_correctly() {
        let profiles = make_profiles_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&profiles, &args);
        assert_eq!(config.name, "over");
        assert_eq!(config.quiet, true);
    }

    #[test]
    pub fn override_profile_adds_columns() {
        let profiles = make_profiles_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&profiles, &args);

        assert!(config.columns.contains(&"col1".to_string()));
        assert!(config.columns.contains(&"col2".to_string()));
        has_default_columns(&config.columns);
    }

    #[test]
    pub fn override_profile_adds_alternate_columns() {
        let profiles = make_profiles_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&profiles, &args);

        assert!(config.alternate_column_names["PID"].contains(&"ProcessId".to_string()));
        assert!(config.alternate_column_names["PID"].contains(&"ProcId".to_string()));
        assert!(config.alternate_column_names["TID"].contains(&"ThreadId".to_string()));
    }

    #[test]
    pub fn override_profile_adds_file_patterns() {
        let profiles = make_profiles_with_override();
        let mut args = Arguments::default();
        args.profile = "over".to_string();

        let config = get_config(&profiles, &args);

        assert_eq!(config.file_patterns, vec!["case*.log"]);
    }

    #[test]
    pub fn for_no_file_patterns_in_args_or_config_adds_default() {
        let mut p = make_override_profile();
        p.file_patterns.clear();
        let mut profiles = ProfileSet::default();
        profiles.insert(p);

        let mut args = Arguments::default();
        args.profile = "over".to_string();
        args.files.clear();

        let config = get_config(&profiles, &args);

        assert_eq!(config.file_patterns, vec!["*.log"]);
    }
}