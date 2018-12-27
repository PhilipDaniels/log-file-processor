/// This module is responsible for preparing an output record from a ParsedLine.
use crate::configuration::Configuration;
use crate::inputs::{is_date_column};
use crate::parsed_line::ParsedLine;
use crate::parse_utils;
use regex::{Captures};

pub fn make_output_record<'p>(config: &Configuration, parsed_line: &'p ParsedLine) -> Vec<String> {
    let mut data = Vec::new();
    
    for column in &config.columns {
        match column.as_str() {
            parse_utils::LOG_DATE => data.push(parsed_line.log_date.to_string()),
            parse_utils::LOG_LEVEL => data.push(parsed_line.log_level.to_string()),
            parse_utils::MESSAGE => data.push(parsed_line.message.to_string()),
            _ => data.push(get_column(config, parsed_line, &column)),
        }
    }

    data
}

fn get_column(config: &Configuration, parsed_line: &ParsedLine, column: &str) -> String {
    // Check for the column under its main name.
    if let Some(kvp) = parsed_line.kvps.get_value(&column) {
        return kvp.to_string();
    }

    // Check for the column under any alternative names.
    if let Some(alternate_names) = config.alternate_column_names.get(column) {
        for alt_name in alternate_names {
            if let Some(kvp) = parsed_line.kvps.get_value(&alt_name) {
                return kvp.to_string();
            }
        }
    }

    try_extract_from_message(config, parsed_line, &column)
}

/// Look for a column (as a KVP in the message). It may be embedded somewhere in the middle
/// of the message. All columns have associated regexes pre-calculated, even standard KVP ones.
fn try_extract_from_message<'p>(config: &Configuration, parsed_line: &'p ParsedLine, column: &str) -> String {
    // let captures = column.regex.captures(parsed_line.line);
    // if captures.is_none() {
    //     return "".to_string();
    // }
    // let captures = captures.unwrap();

    // let mut text = if is_date_column(&column.name) {
    //     let capture_names = column.regex.capture_names().collect::<Vec<_>>();
    //     extract_date(captures, &capture_names)
    // } else {
    //     extract_kvp(captures).to_string()
    // };

    // text = text.replace(|c| c == '\r' || c == '\n', " ");
    // text.trim().to_string()

    "".to_string()
}

fn extract_date(captures: Captures, capture_names: &[Option<&str>]) -> String {
    // Typical values for capture_names are:
    //      KVP regex : [None, None, None, None, None]
    //      Date regex: [None, Some("year"), Some("month"), Some("day"), Some("hour"), Some("minutes"), Some("seconds"), Some("fractions"), Some("year2"), Some("month2"), Some("day2")]

    // We consider the following combinations to be valid extractions.
    //      (year, month, day)
    //      (year, month, day, hour, minutes, seconds)
    //      (year, month, day, hour, minutes, seconds, fractions)
    // Anything else we consider to be a bad match.

    let year = extract_date_part("year", &captures, capture_names);
    if year.is_empty() { return "".to_string() };

    let month = extract_date_part("month", &captures, capture_names);
    if month.is_empty() { return "".to_string() };

    let day = extract_date_part("day", &captures, capture_names);
    if day.is_empty() { return "".to_string() };

    let hour = extract_date_part("hour", &captures, capture_names);
    if hour.is_empty() {
        return format!("{}-{}-{}", year, month, day);
    };

    let minutes = extract_date_part("minutes", &captures, capture_names);
    if minutes.is_empty() {
        return format!("{}-{}-{}", year, month, day);
    };

    let seconds = extract_date_part("seconds", &captures, capture_names);
    if seconds.is_empty() {
        return format!("{}-{}-{}", year, month, day);
    };

    let fractions = extract_date_part("fractions", &captures, capture_names);
    if fractions.is_empty() {
        format!("{}-{}-{} {}:{}:{}", year, month, day, hour, minutes, seconds)
    } else {
        format!("{}-{}-{} {}:{}:{}.{}", year, month, day, hour, minutes, seconds, fractions)
    }
}

fn extract_date_part<'t>(part: &str, captures: &'t Captures, capture_names: &[Option<&str>]) -> &'t str {
    for name in capture_names {
        if name.is_none() {
            continue;
        }

        let match_name = name.as_ref().unwrap();
        if match_name.starts_with(part) {
            let the_match = captures.name(match_name);
            match the_match {
                Some(m) => return m.as_str(),
                None => panic!("Because we have a match name, this should never be called.")
            }
        }
    }

    ""
}

fn extract_kvp<'t>(captures: Captures<'t>) -> &'t str {
    let first_valid_sub_match = captures.iter().skip(1).skip_while(|c| c.is_none()).nth(0).unwrap();
    match first_valid_sub_match {
        Some(m) => return m.as_str(),
        None => return ""
    }
}

/*
#[cfg(test)]
mod make_output_record_extract_kvp_from_message_tests {
    use super::*;
    use crate::regexes;

    fn testing_columns() -> Vec<Column> {
        vec![
            Column { name: "Foo".to_string(), regex: regexes::make_regex_for_column("Foo") }
        ]
    }

    #[test]
    pub fn when_kvp_exists_in_message() {
        let parsed_line = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=10 | Message Foo=Bar some words  SysRef=1").expect("Parse should succeed");
        let columns = testing_columns();
        let data = make_output_record(&parsed_line, &columns);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], "Bar");
    }

    #[test]
    pub fn when_kvp_exists_in_message_with_double_quotes() {
        let parsed_line = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=10 | Message Foo=\"Bar some\" words  SysRef=1").expect("Parse should succeed");
        let columns = testing_columns();
        let data = make_output_record(&parsed_line, &columns);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], "Bar some");
    }

    #[test]
    pub fn when_kvp_exists_in_message_with_empty_value() {
        let parsed_line = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=10 | Message Foo= some words  SysRef=1").expect("Parse should succeed");
        let columns = testing_columns();
        let data = make_output_record(&parsed_line, &columns);
        assert_eq!(data.len(), 1, "Because we push an empty string if no match is found");
        assert_eq!(data[0], "");
    }

    #[test]
    pub fn when_kvp_exists_in_message_and_trailing_kvps() {
        let parsed_line = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=10 | Message Foo=Bar some words  SysRef=1 Foo=Canada").expect("Parse should succeed");
        let columns = testing_columns();
        let data = make_output_record(&parsed_line, &columns);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], "Canada", "Trailing KVPs should have priority");
    }

    #[test]
    pub fn when_kvp_does_not_exist_in_message() {
        let parsed_line = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=10 | Message Kibble=Bar some words  SysRef=1").expect("Parse should succeed");
        let columns = testing_columns();
        let data = make_output_record(&parsed_line, &columns);
        assert_eq!(data.len(), 1, "Because we push an empty string if no match is found");
        assert_eq!(data[0], "");
    }
}
*/