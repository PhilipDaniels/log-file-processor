use std::collections::HashMap;
use serde_derive::{Serialize, Deserialize};
use crate::configuration::{DEFAULT_PROFILE_NAME, DEFAULT_MAX_MESSAGE_LENGTH};
use crate::parse_utils::{LOG_DATE, LOG_LEVEL, MESSAGE};

/// Represents a profile as defined in the configuration file.
/// The main difference between this and the final configuration is that
/// virtually everything is optional, allowing an "override the defaults"
/// configuration system.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub quiet: Option<bool>,
    pub max_message_length: Option<usize>,

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

pub fn vec_add_entry(column_name: String, vec: &mut Vec<String>) {
    if !vec_has_entry(&column_name, vec) {
        vec.push(column_name);
    }
}

impl Profile {
    pub fn blank() -> Self {
        Profile {
            name: "blank".to_string(),
            quiet: None,
            max_message_length: None,
            columns: Vec::new(),
            alternate_column_names: HashMap::new(),
            file_patterns: Vec::new(),
            column_regexes: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn has_column(&self, column_name: &str) -> bool {
        vec_has_entry(column_name, &self.columns) ||
            self.alternate_column_names.values().any(|acns| vec_has_entry(column_name, acns))
    }

    pub fn add_column(&mut self, column_name: String) {
        vec_add_entry(column_name, &mut self.columns);
    }

    pub fn add_alternate_column(&mut self, main_column_name: &str, alternate_column_name: String) {
        let alternate_names = self.alternate_column_names.entry(main_column_name.to_string()).or_default();
        vec_add_entry(alternate_column_name, alternate_names);
    }

    #[cfg(test)]
    pub fn add_file_pattern(&mut self, file_pattern: String) {
        vec_add_entry(file_pattern, &mut self.file_patterns);
    }
}

impl Default for Profile {
    fn default() -> Self {
        let mut p = Self::blank();
        p.name = DEFAULT_PROFILE_NAME.to_string();
        p.quiet = Some(false);
        p.max_message_length = Some(DEFAULT_MAX_MESSAGE_LENGTH);

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

/// The `ProfileSet` is just a hash-map of Profile structs as loaded
/// from the `~/.lpf.json` configuration file.
#[derive(Serialize, Deserialize, Debug)]
pub struct ProfileSet {
    profiles: HashMap<String, Profile>
}

impl Default for ProfileSet {
    fn default() -> Self {
        let mut profiles = ProfileSet {
            profiles: HashMap::new(),
        };

        profiles.insert(Profile::default());
        profiles
    }
}

impl ProfileSet {
    pub fn insert(&mut self, profile: Profile) {
        self.profiles.insert(profile.name.clone(), profile);
    }

    pub fn get(&self, profile_name: &str) -> Option<&Profile> {
        self.profiles.get(profile_name)
    }
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
        let mut p = Profile::blank();
        p.add_column("alpha".to_string());
        assert!(p.has_column("alpha"));
    }

    #[test]
    pub fn has_column_for_matching_alternate_column() {
        let mut p = Profile::blank();
        p.alternate_column_names.insert("alpha".to_string(), vec!["beta".to_string()]);
        assert!(p.has_column("beta"));
    }

    #[test]
    pub fn add_column_for_column_that_exists() {
        let mut p = Profile::blank();
        p.add_column("alpha".to_string());
        p.add_column("alpha".to_string());
        assert_eq!(p.columns, vec!["alpha".to_string()]);
    }

    #[test]
    pub fn add_column_for_column_that_does_not_exist() {
        let mut p = Profile::blank();
        p.add_column("alpha".to_string());
        p.add_column("beta".to_string());
        assert_eq!(p.columns, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_not_present_adds() {
        let mut p = Profile::blank();
        p.add_alternate_column("main", "alpha".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_present_and_column_not_present_adds() {
        let mut p = Profile::blank();
        p.add_alternate_column("main", "alpha".to_string());
        p.add_alternate_column("main", "beta".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    pub fn add_alternate_column_for_key_present_and_column_present_does_not_add() {
        let mut p = Profile::blank();
        p.add_alternate_column("main", "alpha".to_string());
        p.add_alternate_column("main", "alpha".to_string());
        assert_eq!(p.alternate_column_names.len(), 1);
        assert_eq!(p.alternate_column_names["main"], vec!["alpha".to_string()]);
    }
}
