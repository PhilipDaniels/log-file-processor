use crate::byte_extensions::{ByteExtensions, ByteSliceExtensions};

#[derive(Debug, Default)]
pub struct ParsedLineError<'f> {
    /// The zero-based line number.
    pub line_num: usize,

    /// The entire original line with whitespace trimmed from the ends.
    pub line: &'f [u8],

    /// A message describing the error.
    pub message: String
}

#[derive(Debug, Default)]
pub struct ParsedLine2<'f> {
    /// The zero-based line number.
    pub line_num: usize,

    /// The entire original line with whitespace trimmed from the ends.
    pub line: &'f [u8],
    pub log_date: &'f [u8],
    // pub log_level: String,
    // pub message: String,
    // pub kvps: KVPCollection
}

/// The result of parsing a line is one of these types.
pub type ParseLineResult<'f> = Result<ParsedLine2<'f>, ParsedLineError<'f>>;


impl<'f> ParsedLine2<'f> {
    const LENGTH_OF_LOGGING_TIMESTAMP: usize = 27;

    /// Parses a line, returning a struct with all the individual pieces of information.
    pub fn new(line_num: usize, line: &[u8]) -> ParseLineResult {
        let mut line = line.trim_while(ByteExtensions::is_whitespace);
        if line.is_empty() {
            return Err(ParsedLineError{ line_num, line, message: "Line is empty".to_string()});
        }

        let mut parsed_line = ParsedLine2::default();
        parsed_line.line = line;

        // Extract the log date, splitting the line into two slices - the log date and the remainder.
        match ParsedLine2::extract_log_date_fast(&line) {
            Ok((log_date_slice, remainder)) => {
                parsed_line.log_date = log_date_slice;
                line = remainder;
            },
            Err(message) => return Err(ParsedLineError{ line_num, line, message })
        }

        // Now, in the remainder of the line (if there is any), extract KVPs/prologue items until we reach the message.
        // First skip to the usual beginning of the first item in the prologue.
        let line = line.trim_left_while(ByteExtensions::is_whitespace_or_pipe);
        if line.is_empty() { return Ok(parsed_line); }



        Ok(parsed_line)
    }

    fn extract_log_date_fast(line: &[u8]) -> Result<(&[u8],&[u8]), String> {
        if line.len() < ParsedLine2::LENGTH_OF_LOGGING_TIMESTAMP {
            let msg = format!("The input line is less than {} characters, which indicates it does not even contain a logging timestamp", ParsedLine2::LENGTH_OF_LOGGING_TIMESTAMP);
            Err(msg)
        } else {
            Ok(line.split_at(ParsedLine2::LENGTH_OF_LOGGING_TIMESTAMP))
        }
    }

    /// Extracts the log date from the message. We expect this to occur at the beginning of the message
    /// and to have a specific number of characters.
    fn extract_log_date(line: &[u8]) -> Result<(&[u8],&[u8]), String> {
        if line.len() < ParsedLine2::LENGTH_OF_LOGGING_TIMESTAMP {
            let msg = format!("The input line is less than {} characters, which indicates it does not even contain a logging timestamp", ParsedLine2::LENGTH_OF_LOGGING_TIMESTAMP);
            return Err(msg);
        }

        // The numbers.
        for idx in vec![0,1,2,3,5,6,8,9,11,12,14,15,17,18,20,21,22,23,24,25,26] {
            if !line[idx].is_decimal_digit() {
                let msg = format!("Character {} was expected to be a decimal digit, but was {}", idx, line[idx] as char);
                return Err(msg);
            }
        }

        // The separators in the date component.
        for idx in vec![4,7] {
            if line[idx] != b'-' {
                let msg = format!("Character {} was expected to be '-', but was {}", idx, line[idx] as char);
                return Err(msg);
            }
        }

        // The separators in the time component.
        for idx in vec![13,16] {
            if line[idx] != b':' {
                let msg = format!("Character {} was expected to be '-', but was {}", idx, line[idx] as char);
                return Err(msg);
            }
        }

        // YYYY-MM-DD_
        if line[10] != b' ' {
            let msg = format!("Character {} was expected to be ' ', but was {}", 10, line[10] as char);
            return Err(msg);
        }

        // YYYY-MM-DD_HH:MM:SS.
        if line[19] != b'.' {
            let msg = format!("Character {} was expected to be '.', but was {}", 19, line[19] as char);
            return Err(msg);
        }

        // For reference: the code from the old date parsing function.
        // // YYYY-MM-DD_HH:MM:SS.FFFFFFF
        // // Consume all remaining decimal characters. This gives us some flexibility for the future, if it ever becomes possible to have
        // // more precision in the logging timestamps. Theoretically, this code allows one of the F to be a non-digit, but
        // // I am willing to live with that, it never happens in real life.
        // // Leave fraction_end on the last decimal digit.
        // let mut fraction_end = current + 1;
        // while fraction_end <= limit && chars[fraction_end].1.is_digit(10) {
        //     fraction_end += 1;
        // }
        // fraction_end -=1;

        // if fraction_end == current {
        //     Err(LineParseError::BadLogDate("No fractional seconds part was detected on the log_date".to_string()))
        // } else {
        //     Ok(fraction_end)
        // }

        Ok(line.split_at(ParsedLine2::LENGTH_OF_LOGGING_TIMESTAMP))
    }
}

#[cfg(test)]
mod white_space_tests {
    use super::*;

    #[test]
    fn blank_line_returns_error() {
        let result = ParsedLine2::new(0, b"");
        match result {
            Err(ref e) if e.message == "Line is empty" => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn whitespace_line_returns_error() {
        let result = ParsedLine2::new(0, b"  \r  ");
        match result {
            Err(ref e) if e.message == "Line is empty" => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn trims_whitespace_from_both_ends() {
        let result = ParsedLine2::new(0, b"  \r\n 2018-09-26 12:34:56.1146655 | MachineName=Some.machine.net | Message\r\n   ")
            .expect("Parse should succeed");
        assert_eq!(result.line.to_vec(), b"2018-09-26 12:34:56.1146655 | MachineName=Some.machine.net | Message".to_vec());
    }
}

#[cfg(test)]
mod log_date_tests {
    use super::*;

    #[test]
    fn short_line_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-12");
        match result {
            Err(ref msg) if msg.contains("logging timestamp") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y1_returns_error() {
        let result = ParsedLine2::extract_log_date(b"x018-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 0") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y2_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2x18-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 1") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y3_returns_error() {
        let result = ParsedLine2::extract_log_date(b"20x8-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 2") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y4_returns_error() {
        let result = ParsedLine2::extract_log_date(b"201x-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 3") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep1_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018x09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 4") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_mon1_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-x9-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 5") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_mon2_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-0x-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 6") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep2_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09x26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 7") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_d1_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-x6 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 8") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_d2_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-2x 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 9") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep3_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26x23:00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 10") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_h1_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 x3:00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 11") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_h2_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 2x:00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 12") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep4_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 23x00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 13") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_min1_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 23:x0:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 14") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_min2_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 23:0x:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 15") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep5_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 23:00x00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 16") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_s1_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 23:00:x0.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 17") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_s2_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 23:00:0x.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 18") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep6_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 23:00:00x7654321");
        match result {
            Err(ref msg) if msg.contains("Character 19") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_no_fractions_returns_error() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 12:34:56. | some mesage to make the line longer enough");
        match result {
            Err(ref msg) if msg.contains("Character 20") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_only_log_date_extracts_log_date() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 12:34:56.1146655").expect("Parse should succeed");
        assert_eq!(result.0, b"2018-09-26 12:34:56.1146655");
    }

    #[test]
    fn with_only_log_date_and_whitespace_extracts_log_date() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 12:34:56.1146655 ").expect("Parse should succeed");
        assert_eq!(result.0, b"2018-09-26 12:34:56.1146655");
    }

    #[test]
    fn with_nominal_line_extracts_log_date() {
        let result = ParsedLine2::extract_log_date(b"2018-09-26 12:34:56.1146655 | MachineName=foo | Message").expect("Parse should succeed");
        assert_eq!(result.0, b"2018-09-26 12:34:56.1146655");
    }

    // These were supported under the old parser, but not the new one.
    // #[test]
    // fn with_longer_precision_extracts_log_date() {
    //     let result = ParsedLine2::extract_log_date(b"2018-09-26 12:34:56.12345678901 | MachineName=foo | Message").expect("Parse should succeed");
    //     assert_eq!(result.0, b"2018-09-26 12:34:56.12345678901");
    // }

    // #[test]
    // fn with_shorter_precision_extracts_log_date() {
    //     let result = ParsedLine2::extract_log_date(b"2018-09-26 12:34:56.1234 | MachineName=foo | Message").expect("Parse should succeed");
    //     assert_eq!(result.0, b"2018-09-26 12:34:56.1234");
    // }
}
