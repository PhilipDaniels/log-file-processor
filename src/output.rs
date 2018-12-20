/// This module is responsible for preparing an output record from a ParsedLine.

use std::borrow::Cow;
use crate::config::{self, Config};
use crate::inputs::{Column, is_date_column};
use crate::parsed_line::ParsedLine;
use crate::parse_utils;
use regex::{Captures};

pub fn make_output_record<'p>(parsed_line: &'p ParsedLine, columns: &'p [Column]) -> Vec<String> {
    let mut data = Vec::new();
    
    for column in columns {
        match column.name.as_str() {
            parse_utils::LOG_DATE => data.push(parsed_line.log_date.to_string()),
            parse_utils::LOG_LEVEL => data.push(parsed_line.log_level.to_string()),
            parse_utils::MESSAGE => data.push(parsed_line.message.to_string()),

            _ => {
                match parsed_line.kvps.get_value(&column.name) {
                    Some(val) => data.push(val.to_string()),
                    None => data.push(try_extract_from_message(parsed_line, &column)),
                }
            },
        }
    }

    data
}

/// Look for a column (as a KVP in the message). It may be embedded somewhere in the middle
/// of the message. All columns have associated regexes pre-calculated, even standard KVP ones.
fn try_extract_from_message<'p>(parsed_line: &'p ParsedLine, column: &'p Column) -> String {
    let captures = column.regex.captures(parsed_line.line);
    if captures.is_none() {
        return "".to_string();
    }
    let captures = captures.unwrap();

    let text = if is_date_column(&column.name) {
        let capture_names = column.regex.capture_names().collect::<Vec<_>>();
        cleanup_slice(&extract_date(captures, &capture_names)).to_string()
    } else {
        cleanup_slice(extract_kvp(captures)).to_string()
    };


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

// Cleanup the text. Doing this here keeps regexes simpler.
// The '.' deals with people using full-stops in log messages.
fn cleanup_slice(text: &str) -> &str {
    text.trim_matches(|c| c == '.' || char::is_whitespace(c))
}
