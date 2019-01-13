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

/// Represents a log file line after parsing into a convenient format.
/// Owned strings are used for all components so that the upstream code in main can
/// perform filtering and sorting without worrying about lifetimes. If things were &str,
/// then they would have a lifetime tied to the original line read from the log file.
/// Unfortunately, this design slows down the program by a factor of 3 (3 -> 10 seconds
/// runtime).
/// TODO: It might actually be make_output_record which is the cause of the slowdown.
#[derive(Debug, Default)]
pub struct ParsedLine {
    /// The entire original line with whitespace trimmed from the ends.
    pub line: String,
    pub log_date: String,
    pub log_level: String,
    pub message: String,
    pub kvps: KVPCollection
}

impl ParsedLine {
    pub fn new(line: &str) -> Result<Self, LineParseError> {
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
