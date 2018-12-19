/// Makes a regex that extracts key-value pairs of the form
///    Key=Value                            or
///    Key="Some value in double quotes"
pub fn make_kvp_pattern(key_name: &str) -> String {
    // or re = Regex::new(r"'([^']+)'\s+\((\d{4})\)").unwrap();
    format!(r###"\W{0}="(.*?)"|\W{0}=(\S*)"###, regex::escape(key_name))
}

/// Makes a regex similar to `make_kvp_pattern`, but that allows an alternate name
/// for the key.
pub fn make_kvp_pattern_with_alternate_key(key_name: &str, alternate_key_name: &str) -> String {
    let mut s = make_kvp_pattern(key_name);
    s.push('|');
    s += &make_kvp_pattern(alternate_key_name);
    s
}

/// A regex pattern that can be used to capture the log date timestamp and similar
/// values that are in the form `YYYY-MM-DD HH:MM:SS.fffff...`.
/// ExpiryDate = 2018-12-03T15:10:04.1114295Z
/// 0001-01-01T00:00:00.0000000

const STANDARD_DATE_PATTERN: &str = r###"(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2}) (?P<hour>\d{2}):(?P<minutes>\d{2}):(?P<seconds>\d{2})\.+(?P<fractions>\d+)"###;
const YMD_DASH_PATTERN: &str = r###"(?P<year2>\d{4})-(?P<month2>\d{2})-(?P<day2>\d{2})"###;

pub fn make_date_pattern() -> String {
    format!("{}|{}", STANDARD_DATE_PATTERN, YMD_DASH_PATTERN)
}

pub fn make_log_date_pattern() -> String {
    format!("^{}|^{}", STANDARD_DATE_PATTERN, YMD_DASH_PATTERN)
}