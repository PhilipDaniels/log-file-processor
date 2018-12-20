use std::borrow::Cow;
use crate::parse_utils::*;
use crate::kvps::{KVPCollection, next_kvp, prev_kvp};
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


#[derive(Debug, PartialEq, Eq)]
pub enum LineParseError {
    EmptyLine,
    IncompleteLine(String),
    BadLogDate(String)
}

#[derive(Debug, Default)]
pub struct ParsedLine<'t> {
    /// The entire original line with whitespace trimmed from the ends.
    pub line: &'t str,
    pub log_date: &'t str,
    pub log_level: &'t str,
    pub message: Cow<'t, str>,
    pub kvps: KVPCollection<'t>
}

impl<'t, 'k> ParsedLine<'t> {
    pub fn new(line: &'t str) -> Result<Self, LineParseError> {
        if line.len() == 0 {
            return Err(LineParseError::EmptyLine);
        }

        let chars = line.char_indices().collect::<Vec<_>>();
        let last_valid_chars_index = chars.len().saturating_sub(1);

        // Trim whitespace from the start. If everything gets trimmed we were passed an empty line.
        let start = next_none_ws(&chars, 0, last_valid_chars_index);
        if start.is_none() {
            return Err(LineParseError::EmptyLine);
        }
        let start = start.unwrap();

        // Trim whitespace from the end.
        let end = prev_none_ws(&chars, chars.len() - 1, start);
        if end.is_none() {
            panic!("Since start is not None, end can never be None so this should not occur.");
        }
        let end = end.unwrap();

        // Store a copy of the entire trimmed line.
        let mut parsed_line = ParsedLine::default();
        parsed_line.line = unchecked_slice(line, &chars, start, end);

        // Try to extract the log_date. 
        // Post: end is on the last fractional seconds digit.
        let end = extract_log_date(&chars, start, end)?;
        parsed_line.log_date = unchecked_slice(line, &chars, start, end);

        // Now, in the remainder of the line (if there is any), extract KVPs/prologue items until we reach the message.
        let start = next_none_ws_or_pipe_after(&chars, end, last_valid_chars_index);
        if start.is_none() {
            return Ok(parsed_line);
        }

        // We could optimize this, but for now it will do. We are unlikely to hit it.
        let mut end = last_valid_chars_index;
        let mut start = start.unwrap();

        // Now start to eat KVPs until we find something that is not a KVP.
        // Record the index of the last one we find, we need that to find the start of the message.
        let mut end_of_last_kvp = 0;
        while let Some(kvp) = next_kvp(&chars, start, end) {
            debug_assert!(kvp.has_key(), "A KVP should always have a key, we can't do anything with it otherwise. KVP = {:?}", kvp);
            let kvp_strings = kvp.get_kvp_strings(line, &chars);
            //println!(">>>>> NEXTKVP = {:?}, kvp_strings = {:?}", kvp, kvp_strings);

            if kvp.is_log_level {
                parsed_line.log_level = kvp_strings.key;
            } else {
                parsed_line.kvps.insert(kvp_strings);
            }

            end_of_last_kvp = kvp.max_index();
            let potential_start = next_none_ws_or_pipe_after(&chars, end_of_last_kvp, end);
            if potential_start.is_none() { break; }
            start = potential_start.unwrap();
        }
        
        let start_of_message_index = next_none_ws_or_pipe_after(&chars, end_of_last_kvp, end);
        if start_of_message_index.is_none() {
            // Reached the end of the line without there being a message.
            return Ok(parsed_line);
        }
        let start_of_message_index = start_of_message_index.unwrap();
        
        // Now we have located the start of the message we can begin to eat KVPs from the end of the line,
        // going backwards, until we are no longer hitting KVPs. Note that it is quite likely there will
        // be several lines separated by \r characters.
        let limit = start_of_message_index;
        let mut start_of_this_kvp = end;    // We start looking at the last character on the line.

        while let Some(kvp) = prev_kvp(&chars, end, limit) {
            debug_assert!(kvp.has_key(), "A KVP should always have a key, we can't do anything with it otherwise. KVP = {:?}", kvp);
            let kvp_strings = kvp.get_kvp_strings(line, &chars);
            // println!(">>>>> PREVKVP = {:?}, k = {:?}, v = {:?}", kvp, k, v);
            parsed_line.kvps.insert(kvp_strings);

            start_of_this_kvp = kvp.min_index();
            end = prev_none_ws(&chars, start_of_this_kvp - 1, limit).expect("Should be safe to unwrap, there should be a message.");
        }

        // If there were no KVPs then the end of the message is just the end of the line.
        // Otherwise, go back to the first non-ws character.
        let end_of_message_index = if start_of_this_kvp == end {
            end
        } else {
            prev_none_ws(&chars, start_of_this_kvp - 1, limit).expect("Should be safe to unwrap, there should be a message.")
        };
        
        // This 'if' is deals with the badly-formed line case of "2018-09-26 12:34:56.1146655 | pid=12".
        if !(last_valid_chars_index == start_of_message_index && last_valid_chars_index == end_of_message_index) {
            // Now we can extract the message, and clean it up so it has no newline characters,
            // which means it will work in a CSV file without further processing.
            parsed_line.message = checked_slice(line, &chars, start_of_message_index, end_of_message_index);
        }

        Ok(parsed_line)
    }
}


/// Pre: current and limit are valid indexes into chars[].
/// For " 2018-09-26 12:34:56.1146657 ...", current will be '2' and limit on the last character.
/// Returns: Ok(n) where n will be on the final 7 or an error.
fn extract_log_date(chars: &[(usize, char)], mut current: usize, limit: usize) -> Result<usize, LineParseError> {
    const LENGTH_OF_LOGGING_TIMESTAMP: usize = 27;

    if num_chars_available(current, limit) < LENGTH_OF_LOGGING_TIMESTAMP {
        let msg = format!("The input line is less than {} characters, which indicates it does not even contain a logging timestamp", LENGTH_OF_LOGGING_TIMESTAMP);
        return Err(LineParseError::IncompleteLine(msg));
    }

    // Y
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YY
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYY
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-
    if chars[current].1 != '-' {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "-", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-M
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-
    if chars[current].1 != '-' {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "-", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-D
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_
    if chars[current].1 != ' ' {
        let msg = format!("Character {} was expected to be {}, but was {}", current, " ", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_H
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH:
    if chars[current].1 != ':' {
        let msg = format!("Character {} was expected to be {}, but was {}", current, ":", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH:M
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH:MM
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH:MM:
    if chars[current].1 != ':' {
        let msg = format!("Character {} was expected to be {}, but was {}", current, ":", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH:MM:S
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH:MM:SS
    if !chars[current].1.is_digit(10) {
        let msg = format!("Character {} was expected to be {}, but was {}", current, "digit", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }
    current += 1;

    // YYYY-MM-DD_HH:MM:SS.
    if chars[current].1 != '.' {
        let msg = format!("Character {} was expected to be {}, but was {}", current, ".", chars[current].1);
        return Err(LineParseError::BadLogDate(msg));
    }

    // YYYY-MM-DD_HH:MM:SS.FFFFFFF
    // Consume all remaining decimal characters. This gives us some flexibility for the future, if it ever becomes possible to have
    // more precision in the logging timestamps. Theoretically, this code allows one of the F to be a non-digit, but
    // I am willing to live with that, it never happens in real life.
    // Leave fraction_end on the last decimal digit.
    let mut fraction_end = current + 1;
    while fraction_end <= limit && chars[fraction_end].1.is_digit(10) {
        fraction_end += 1;
    }
    fraction_end -=1;

    if fraction_end == current {
        Err(LineParseError::BadLogDate("No fractional seconds part was detected on the log_date".to_string()))
    } else {
        Ok(fraction_end)
    }
}

#[cfg(test)]
mod white_space_tests {
    use super::*; 

    #[test]
    fn blank_line_returns_error() {
        let result = ParsedLine::new("");
        match result {
            Err(LineParseError::EmptyLine) => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn whitespace_line_returns_error() {
        let result = ParsedLine::new("  \r  ");
        match result {
            Err(LineParseError::EmptyLine) => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn trims_whitespace_from_both_ends() {
        let result = ParsedLine::new("  \r\n 2018-09-26 12:34:56.1146655 | MachineName=Some.machine.net | Message\r\n   ")
            .expect("Parse should succeed");
        assert_eq!(result.line, "2018-09-26 12:34:56.1146655 | MachineName=Some.machine.net | Message");
    }
}

#[cfg(test)]
mod log_date_tests {
    use super::*; 

    #[test]
    fn short_line_returns_error() {
        let result = ParsedLine::new("2018-12");
        match result {
            Err(LineParseError::IncompleteLine(ref msg)) if msg.contains("logging timestamp") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y1_returns_error() {
        let result = ParsedLine::new("x018-09-26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 0") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y2_returns_error() {
        let result = ParsedLine::new("2x18-09-26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 1") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y3_returns_error() {
        let result = ParsedLine::new("20x8-09-26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 2") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_y4_returns_error() {
        let result = ParsedLine::new("201x-09-26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 3") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep1_returns_error() {
        let result = ParsedLine::new("2018x09-26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 4") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_mon1_returns_error() {
        let result = ParsedLine::new("2018-x9-26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 5") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_mon2_returns_error() {
        let result = ParsedLine::new("2018-0x-26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 6") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep2_returns_error() {
        let result = ParsedLine::new("2018-09x26 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 7") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_d1_returns_error() {
        let result = ParsedLine::new("2018-09-x6 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 8") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_d2_returns_error() {
        let result = ParsedLine::new("2018-09-2x 12:34:56.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 9") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep3_returns_error() {
        let result = ParsedLine::new("2018-09-26x23:00:00.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 10") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_h1_returns_error() {
        let result = ParsedLine::new("2018-09-26 x3:00:00.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 11") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_h2_returns_error() {
        let result = ParsedLine::new("2018-09-26 2x:00:00.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 12") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep4_returns_error() {
        let result = ParsedLine::new("2018-09-26 23x00:00.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 13") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_min1_returns_error() {
        let result = ParsedLine::new("2018-09-26 23:x0:00.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 14") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_min2_returns_error() {
        let result = ParsedLine::new("2018-09-26 23:0x:00.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 15") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep5_returns_error() {
        let result = ParsedLine::new("2018-09-26 23:00x00.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 16") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_s1_returns_error() {
        let result = ParsedLine::new("2018-09-26 23:00:x0.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 17") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_s2_returns_error() {
        let result = ParsedLine::new("2018-09-26 23:00:0x.7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 18") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_invalid_sep6_returns_error() {
        let result = ParsedLine::new("2018-09-26 23:00:00x7654321");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("Character 19") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_no_fractions_returns_error() {
        let result = ParsedLine::new("2018-09-26 12:34:56. | some mesage to make the line longer enough");
        match result {
            Err(LineParseError::BadLogDate(ref msg)) if msg.contains("No fractional") => assert!(true),
            _ => assert!(false, "Unexpected result"),
        }
    }

    #[test]
    fn with_only_log_date_extracts_log_date() {
        let result = ParsedLine::new("2018-09-26 12:34:56.1146655").expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-09-26 12:34:56.1146655");
    }

    #[test]
    fn with_only_log_date_and_whitespace_extracts_log_date() {
        let result = ParsedLine::new(" 2018-09-26 12:34:56.1146655 ").expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-09-26 12:34:56.1146655");
    }

    #[test]
    fn with_nominal_line_extracts_log_date() {
        let result = ParsedLine::new("2018-09-26 12:34:56.1146655 | MachineName=foo | Message").expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-09-26 12:34:56.1146655");
    }

    #[test]
    fn with_longer_precision_extracts_log_date() {
        let result = ParsedLine::new("2018-09-26 12:34:56.12345678901 | MachineName=foo | Message").expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-09-26 12:34:56.12345678901");
    }

    #[test]
    fn with_shorter_precision_extracts_log_date() {
        let result = ParsedLine::new("2018-09-26 12:34:56.1234 | MachineName=foo | Message").expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-09-26 12:34:56.1234");
    }
}

#[cfg(test)]
mod prologue_tests {
    use super::*;

    #[test]
    pub fn with_empty_prologue_returns_empty_kvps_and_log_level() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321").expect("Parse should succeed");
        assert!(result.kvps.is_empty());
        assert!(result.log_level.is_empty())
    }

    #[test]
    pub fn with_prologue_containing_log_level_returns_appropriate_log_level() {
        for log_level in &LOG_LEVELS {
            let line = format!("2018-09-26 12:34:56.7654321 | a=b | {} | Message", log_level);
            let result = ParsedLine::new(&line).expect("Parse should succeed");
            assert_eq!(result.log_level, &log_level[0..log_level.len()]);
        }
    }

    #[test]
    pub fn with_prologue_containing_kpvs_returns_kvps() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | a=b | pid=123 | [INFO_] | Message")
            .expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 2);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("a"), "b");
        assert_eq!(result.kvps.value("PID"), "123");
    }

    #[test]
    pub fn with_prologue_containing_all_kpv_forms_returns_kvps() {
        //          _123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789
        let line = "2018-09-26 12:34:56.7654321 | a=\"Value with space\" | pid=123 | Empty= | [INFO_] | Message\n| | line2\nFoo=Bar SysRef=AA123456";
        let result = ParsedLine::new(line).expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 5);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("a"), "Value with space");
        assert_eq!(result.kvps.value("PID"), "123");
        assert_eq!(result.kvps.value("foo"), "Bar");
        assert_eq!(result.kvps.value("sysref"), "AA123456");
        assert_eq!(result.kvps.value("empty"), "");
    }

    #[test]
    pub fn with_prologue_and_no_message_returns_kvps() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | a=b | pid=123 | [INFO_] |").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 2);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("a"), "b");
        assert_eq!(result.kvps.value("PID"), "123");
    }

    #[test]
    pub fn with_prologue_and_no_message_returns_kvps2() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | a=b | pid=123 | [INFO_]").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 2);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("a"), "b");
        assert_eq!(result.kvps.value("PID"), "123");
    }
}

#[cfg(test)]
mod trailing_kvp_tests {
    use super::*;

    #[test]
    pub fn with_trailing_kvp_no_value() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | [INFO_] | Message SysRef=").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 1);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("sysref"), "");
    }

    #[test]
    pub fn with_trailing_kvp_standard_value() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | [INFO_] | Message SysRef=AA123456").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 1);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("sysref"), "AA123456");
    }

    #[test]
    pub fn with_trailing_kvp_quoted_value() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | [INFO_] | Message SysRef=\"It's like AA123456\"").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 1);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("sysref"), "It's like AA123456");
    }

    #[test]
    pub fn with_multiple_trailing_kvps_returns_kvps() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | [INFO_] | Message Foo=Bar Hit= Http.Request=http:/www.foo.com SysRef=\"It's like AA123456\"").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 4);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("sysref"), "It's like AA123456");
        assert_eq!(result.kvps.value("foo"), "Bar");
        assert_eq!(result.kvps.value("hit"), "");
        assert_eq!(result.kvps.value("Http.Request"), "http:/www.foo.com");
    }

    #[test]
    pub fn with_quoted_kvp_that_spans_lines_returns_values_with_newlines_replaced() {
        let result = ParsedLine::new("2018-09-26 12:34:56.7654321 | [INFO_] | Message Foo=\"Bar\nBar2\nBar3\" Hit= Http.Request=http:/www.foo.com").expect("Parse should succeed");
        assert_eq!(result.kvps.len(), 3);
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.kvps.value("foo"), "Bar Bar2 Bar3");
        assert_eq!(result.kvps.value("hit"), "");
        assert_eq!(result.kvps.value("Http.Request"), "http:/www.foo.com");
    }
}

#[cfg(test)]
mod message_extraction_tests {
    use super::*;

    #[test]
    fn with_incomplete_prologue_sets_message_to_empty() {
        let result = ParsedLine::new("2018-09-26 12:34:56.1146655").expect("Parse should succeed");
        assert_eq!(result.message, "");
    }

    #[test]
    fn with_no_message_sets_message_to_empty() {
        let result = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=12 |").expect("Parse should succeed");
        assert_eq!(result.message, "");
        let result = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=12").expect("Parse should succeed");
        assert_eq!(result.message, "");
        let result = ParsedLine::new("2018-09-26 12:34:56.1146655 | pid=12 |   ").expect("Parse should succeed");
        assert_eq!(result.message, "");
    }

    #[test]
    pub fn with_complex_kvps_extracts_message() {
        let line = "2018-09-26 12:34:56.7654321 | a=\"Value with space\" | pid=123 | Empty= | [INFO_] | Message\n| | line2\nFoo=Bar SysRef=AA123456";
        let result = ParsedLine::new(line).expect("Parse should succeed");
        assert_eq!(result.message, "Message | | line2");
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

        let result = ParsedLine::new(&line).expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-09-26 12:34:56.1146655");
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.message, "Running aggregate capacity generator.");

        assert_eq!(result.kvps.len(), 8);
        assert_eq!(result.kvps.value("MachineName"), "Some.machine.net");
        assert_eq!(result.kvps.value("AppName"), "Some.Service-Z63JHGJKK23");
        assert_eq!(result.kvps.value("pid"), "4964");
        assert_eq!(result.kvps.value("tid"), "22");
        assert_eq!(result.kvps.value("Source"), "AggregateCapacityGenerator");
        assert_eq!(result.kvps.value("Action"), "Run");
        assert_eq!(result.kvps.value("SourceInfo"), "Something.Something.DarkSide.Aggregation.AggregateCapacityGenerator, Something.Something.DarkSide v1.0.0");
        assert_eq!(result.kvps.value("SourceInstance"), "38449385");
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

        let result = ParsedLine::new(&line).expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-09-27 11:29:51.0680203");
        assert_eq!(result.log_level, "[INFO_]");
        message = message.replace("\n", " ");
        assert_eq!(result.message, message);

        assert_eq!(result.kvps.len(), 11);
        assert_eq!(result.kvps.value("MachineName"), "Some.machine.net");
        assert_eq!(result.kvps.value("AppName"), "Some.Service-Z63JHGJKK23");
        assert_eq!(result.kvps.value("pid"), "4964");
        assert_eq!(result.kvps.value("tid"), "144");
        assert_eq!(result.kvps.value("Source"), "SurveyorCapacityService");
        assert_eq!(result.kvps.value("Action"), "GetSurveyorAvailabilityFor");
        assert_eq!(result.kvps.value("SourceInfo"), "What.Ever.Another.Service.SurveyorCapacityService, Blah.Blah.Blah.Services v1.11.18213.509");
        assert_eq!(result.kvps.value("SourceInstance"), "855390");
        assert_eq!(result.kvps.value("startDate"), "28/09/2018");
        assert_eq!(result.kvps.value("endDate"), "11/10/2018");
        assert_eq!(result.kvps.value("postcode"), "MK16 8QF");
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

        let result = ParsedLine::new(&line).expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-11-27 10:33:37.2324929");
        assert_eq!(result.log_level, "[ERROR]");
        message = message.replace("\n", " ");
        assert_eq!(result.message, &message[1..]);  // This message has an extra leading space, we need to trim it for the test.

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value("pid"), "6384");
        assert_eq!(result.kvps.value("tid"), "57");
        assert_eq!(result.kvps.value("Source"), "OurController");
        assert_eq!(result.kvps.value("Action"), "SomeSpecialAction");
        assert_eq!(result.kvps.value("SourceInfo"), "Blah.Blah.Blah.IISHost.Api.OurController, Blah.Blah.Blah.IISHost v1.12.18323.4");
        assert_eq!(result.kvps.value("SourceInstance"), "35519589");
        assert_eq!(result.kvps.value("startDate"), "28/11/2018");
        assert_eq!(result.kvps.value("endDate"), "04/12/2018");
        assert_eq!(result.kvps.value("sysref"), "QU090700");
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
        
        let result = ParsedLine::new(&line).expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-11-27 10:33:37.2324929");
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.message, "Some irrelevant message");

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value("pid"), "6384");
        assert_eq!(result.kvps.value("tid"), "57");
        assert_eq!(result.kvps.value("Source"), "SecretService");
        assert_eq!(result.kvps.value("Action"), "GetSurveyorScheduleSummary");
        assert_eq!(result.kvps.value("SourceInfo"), "Blah.Blah.Blah.Services.SurveyorSchedules.SecretService, Blah.Blah.Blah.Services v1.12.18323.4");
        assert_eq!(result.kvps.value("SourceInstance"), "17453777");
        assert_eq!(result.kvps.value("criteria"), "{_IncludeInactiveSurveyors_:true,_StartDate_:{_year_:2018,_month_:11,_day_:27},_NumberOfDays_:7,_Skip_:0,_Take_:10,_Filter_:[{_FieldName_:0,_Value_:_1_}],_SortField_:0,_Descending_:false}");
        assert_eq!(result.kvps.value("summary"), "  Total surveyors: 577  Total surveyors matching criteria: 577");
    }

    #[test]
    pub fn case_service_test() {
        let mut line = "2018-12-03 14:42:48.1783541 | MachineName=RD12345.corp.net | AppName=Another.Host | pid=8508 | tid=1 | [VRBSE] | Attempting to load assembly C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll".to_string();
        line.push_str("\n Source=ContainerBuilder Action=GetOrLoadAssembly");
        line.push_str("\n SourceInfo=\"Some.UnityThing.ContainerBuilder, Some.UnityThing v1.12.18333.11642\" SourceInstance=61115925");
        line.push_str("\n AssemblyFile=C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll");

        let result = ParsedLine::new(&line).expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-12-03 14:42:48.1783541");
        assert_eq!(result.log_level, "[VRBSE]");
        assert_eq!(result.message, "Attempting to load assembly C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll");

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value("MachineName"), "RD12345.corp.net");
        assert_eq!(result.kvps.value("AppName"), "Another.Host");
        assert_eq!(result.kvps.value("pid"), "8508");
        assert_eq!(result.kvps.value("tid"), "1");
        assert_eq!(result.kvps.value("Source"), "ContainerBuilder");
        assert_eq!(result.kvps.value("Action"), "GetOrLoadAssembly");
        assert_eq!(result.kvps.value("SourceInfo"), "Some.UnityThing.ContainerBuilder, Some.UnityThing v1.12.18333.11642");
        assert_eq!(result.kvps.value("SourceInstance"), "61115925");
        assert_eq!(result.kvps.value("AssemblyFile"), "C:\\Users\\pdaniels\\AppData\\Local\\Temp\\Whatever-201802-03-1434124214.3324\\Something.Database.dll");
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

        let result = ParsedLine::new(&line).expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-06-27 12:40:02.8554336");
        assert_eq!(result.log_level, "[INFO_]");
        assert_eq!(result.message, "Successfully retrived 20 number of audit items for target id PD123456 and targetType Case");

        assert_eq!(result.kvps.len(), 18);
        assert_eq!(result.kvps.value("pid"), "7900");
        assert_eq!(result.kvps.value("tid"), "18");
        assert_eq!(result.kvps.value("Source"), "TheClient");
        assert_eq!(result.kvps.value("Action"), "TheAuditAction");
        assert_eq!(result.kvps.value("CorrelationKey"), "0f5feb1d-996e-499d-9a52-7741b543c21d");
        assert_eq!(result.kvps.value("Tenant"), "Somebody");
        assert_eq!(result.kvps.value("UserId"), "SomeUserId");
        assert_eq!(result.kvps.value("UserName"), "Philip+Daniels");
        assert_eq!(result.kvps.value("UserIdentity"), "SomeUserIdentity");
        assert_eq!(result.kvps.value("UserEmail"), "philip.daniels%40ex.com");
        assert_eq!(result.kvps.value("Owin.Request.Id"), "5a7223cb-06ef-4620-92a8-57eeb7c04b7c");
        assert_eq!(result.kvps.value("Owin.Request.Path"), "/api/tosomewhere/PD123456/20");
        assert_eq!(result.kvps.value("Owin.Request.QueryString"), "");
        assert_eq!(result.kvps.value("SourceInfo"), "Something.Auditor.ReadClient.TheClient, Something.Auditor.ReadClient v1.10.18155.4");
        assert_eq!(result.kvps.value("SourceInstance"), "56183685");
        assert_eq!(result.kvps.value("pageSize"), "20");
        assert_eq!(result.kvps.value("targetId"), "PD123456");
        assert_eq!(result.kvps.value("targetType"), "Case");
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

        let result = ParsedLine::new(&line).expect("Parse should succeed");
        assert_eq!(result.log_date, "2018-06-27 12:32:00.6811879");
        assert_eq!(result.log_level, "[INFO_]");
        message = message.replace("\n", " ");
        assert_eq!(result.message, message);

        assert_eq!(result.kvps.len(), 9);
        assert_eq!(result.kvps.value("pid"), "7900");
        assert_eq!(result.kvps.value("tid"), "21");
        assert_eq!(result.kvps.value("Source"), "NotificationTemplater");
        assert_eq!(result.kvps.value("Action"), "GenerateNotificationMessage");
        assert_eq!(result.kvps.value("CorrelationKey"), "122a47ac-2af1-4afa-b4f1-b0bf297450f3");
        assert_eq!(result.kvps.value("SourceInfo"), "Our.Templater.NotificationTemplater, Our.Templater.Notifications v0.5.18129.24");
        assert_eq!(result.kvps.value("SourceInstance"), "52920148");
        assert_eq!(result.kvps.value("template"), "Invoice Authorisation Code => {InvoiceAuthorisationCode} Invoice Customer Payment Type => {InvoiceCustomerPaymentType}  some words ");
        assert_eq!(result.kvps.value("SysRef"), "QU076868");
    }
}
