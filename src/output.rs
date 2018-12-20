/// This module is responsible for preparing an output record from a ParsedLine.
use unicase::UniCase;
use std::borrow::Cow;

use crate::config::{self, Config};
use crate::inputs::Column;
use crate::parsed_line::ParsedLine;

/*
pub fn make_output_record<'p, 't, 'c>(parsed_line: &'p ParsedLine<'t>, columns: &'c [Column]) -> Vec<&'t str> {
    let mut data = Vec::new();
    
    for column in columns {
        match column.name.as_str() {
            config::LOG_DATE => data.push(parsed_line.log_date),
            config::LOG_LEVEL => data.push(parsed_line.log_level),
            config::MESSAGE => data.push(&parsed_line.message),

            _ => {
                let ci_comparer = UniCase::new(column.name.as_str());
                match parsed_line.kvps.get(&ci_comparer) {
                    // This is the problem here. To make it explicit:
                    //     val is a "&'t Cow<'t, str>" and x is "&'t str"
                    Some(val) => {
                        let x = val.as_ref();
                        data.push(x);
                    },
                    None => data.push(""),
                }
            },
        }
    }

    data
}
*/


    
//return Vec::new();


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
