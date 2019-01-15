use structopt::StructOpt;

/// Represents command-line arguments.
#[derive(StructOpt, Debug)]
pub struct Arguments {
    /// The name of the profile to read from the configuration file.
    /// Profiles are additive - first the default profile is applied, then this profile,
    /// if any, is applied on top to produce the effective profile. This keeps most custom
    /// defined profiles very short. To completely suppress the default profile, use
    /// the `no-default-profile` flag.
    #[structopt(short = "p", long = "profile", default_value = "default")]
    pub profile: String,

    /// Suppresses loading of the default profile, meaning that the profile you
    /// name will be the only one applied.
    #[structopt(short = "D", long = "no-default-profile")]
    pub no_default_profile: bool,

    /// If true, run quietly, without any progress bars.
    #[structopt(short = "q", long = "quiet")]
    pub quiet: Option<bool>,

    /// Specifies the maximum length of the message component when written to the output.
    /// Some log lines are extremely long and can generate warnings in LibreOffice or Excel,
    /// this allows them to be trimmed down to something more reasonable.
    #[structopt(short = "m", long = "max-message-length")]
    pub max_message_length: Option<usize>,

    /// If true, dumps an example configuration file, based on the default configuration,
    /// to stdout.
    #[structopt(short = "d", long = "dump-config")]
    pub dump_config: bool,

    /// Optional list of sysrefs to filter by. Separate them by commas.
    #[structopt(short = "s", long = "sysrefs", use_delimiter = true)]
    pub sysrefs: Vec<String>,

    /// List of files to process. Defaults to "*.log".
    #[structopt(name = "FILE")]
    pub files: Vec<String>,
}

#[cfg(test)]
impl Default for Arguments {
    fn default() -> Self {
        use crate::configuration::DEFAULT_PROFILE_NAME;

        Arguments {
            profile: DEFAULT_PROFILE_NAME.to_string(),
            no_default_profile: false,
            quiet: None,
            max_message_length: None,
            dump_config: false,
            sysrefs: vec![],
            files: vec![],
        }
    }
}
