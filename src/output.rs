/// This module is responsible for preparing an output record from a ParsedLine.

use unicase::UniCase;

use crate::config::{self, Config};
use crate::inputs::Column;
use crate::parsed_line::ParsedLine;

pub fn make_output_record<'p>(parsed_line: &'p ParsedLine, columns: &'p [Column]) -> Vec<&'p str> {
    let mut data = Vec::new();
    
    for column in columns {
        match column.name.as_str() {
            config::LOG_DATE => data.push(parsed_line.log_date),
            config::LOG_LEVEL => data.push(parsed_line.log_level),
            config::MESSAGE => data.push(&parsed_line.message),

            _ => {
                let ci_comparer = UniCase::new(column.name.as_str());
                match parsed_line.kvps.get(&ci_comparer) {
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
