use std::borrow::Cow;
use crate::byte_extensions::{ByteExtensions, ByteSliceExtensions};
use crate::kvp::{KVPCollection, ByteSliceKvpExtensions};

/*
Notes
=====
- The framework replaces any \r\n in messages with \n
  Therefore a message is always terminated by a \r\n - 0D 0A - pair.

Patterns known to the logging framework:
    var replacements = new Dictionary<string, string>
    {
        {"timestamp", DateTime.UtcNow.AsLoggingTimeStamp() },
        {"processId", Logger.GetCurrentProcessID().ToString(CultureInfo.InvariantCulture)},
        {"threadId", Logger.GetCurrentManagedThreadId().ToString(CultureInfo.InvariantCulture)},
        {"level", Logger.LevelDescriptor(level)},
        {"message", parameters != null ? string.Format(format, parameters) : format},
        {"tags", tags.ToString()},
        {"correlationKey", this.CorrelationKey},
        {"eventId", this.EventId.ToString(CultureInfo.InvariantCulture)},
        {"appName", ApplicationName },
        {"machineName", MachineName }
    };

This function is used to quote LoggingContext data (but only in the context.)
    public static string QuoteForLogging(this string source)
    {
        if (source == null)
        {
            return RepresentationOfNull;
        }

        if (source.Contains(" ") || source.Contains("\""))
        {
            source = source.Replace('"', '_');
            return '"' + source + '"';
        }
        else
        {
            return source;
        }
    }


Some message formats
    messageFormat="{timestamp} | MachineName={machineName} | AppName={appName} | pid={processId} | tid={threadId} | {level} | {message}"  (Case Service)
    messageFormat="{timestamp} | MachineName={machineName} | ApplicationName={appName} | pid={processId} | tid={threadId} | {level} | {message}"  (Case Service)
    messageFormat="{timestamp} | AppName={appName} | pid={processId} | tid={threadId} | {level} | {message}"  (Case Service)
*/

#[derive(Debug, Default)]
pub struct ParsedLineError<'f> {
    // It makes sorting easier if we also include a reference to the original file or HTTP source.
    pub source: &'f str,
    pub line_num: usize,

    /// The entire original line with whitespace trimmed from the ends.
    pub line: &'f [u8],

    /// A message describing the error.
    pub message: String,
}

impl<'f> ParsedLineError<'f> {
    pub fn new(message: &str, line: &'f [u8]) -> Self {
        ParsedLineError {
            message: message.to_string(),
            line,
            .. ParsedLineError::default()
        }
    }
}

/// Represents the successful parse of a log line into a more convenient structure.
/// Consists of lots of slices into the original file's byte array
/// The lifetime `f` means the original file's lifetime, or more particularly
/// the lifetime of its bytes.
#[derive(Debug, Default)]
pub struct ParsedLine<'f> {
    // It makes sorting easier if we also include a reference to the original file or HTTP source.
    pub log_date: &'f [u8],
    pub source: &'f str,
    pub line_num: usize,

    /// The entire original line with whitespace trimmed from the ends.
    pub line: &'f [u8],

    pub log_level: &'f [u8],
    pub kvps: KVPCollection<'f>,
    pub message: Cow<'f, [u8]>,
}

/// The result of parsing a line is one of these types.
pub type ParseLineResult<'f> = Result<ParsedLine<'f>, ParsedLineError<'f>>;

impl<'f> ParsedLine<'f> {
    const LENGTH_OF_LOGGING_TIMESTAMP: usize = 27;

    /// Parses a line, returning a struct with all the individual pieces of information.
    pub fn parse(line: &[u8]) -> ParseLineResult {
        let mut line = line.trim_while(ByteExtensions::is_whitespace);
        if line.is_empty() {
            return Err(ParsedLineError::new("Line is empty", line));
        }

        let mut parsed_line = ParsedLine { line, .. ParsedLine::default() };

        // Extract the log date, splitting the line into two slices - the log date and the remainder.
        match ParsedLine::extract_log_date(&line) {
            Ok((log_date_slice, remainder)) => {
                parsed_line.log_date = log_date_slice;
                line = remainder;
            },
            Err(message) => return Err(ParsedLineError::new(&message, line))
        }

        // Now, in the remainder of the line (if there is any), extract KVPs/prologue items until we reach the message.
        // First skip to the usual beginning of the first item in the prologue.
        let mut line = line.trim_left_while(ByteExtensions::is_whitespace_or_pipe);
        if line.is_empty() { return Ok(parsed_line); }

        loop {
            let kvp_parse_result = line.next_kvp();
            line = kvp_parse_result.remaining_slice.trim_left_while(ByteExtensions::is_whitespace_or_pipe);
            if let Some(kvp) = kvp_parse_result.kvp {
                if kvp.is_log_level {
                    parsed_line.log_level = kvp.key;
                } else {
                    parsed_line.kvps.insert(kvp);
                }
            } else {
                break;
            }
        }

        // If there is nothing left (unlikely, means there was no message), we are done.
        let mut line = line.trim_while(ByteExtensions::is_whitespace);
        if line.is_empty() { return Ok(parsed_line); }

        // Store the entire remainder of the line as the message. This includes trailing KVPs.
        // This is important, as without it, context can be lost. For example, if a KVP is not
        // named as a column and we do this after trimming the trailing KVPs we will never see
        // what that KVP's value was. This may hinder debugging.
        parsed_line.message = line.make_safe();

        // Now find trailing KVPs. There are usually more of these than leading ones.
        loop {
            let kvp_parse_result = line.prev_kvp();
            line = kvp_parse_result.remaining_slice.trim_right_while(ByteExtensions::is_whitespace);
            if let Some(kvp) = kvp_parse_result.kvp {
                parsed_line.kvps.insert(kvp);
            } else {
                break;
            }
        }

        Ok(parsed_line)
    }

    /*
    fn extract_log_date_fast(line: &[u8]) -> Result<(&[u8],&[u8]), String> {
        if line.len() < ParsedLine::LENGTH_OF_LOGGING_TIMESTAMP {
            let msg = format!("The input line is less than {} characters, which indicates it does not even contain a logging timestamp", ParsedLine::LENGTH_OF_LOGGING_TIMESTAMP);
            Err(msg)
        } else {
            Ok(line.split_at(ParsedLine::LENGTH_OF_LOGGING_TIMESTAMP))
        }
    }
    */

    /// Extracts the log date from the message. We expect this to occur at the beginning of the message
    /// and to have a specific number of characters.
    fn extract_log_date(line: &[u8]) -> Result<(&[u8],&[u8]), String> {
        if line.len() < ParsedLine::LENGTH_OF_LOGGING_TIMESTAMP {
            let msg = format!("The input line is less than {} characters, which indicates it does not even contain a logging timestamp", ParsedLine::LENGTH_OF_LOGGING_TIMESTAMP);
            return Err(msg);
        }

        // The numbers.
        const DECIMAL_INDEXES: [usize; 21] = [0,1,2,3,5,6,8,9,11,12,14,15,17,18,20,21,22,23,24,25,26];
        for &idx in &DECIMAL_INDEXES {
            if !line[idx].is_decimal_digit() {
                let msg = format!("Character {} was expected to be a decimal digit, but was '{}'", idx, line[idx] as char);
                return Err(msg);
            }
        }

        // The separators in the date component.
        const DATE_SEP_INDEXES: [usize; 2] = [4,7];
        for &idx in &DATE_SEP_INDEXES {
            if line[idx] != b'-' {
                let msg = format!("Character {} was expected to be '-', but was '{}'", idx, line[idx] as char);
                return Err(msg);
            }
        }

        // The separators in the time component.
        const TIME_SEP_INDEXES: [usize; 2] = [13,16];
        for &idx in &TIME_SEP_INDEXES {
            if line[idx] != b':' {
                let msg = format!("Character {} was expected to be '-', but was '{}'", idx, line[idx] as char);
                return Err(msg);
            }
        }

        // YYYY-MM-DD_
        if line[10] != b' ' {
            let msg = format!("Character {} was expected to be ' ', but was '{}'", 10, line[10] as char);
            return Err(msg);
        }

        // YYYY-MM-DD_HH:MM:SS.
        if line[19] != b'.' {
            let msg = format!("Character {} was expected to be '.', but was '{}'", 19, line[19] as char);
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

        Ok(line.split_at(ParsedLine::LENGTH_OF_LOGGING_TIMESTAMP))
    }
}

#[cfg(test)]
mod white_space_tests {
    use super::*;

    #[test]
    fn blank_line_returns_error() {
        let result = ParsedLine::parse(b"");
        match result {
            Err(ref e) if e.message == "Line is empty" => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn whitespace_line_returns_error() {
        let result = ParsedLine::parse(b"  \r  ");
        match result {
            Err(ref e) if e.message == "Line is empty" => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn trims_whitespace_from_both_ends() {
        let result = ParsedLine::parse(b"  \r\n 2018-09-26 12:34:56.1146655 | MachineName=Some.machine.net | Message\r\n   ")
            .expect("Parse should succeed");
        assert_eq!(result.line.to_vec(), b"2018-09-26 12:34:56.1146655 | MachineName=Some.machine.net | Message".to_vec());
    }
}

#[cfg(test)]
mod extract_log_date_tests {
    use super::*;

    #[test]
    fn short_line_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-12");
        match result {
            Err(ref msg) if msg.contains("logging timestamp") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y1_returns_error() {
        let result = ParsedLine::extract_log_date(b"x018-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 0") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y2_returns_error() {
        let result = ParsedLine::extract_log_date(b"2x18-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 1") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y3_returns_error() {
        let result = ParsedLine::extract_log_date(b"20x8-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 2") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y4_returns_error() {
        let result = ParsedLine::extract_log_date(b"201x-09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 3") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep1_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018x09-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 4") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_mon1_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-x9-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 5") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_mon2_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-0x-26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 6") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep2_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09x26 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 7") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_d1_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-x6 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 8") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_d2_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-2x 12:34:56.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 9") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep3_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26x23:00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 10") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_h1_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 x3:00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 11") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_h2_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 2x:00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 12") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep4_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 23x00:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 13") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_min1_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 23:x0:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 14") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_min2_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 23:0x:00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 15") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep5_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 23:00x00.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 16") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_s1_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 23:00:x0.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 17") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_s2_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 23:00:0x.7654321");
        match result {
            Err(ref msg) if msg.contains("Character 18") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep6_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 23:00:00x7654321");
        match result {
            Err(ref msg) if msg.contains("Character 19") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_no_fractions_returns_error() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 12:34:56. | some mesage to make the line longer enough");
        match result {
            Err(ref msg) if msg.contains("Character 20") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_only_log_date_extracts_log_date() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 12:34:56.1146655").expect("Parse should succeed");
        assert_eq!(result.0, b"2018-09-26 12:34:56.1146655");
    }

    #[test]
    fn with_only_log_date_and_whitespace_extracts_log_date() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 12:34:56.1146655 ").expect("Parse should succeed");
        assert_eq!(result.0, b"2018-09-26 12:34:56.1146655");
    }

    #[test]
    fn with_nominal_line_extracts_log_date() {
        let result = ParsedLine::extract_log_date(b"2018-09-26 12:34:56.1146655 | MachineName=foo | Message").expect("Parse should succeed");
        assert_eq!(result.0, b"2018-09-26 12:34:56.1146655");
    }

    // These were supported under the old parser, but not the new one.
    // #[test]
    // fn with_longer_precision_extracts_log_date() {
    //     let result = ParsedLine::extract_log_date(b"2018-09-26 12:34:56.12345678901 | MachineName=foo | Message").expect("Parse should succeed");
    //     assert_eq!(result.0, b"2018-09-26 12:34:56.12345678901");
    // }

    // #[test]
    // fn with_shorter_precision_extracts_log_date() {
    //     let result = ParsedLine::extract_log_date(b"2018-09-26 12:34:56.1234 | MachineName=foo | Message").expect("Parse should succeed");
    //     assert_eq!(result.0, b"2018-09-26 12:34:56.1234");
    // }
}

#[cfg(test)]
mod leading_kvps_tests {
    use super::*;
    use crate::kvp;

    #[test]
    pub fn with_empty_prologue_returns_empty_kvps_and_empty_log_level() {
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.7654321").expect("Parse should succeed");
        assert!(result.kvps.is_empty());
        assert!(result.log_level.is_empty())
    }

    #[test]
    pub fn with_prologue_containing_log_level_returns_appropriate_log_level() {
        for log_level in &kvp::LOG_LEVELS {
            let line = format!("2018-09-26 12:34:56.7654321 | a=b | {} | Message", String::from_utf8(log_level.to_vec()).unwrap());
            let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
            assert_eq!(result.log_level, &log_level[0..log_level.len()]);
        }
    }

    #[test]
    pub fn with_prologue_containing_kpvs_returns_kvps() {
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.7654321 | a=b | pid=123 | [INFO_] | Message")
            .expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 2);
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.kvps.value(b"a"), b"b");
        assert_eq!(result.kvps.value(b"PID"), b"123");
    }

    #[test]
    pub fn with_prologue_and_no_message_returns_kvps_and_log_level() {
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.7654321 | a=b | pid=123 | [INFO_]").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 2);
        assert_eq!(result.log_level, b"[INFO_]", "Log level should be extracted");
        assert_eq!(result.kvps.value(b"a"), b"b");
        assert_eq!(result.kvps.value(b"PID"), b"123");
    }

    #[test]
    pub fn with_prologue_and_no_message_and_whitespace_returns_kvps_and_log_level() {
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.7654321 | a=b | pid=123 | [INFO_]  ").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 2);
        assert_eq!(result.log_level, b"[INFO_]", "Log level should be extracted");
        assert_eq!(result.kvps.value(b"a"), b"b");
        assert_eq!(result.kvps.value(b"PID"), b"123");
    }

    #[test]
    pub fn with_prologue_containing_all_well_formed_kpv_types_returns_kvps() {
        let line = b"2018-09-26 12:34:56.7654321 | a=\"Value with space\" | pid=123 | Empty= | [INFO_] | Message\nFoo=Bar SysRef=AA123456 whatever";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 3);
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.kvps.value(b"a"), b"Value with space");
        assert_eq!(result.kvps.value(b"PID"), b"123");
        assert_eq!(result.kvps.value(b"empty"), b"");
        // The \n terminates parsing so Foo and SysRef should not be found.
    }
}

#[cfg(test)]
mod trailing_kvps_tests {
    use super::*;

    #[test]
    pub fn with_trailing_kvp_no_value() {
        let line = b"2018-09-26 12:34:56.7654321 | [INFO_] | Message SysRef=";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 1);
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.kvps.value(b"SysRef"), b"");
    }

    #[test]
    pub fn with_trailing_kvp_standard_value() {
        let line = b"2018-09-26 12:34:56.7654321 | [INFO_] | Message SysRef=AA123456";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 1);
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.kvps.value(b"SysRef"), b"AA123456");
    }

    #[test]
    pub fn with_trailing_kvp_quoted_value() {
        let line = b"2018-09-26 12:34:56.7654321 | [INFO_] | Message SysRef=\"It's like AA123456\"";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 1);
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.kvps.value(b"SysRef"), b"It's like AA123456");
    }

    #[test]
    pub fn with_multiple_trailing_kvps_returns_kvps() {
        let line = b"2018-09-26 12:34:56.7654321 | [INFO_] | Message Foo=Bar Hit= Http.Request=http:/www.foo.com SysRef=\"It's like AA123456\"";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 4);
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.kvps.value(b"SysRef"), b"It's like AA123456");
        assert_eq!(result.kvps.value(b"Foo"), b"Bar");
        assert_eq!(result.kvps.value(b"Hit"), b"");
        assert_eq!(result.kvps.value(b"Http.Request"), b"http:/www.foo.com");
    }

    #[test]
    pub fn with_quoted_kvp_that_spans_lines_returns_kvp_with_newlines() {
        // Stripping of the newlines will occur when we write the CSV, so
        // it is OK for them to remain at this point.
        let line = b"2018-09-26 12:34:56.7654321 | [INFO_] | Message Foo=\"Bar\nBar2\nBar3\" Hit= Http.Request=http:/www.foo.com";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 3);
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.kvps.value(b"foo"), b"Bar\nBar2\nBar3");
        assert_eq!(result.kvps.value(b"hit"), b"");
        assert_eq!(result.kvps.value(b"Http.Request"), b"http:/www.foo.com");
    }
}


#[cfg(test)]
mod message_extraction_tests {
    use super::*;

    #[test]
    fn with_incomplete_prologue_sets_message_to_empty() {
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.1146655").expect("Parse should succeed");
        assert_eq!(result.message.as_ref(), b"");
    }

    #[test]
    fn with_no_message_sets_message_to_empty() {
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.1146655 | pid=12 |").expect("Parse should succeed");
        assert_eq!(result.message.as_ref(), b"");
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.1146655 | pid=12").expect("Parse should succeed");
        assert_eq!(result.message.as_ref(), b"");
        let result = ParsedLine::parse(b"2018-09-26 12:34:56.1146655 | pid=12 |   ").expect("Parse should succeed");
        assert_eq!(result.message.as_ref(), b"");
    }

    #[test]
    pub fn with_no_trailing_kvps() {
        let line = b"2018-09-26 12:34:56.7654321 | a=\"Value with space\" | pid=123 | Empty= | [INFO_] | Some long message";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.message.as_ref(), b"Some long message");
    }

    #[test]
    pub fn with_leading_and_trailing_whitespace() {
        let line = b"2018-09-26 12:34:56.7654321 | a=\"Value with space\" | pid=123 | Empty= | [INFO_] |    Some long message   \r\n";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.message.as_ref(), b"Some long message");
    }

    #[test]
    pub fn with_message_with_newlines_extracts_message() {
        let line = b"2018-09-26 12:34:56.7654321 | a=\"Value with space\" | pid=123 | Empty= | [INFO_] | Message\n| | line2\nFoo=Bar SysRef=AA123456";
        let result = ParsedLine::parse(line).expect("Parse should succeed");
        assert_eq!(result.message.as_ref(), b"Message\n| | line2");
    }
}

#[cfg(test)]
mod real_log_line_tests {
    use super::*;

    #[test]
    pub fn capacity_service_test() {
        let mut line = "2018-09-26 12:34:56.1146655 | MachineName=Some.machine.net | AppName=Some.Service-Z63JHGJKK23 | pid=4964 | tid=22 | [INFO_] | Running aggregate capacity generator.".to_string();
        line.push_str("\n Source=AggregateCapacityGenerator Action=Run");
        line.push_str("\n SourceInfo=\"Something.Something.DarkSide.Aggregation.AggregateCapacityGenerator, Something.Something.DarkSide v1.0.0\"");
        line.push_str("\n SourceInstance=38449385");

        let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
        assert_eq!(result.log_date, b"2018-09-26 12:34:56.1146655");
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.message.to_vec(), b"Running aggregate capacity generator.".to_vec());

        assert_eq!(result.kvps.len(), 8);
        assert_eq!(result.kvps.value(b"MachineName"), b"Some.machine.net");
        assert_eq!(result.kvps.value(b"AppName"), b"Some.Service-Z63JHGJKK23");
        assert_eq!(result.kvps.value(b"pid"), b"4964");
        assert_eq!(result.kvps.value(b"tid"), b"22");
        assert_eq!(result.kvps.value(b"Source"), b"AggregateCapacityGenerator");
        assert_eq!(result.kvps.value(b"Action"), b"Run");
        assert_eq!(result.kvps.value(b"SourceInfo").to_vec(), b"Something.Something.DarkSide.Aggregation.AggregateCapacityGenerator, Something.Something.DarkSide v1.0.0".to_vec());
        assert_eq!(result.kvps.value(b"SourceInstance"), b"38449385");
    }

    #[test]
    pub fn capacity_service_test2() {
        let mut message = "42 surveyors and their capacity have been built with the following information:".to_string();
        message.push_str("\n: , 3211, Date: 28/09/2018, Points Target: 6.0, Absence Points: 0, Booked Cases: 1, Booked Case Points: 1.0, Basic Allocation: 1.0, Points Outstanding: 5.0, Success?:True, ");
        message.push_str("\nSurveyor: , 3211, Date: 29/09/2018, Points Target: 0, Absence Points: 0, Booked Cases: 0, Booked Case Points: 0, Basic Allocation: 0, Points Outstanding: 0, Success?:False, Case QU096709 requires 1.0 points but the surveyor has 0 points on this day");
        message.push_str("\nSurveyor: , 50046, Date: 10/10/2018, Points Target: 0, Absence Points: 0, Booked Cases: 0, Booked Case Points: 0, Basic Allocation: 0, Points Outstanding: 0, Success?:False, Case QU096709 requires 1.0 points but the surveyor has 0 points on this day");
        message.push_str("\nSurveyor: , 50046, Date: 11/10/2018, Points Target: 0, Absence Points: 0, Booked Cases: 0, Booked Case Points: 0, Basic Allocation: 0, Points Outstanding: 0, Success?:False, Case QU096709 requires 1.0 points but the surveyor has 0 points on this day");

        let mut line = "2018-09-27 11:29:51.0680203 | MachineName=Some.machine.net | AppName=Some.Service-Z63JHGJKK23 | pid=4964 | tid=144 | [INFO_] | ".to_string();
        line.push_str(&message);
        line.push_str("\n");
        line.push_str("\n Source=SurveyorCapacityService Action=GetSurveyorAvailabilityFor");
        line.push_str("\n SourceInfo=\"What.Ever.Another.Service.SurveyorCapacityService, Blah.Blah.Blah.Services v1.11.18213.509\"");
        line.push_str("\n SourceInstance=855390 startDate=28/09/2018 endDate=11/10/2018 postcode=\"MK16 8QF\"");

        let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
        assert_eq!(result.log_date, b"2018-09-27 11:29:51.0680203");
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.message, message.as_bytes());

        assert_eq!(result.kvps.len(), 11);
        assert_eq!(result.kvps.value(b"MachineName"), b"Some.machine.net");
        assert_eq!(result.kvps.value(b"AppName"), b"Some.Service-Z63JHGJKK23");
        assert_eq!(result.kvps.value(b"pid"), b"4964");
        assert_eq!(result.kvps.value(b"tid"), b"144");
        assert_eq!(result.kvps.value(b"Source"), b"SurveyorCapacityService");
        assert_eq!(result.kvps.value(b"Action"), b"GetSurveyorAvailabilityFor");
        assert_eq!(result.kvps.value(b"SourceInfo").to_vec(), b"What.Ever.Another.Service.SurveyorCapacityService, Blah.Blah.Blah.Services v1.11.18213.509".to_vec());
        assert_eq!(result.kvps.value(b"SourceInstance"), b"855390");
        assert_eq!(result.kvps.value(b"startDate"), b"28/09/2018");
        assert_eq!(result.kvps.value(b"endDate"), b"11/10/2018");
        assert_eq!(result.kvps.value(b"postcode"), b"MK16 8QF");
    }

    #[test]
    pub fn capacity_service_test3() {
        let mut message = " ApplicationException - The requested endpoint '/4142' could not be found".to_string();
        message.push_str("\n||   at Secret.ServiceClient.Clients.ClientContractServiceClient.<ParseErrorForEndpoint>d__12.MoveNext()");
        message.push_str("\n||--- End of stack trace from previous location where exception was thrown ---");
        message.push_str("\n||   at System.Runtime.ExceptionServices.ExceptionDispatchInfo.Throw()");
        message.push_str("\n||   at System.Runtime.CompilerServices.TaskAwaiter.HandleNonSuccessAndDebuggerNotification(Task task)");
        message.push_str("\n||   at Secret.ServiceClient.Clients.ClientContractServiceClient.<GetContractByIdAsync>d__10.MoveNext()");
        message.push_str("\n||--- End of stack trace from previous location where exception was thrown ---");
        message.push_str("\n||   at System.Runtime.ExceptionServices.ExceptionDispatchInfo.Throw()");
        message.push_str("\n||   at System.Runtime.CompilerServices.TaskAwaiter.HandleNonSuccessAndDebuggerNotification(Task task)");
        message.push_str("\n||   at System.Runtime.CompilerServices.TaskAwaiter`1.GetResult()");
        message.push_str("\n||   at What.Ever.Another.Service.SurveyorCapacityService.<SomeSpecialAction>d__9.MoveNext() in C:\\build\\job1\\Services\\SurveyorCapacity\\TheService.cs:line 177");
        message.push_str("\n||--- End of stack trace from previous location where exception was thrown ---");
        message.push_str("\n||   at System.Runtime.ExceptionServices.ExceptionDispatchInfo.Throw()");
        message.push_str("\n||   at System.Runtime.CompilerServices.TaskAwaiter.HandleNonSuccessAndDebuggerNotification(Task task)");
        message.push_str("\n||   at System.Runtime.CompilerServices.TaskAwaiter`1.GetResult()");
        message.push_str("\n||   at Blah.Blah.Blah.IISHost.Api.OurController.<SomeSpecialAction>d__4.MoveNext() in C:\\build\\job1\\IISHost\\Api\\TheController.cs:line 123");

        let mut line = "2018-11-27 10:33:37.2324929 | pid=6384 | tid=57 | [ERROR] | ".to_string();
        line.push_str(&message);
        line.push_str("\n");
        line.push_str("\n");
        line.push_str("\n Source=OurController Action=SomeSpecialAction");
        line.push_str("\n SourceInfo=\"Blah.Blah.Blah.IISHost.Api.OurController, Blah.Blah.Blah.IISHost v1.12.18323.4\"");
        line.push_str("\n SourceInstance=35519589 startDate=28/11/2018 endDate=04/12/2018 sysref=QU090700");

        let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
        assert_eq!(result.log_date, b"2018-11-27 10:33:37.2324929");
        assert_eq!(result.log_level, b"[ERROR]");
        assert_eq!(result.message, (&message[1..]).as_bytes());  // This message has an extra leading space, we need to trim it for the test.

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value(b"pid"), b"6384");
        assert_eq!(result.kvps.value(b"tid"), b"57");
        assert_eq!(result.kvps.value(b"Source"), b"OurController");
        assert_eq!(result.kvps.value(b"Action"), b"SomeSpecialAction");
        assert_eq!(result.kvps.value(b"SourceInfo").to_vec(), b"Blah.Blah.Blah.IISHost.Api.OurController, Blah.Blah.Blah.IISHost v1.12.18323.4".to_vec());
        assert_eq!(result.kvps.value(b"SourceInstance"), b"35519589");
        assert_eq!(result.kvps.value(b"startDate"), b"28/11/2018");
        assert_eq!(result.kvps.value(b"endDate"), b"04/12/2018");
        assert_eq!(result.kvps.value(b"sysref"), b"QU090700");
    }

    #[test]
    pub fn capacity_service_test4() {
        let mut line = "2018-11-27 10:33:37.2324929 | pid=6384 | tid=57 | [INFO_] | Some irrelevant message".to_string();
        line.push_str("\n Source=SecretService Action=GetSurveyorScheduleSummary");
        line.push_str("\n SourceInfo=\"Blah.Blah.Blah.Services.SurveyorSchedules.SecretService, Blah.Blah.Blah.Services v1.12.18323.4\"");
        line.push_str("\n SourceInstance=17453777");
        line.push_str("\n criteria=\"{_IncludeInactiveSurveyors_:true,_StartDate_:{_year_:2018,_month_:11,_day_:27},_NumberOfDays_:7,_Skip_:0,_Take_:10,_Filter_:[{_FieldName_:0,_Value_:_1_}],_SortField_:0,_Descending_:false}\"");
        line.push_str("\n surveyorsCount=10");
        line.push_str("\n summary=\"");
        line.push_str("\n Total surveyors: 577");
        line.push_str("\n Total surveyors matching criteria: 577\"");

        let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
        assert_eq!(result.log_date, b"2018-11-27 10:33:37.2324929");
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.message.as_ref(), b"Some irrelevant message");

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value(b"pid"), b"6384");
        assert_eq!(result.kvps.value(b"tid"), b"57");
        assert_eq!(result.kvps.value(b"Source"), b"SecretService");
        assert_eq!(result.kvps.value(b"Action"), b"GetSurveyorScheduleSummary");
        assert_eq!(result.kvps.value(b"SourceInfo").to_vec(), b"Blah.Blah.Blah.Services.SurveyorSchedules.SecretService, Blah.Blah.Blah.Services v1.12.18323.4".to_vec());
        assert_eq!(result.kvps.value(b"SourceInstance"), b"17453777");
        assert_eq!(result.kvps.value(b"criteria").to_vec(), b"{_IncludeInactiveSurveyors_:true,_StartDate_:{_year_:2018,_month_:11,_day_:27},_NumberOfDays_:7,_Skip_:0,_Take_:10,_Filter_:[{_FieldName_:0,_Value_:_1_}],_SortField_:0,_Descending_:false}".to_vec());
        // TODO: Interesting case of whitespace at the ends of a double-quoted value. Very rare.
        assert_eq!(result.kvps.value(b"summary").to_vec(), b"\n Total surveyors: 577\n Total surveyors matching criteria: 577".to_vec());
    }

    #[test]
    pub fn case_service_test() {
        let mut line = "2018-12-03 14:42:48.1783541 | MachineName=RD12345.corp.net | AppName=Another.Host | pid=8508 | tid=1 | [VRBSE] | Attempting to load assembly C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll".to_string();
        line.push_str("\n Source=ContainerBuilder Action=GetOrLoadAssembly");
        line.push_str("\n SourceInfo=\"Some.UnityThing.ContainerBuilder, Some.UnityThing v1.12.18333.11642\" SourceInstance=61115925");
        line.push_str("\n AssemblyFile=C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll");

        let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
        assert_eq!(result.log_date, b"2018-12-03 14:42:48.1783541");
        assert_eq!(result.log_level, b"[VRBSE]");
        assert_eq!(result.message.to_vec(), b"Attempting to load assembly C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll".to_vec());

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value(b"MachineName"), b"RD12345.corp.net");
        assert_eq!(result.kvps.value(b"AppName"), b"Another.Host");
        assert_eq!(result.kvps.value(b"pid"), b"8508");
        assert_eq!(result.kvps.value(b"tid"), b"1");
        assert_eq!(result.kvps.value(b"Source"), b"ContainerBuilder");
        assert_eq!(result.kvps.value(b"Action"), b"GetOrLoadAssembly");
        assert_eq!(result.kvps.value(b"SourceInfo").to_vec(), b"Some.UnityThing.ContainerBuilder, Some.UnityThing v1.12.18333.11642".to_vec());
        assert_eq!(result.kvps.value(b"SourceInstance"), b"61115925");
        assert_eq!(result.kvps.value(b"AssemblyFile").to_vec(), b"C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll".to_vec());
    }

    #[test]
    pub fn audit_client_test() {
        let mut line = "2018-06-27 12:40:02.8554336 | pid=7900 | tid=18 | [INFO_] | Successfully retrived 20 number of audit items for target id PD123456 and targetType Case".to_string();
        line.push_str("\n Source=TheClient");
        line.push_str("\n Action=TheAuditAction");
        line.push_str("\n CorrelationKey=0f5feb1d-996e-499d-9a52-7741b543c21d");
        line.push_str("\n Tenant=Somebody");
        line.push_str("\n UserId=SomeUserId");
        line.push_str("\n UserName=Philip+Daniels");
        line.push_str("\n UserIdentity=SomeUserIdentity");
        line.push_str("\n UserEmail=philip.daniels%40ex.com");
        line.push_str("\n Owin.Request.Id=5a7223cb-06ef-4620-92a8-57eeb7c04b7c");
        line.push_str("\n Owin.Request.Path=/api/tosomewhere/PD123456/20");
        line.push_str("\n Owin.Request.QueryString=");
        line.push_str("\n SourceInfo=\"Something.Auditor.ReadClient.TheClient, Something.Auditor.ReadClient v1.10.18155.4\"");
        line.push_str("\n SourceInstance=56183685");
        line.push_str("\n pageSize=20");
        line.push_str("\n targetId=PD123456");
        line.push_str("\n targetType=Case");

        let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
        assert_eq!(result.log_date, b"2018-06-27 12:40:02.8554336");
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.message.to_vec(), b"Successfully retrived 20 number of audit items for target id PD123456 and targetType Case".to_vec());

        assert_eq!(result.kvps.len(), 18);
        assert_eq!(result.kvps.value(b"pid"), b"7900");
        assert_eq!(result.kvps.value(b"tid"), b"18");
        assert_eq!(result.kvps.value(b"Source"), b"TheClient");
        assert_eq!(result.kvps.value(b"Action"), b"TheAuditAction");
        assert_eq!(result.kvps.value(b"CorrelationKey").to_vec(), b"0f5feb1d-996e-499d-9a52-7741b543c21d".to_vec());
        assert_eq!(result.kvps.value(b"Tenant"), b"Somebody");
        assert_eq!(result.kvps.value(b"UserId"), b"SomeUserId");
        assert_eq!(result.kvps.value(b"UserName"), b"Philip+Daniels");
        assert_eq!(result.kvps.value(b"UserIdentity"), b"SomeUserIdentity");
        assert_eq!(result.kvps.value(b"UserEmail"), b"philip.daniels%40ex.com");
        assert_eq!(result.kvps.value(b"Owin.Request.Id").to_vec(), b"5a7223cb-06ef-4620-92a8-57eeb7c04b7c".to_vec());
        assert_eq!(result.kvps.value(b"Owin.Request.Path"), b"/api/tosomewhere/PD123456/20");
        assert_eq!(result.kvps.value(b"Owin.Request.QueryString"), b"");
        assert_eq!(result.kvps.value(b"SourceInfo").to_vec(), b"Something.Auditor.ReadClient.TheClient, Something.Auditor.ReadClient v1.10.18155.4".to_vec());
        assert_eq!(result.kvps.value(b"SourceInstance"), b"56183685");
        assert_eq!(result.kvps.value(b"pageSize"), b"20");
        assert_eq!(result.kvps.value(b"targetId"), b"PD123456");
        assert_eq!(result.kvps.value(b"targetType"), b"Case");
    }

    #[test]
    pub fn notification_template_test() {
        let mut message = "Notification Template - [Invoice Authorisation Code => {InvoiceAuthorisationCode}".to_string();
        message.push_str("\nInvoice Customer Payment Type => {InvoiceCustomerPaymentType}");
        message.push_str("\nInvoice Customer Payment Date => {InvoiceCustomerPaymentDate}");
        message.push_str("\nVal Fee => {GrossFee}");
        message.push_str("\n\n");
        message.push_str("\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Pellentesque erat diam, blandit ut turpis a, vehicula porttitor dui. Suspendisse et mattis nisl. Maecenas egestas interdum quam, commodo sollicitudin nulla viverra nec. Vivamus facilisis semper dui sed finibus. ");
        message.push_str("\n\n");
        message.push_str("\nIn magna lacus, feugiat ut arcu id, condimentum commodo justo. Curabitur at augue enim. Phasellus ultricies dignissim ex, tincidunt consectetur turpis euismod at. Phasellus rhoncus maximus mauris vel convallis. Vestibulum ante ipsum primis in faucibus orci luctus et ultrices posuere cubilia Curae.] is merging the tags for case QU076868.");

        let mut line = "2018-06-27 12:32:00.6811879 | pid=7900 | tid=21 | [INFO_] | ".to_string();
        line.push_str(&message);
        line.push_str("\n");
        line.push_str("\n Source=NotificationTemplater");
        line.push_str("\n Action=GenerateNotificationMessage");
        line.push_str("\n CorrelationKey=122a47ac-2af1-4afa-b4f1-b0bf297450f3");
        line.push_str("\n SourceInfo=\"Our.Templater.NotificationTemplater, Our.Templater.Notifications v0.5.18129.24\"");
        line.push_str("\n SourceInstance=52920148");
        // template is a multi-line KVP.
        line.push_str("\n template=\"Invoice Authorisation Code => {InvoiceAuthorisationCode}");
        line.push_str("\nInvoice Customer Payment Type => {InvoiceCustomerPaymentType}");
        line.push_str("\n some words \"");
        line.push_str("\n SysRef=QU076868");

        let result = ParsedLine::parse(line.as_bytes()).expect("Parse should succeed");
        assert_eq!(result.log_date, b"2018-06-27 12:32:00.6811879");
        assert_eq!(result.log_level, b"[INFO_]");
        assert_eq!(result.message, message.as_bytes());

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value(b"pid"), b"7900");
        assert_eq!(result.kvps.value(b"tid"), b"21");
        assert_eq!(result.kvps.value(b"Source"), b"NotificationTemplater");
        assert_eq!(result.kvps.value(b"Action"), b"GenerateNotificationMessage");
        assert_eq!(result.kvps.value(b"CorrelationKey").to_vec(), b"122a47ac-2af1-4afa-b4f1-b0bf297450f3".to_vec());
        assert_eq!(result.kvps.value(b"SourceInfo").to_vec(), b"Our.Templater.NotificationTemplater, Our.Templater.Notifications v0.5.18129.24".to_vec());
        assert_eq!(result.kvps.value(b"SourceInstance"), b"52920148");
        assert_eq!(result.kvps.value(b"template").to_vec(), b"Invoice Authorisation Code => {InvoiceAuthorisationCode}\nInvoice Customer Payment Type => {InvoiceCustomerPaymentType}\n some words ".to_vec());
        assert_eq!(result.kvps.value(b"SysRef"), b"QU076868");
    }
}
