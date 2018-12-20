/// This module is responsible for preparing an output record from a ParsedLine.

use crate::config::{self, Config};
use crate::inputs::Column;
use crate::parsed_line::ParsedLine;
use crate::parse_utils;

pub fn make_output_record<'p>(parsed_line: &'p ParsedLine, columns: &'p [Column]) -> Vec<&'p str> {
    let mut data = Vec::new();
    
    for column in columns {
        match column.name.as_str() {
            parse_utils::LOG_DATE => data.push(parsed_line.log_date),
            parse_utils::LOG_LEVEL => data.push(parsed_line.log_level),
            parse_utils::MESSAGE => data.push(&parsed_line.message),

            _ => {
                match parsed_line.kvps.get_value(&column.name) {
                    Some(val) => data.push(val),
                    None => data.push(""),
                }
            },
        }
    }

    data
}


/* OLD CODE.
// let captures = column.regex.captures(&line);
// if captures.is_none() {
//     data.push("".to_string());
//     continue;
// }
// let captures = captures.unwrap();

// let text = if is_date_column(&column.name) {
//     let capture_names = column.regex.capture_names().collect::<Vec<_>>();
//     cleanup_slice(&extract_date(captures, &capture_names)).to_string()
// } else {
//     cleanup_slice(extract_kvp(captures)).to_string()
// };
*/
