/// This module is responsible for preparing an output record from a ParsedLine.
use regex::Captures;

use crate::configuration::Configuration;
//use crate::parse_utils;

// pub fn make_output_record(config: &Configuration, parsed_line: &ParsedLine) -> Vec<String> {
//     let mut data = Vec::new();

//     for column in &config.columns {
//         match column.as_str() {
//             parse_utils::LOG_DATE => data.push(parsed_line.log_date.to_string()),
//             parse_utils::LOG_LEVEL => data.push(parsed_line.log_level.to_string()),
//             parse_utils::MESSAGE => data.push(parsed_line.message.to_string()),
//             _ => data.push(get_column(config, parsed_line, &column)),
//         }
//     }

//     data
// }

// fn get_column(config: &Configuration, parsed_line: &ParsedLine, column: &str) -> String {
//     // Check for the column under its main name.
//     if let Some(kvp) = parsed_line.kvps.get_value(&column) {
//         return kvp.to_string();
//     }

//     // Check for the column under any alternative names.
//     if let Some(alternate_names) = config.alternate_column_names.get(column) {
//         for alt_name in alternate_names {
//             if let Some(kvp) = parsed_line.kvps.get_value(&alt_name) {
//                 return kvp.to_string();
//             }
//         }
//     }

//     try_extract_from_message(config, parsed_line, column)
// }

// /// Look for a column (as a KVP in the message). It may be embedded somewhere in the middle
// /// of the message. All columns have associated regexes pre-calculated, even standard KVP ones.
// fn try_extract_from_message<'p>(config: &Configuration, parsed_line: &'p ParsedLine, column: &str) -> String {
//     if let Some(regex) = config.column_regexes.get(column) {
//         if let Some(captures) = regex.captures(&parsed_line.line) {
//             let value = extract_kvp_value(captures);
//             let value = parse_utils::safe_string(value);
//             return value.trim().to_string();
//         }
//     }

//     "".to_string()
// }

// fn extract_kvp_value<'t>(captures: Captures<'t>) -> &'t str {
//     let first_valid_sub_match = captures.iter().skip(1).skip_while(|c| c.is_none()).nth(0).unwrap();
//     match first_valid_sub_match {
//         Some(m) => return m.as_str(),
//         None => return ""
//     }
// }

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