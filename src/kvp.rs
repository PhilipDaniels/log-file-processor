use crate::byte_extensions::{ByteExtensions, ByteSliceExtensions};

/// This module contains the representation of a Key-Value pair as parsed from the original line,
/// and some utility methods for doing that parsing.

/// The set of possible log level emitted by the logging framework.
/// They are ordered by frequency of occurence, as this should give a (very small!)
/// performance boost when checking for them.
pub const LOG_LEVELS: [&'static [u8]; 9] =
[
    b"[INFO_]",
    b"[DEBUG]",
    b"[VRBSE]",
    b"[WARNG]",
    b"[ERROR]",
    b"[FATAL]",
    b"[UNDEF]",
    b"[DEBG2]",
    b"[DEBG1]",
];

/// The name of the built-in LogDate column.
pub const LOG_DATE: &str = "LogDate";

/// The name of the built-in LogLevel column.
pub const LOG_LEVEL: &str = "LogLevel";

/// The name of the built-in LogSource column.
pub const LOG_SOURCE: &str = "LogSource";

/// The name of the built-in Message column.
pub const MESSAGE: &str = "Message";

/// Represents a single Key-Value pair as parsed from the log line.
//#[derive(Debug, Default)]
#[derive(Debug, Default)]
pub struct KVP<'f> {
    /// The key of the KVP. Should never be empty.
    pub key: &'f [u8],

    /// The value of the KVP. Can be empty, in the case of expressions like 'SysRef='.
    pub value: &'f [u8],

    /// It turns out to be handy to handle the log level field as a special case of
    /// a KVP, it makes parsing easier.
    pub is_log_level: bool
}

impl<'f> KVP<'f> {
    fn new(key: &'f [u8], value: &'f [u8]) -> Self {
        KVP { key, value, is_log_level: false }
    }
}

/// A Vec is probably as fast as a HashMap for the small number of KVPs we expect to see.
//#[derive(Debug, Default)]
#[derive(Debug, Default)]
pub struct KVPCollection<'f> {
    kvps: Vec<KVP<'f>>
}

impl<'f> KVPCollection<'f> {
    /// Insert a new KVP, but only if it does not already exist.
    pub fn insert(&mut self, new_kvp: KVP<'f>) {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(new_kvp.key) {
                return;
            }
        }

        self.kvps.push(new_kvp);
    }

    /// Gets a value, looking it up case-insensitively by the specified key.
    /// Returns None if there is no value for that key.
    pub fn get_value(&self, key: &[u8]) -> Option<&[u8]> {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(key) {
                return Some(kvp.value);
            }
        }

        None
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.kvps.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.kvps.is_empty()
    }

    /// Gets a value, looking it up case-insensitively by the specified key.
    /// Panics if the key is not in the collection. Helps keep tests short.
    #[cfg(test)]
    pub fn value(&self, key: &[u8]) -> &[u8] {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(key) {
                return &kvp.value;
            }
        }

        panic!("No value found for key {}", String::from_utf8(key.to_vec()).unwrap())
    }
}

pub trait ByteSliceKvpExtensions<'s> {
    fn next_kvp(self) -> KVPParseResult<'s>;
    fn prev_kvp(self) -> KVPParseResult<'s>;
}

/// The result of trying to parse a KVP from a slice. The remaining slice is always returned,
/// and normally there is also a KVP. If `kvp` is None, then no more KVPs could be found and
/// parsing should terminate.
#[derive(Debug, Default)]
pub struct KVPParseResult<'s> {
    pub remaining_slice: &'s [u8],
    pub kvp: Option<KVP<'s>>,
}

impl<'s> ByteSliceKvpExtensions<'s> for &'s [u8] {
    /// Attempts to extract a Key-Value pair from a slice, starting at the beginning of the slice
    /// and reading forward. There are several possible forms of a KVP:
    ///
    ///     Key=
    ///     Key=Value
    ///     Key="Value with space"
    ///
    /// These forms are guaranteed by the logging framework. In particular, there is guaranteed
    /// to be no space around the '=', and the value will be wrapped in double quotes if it has
    /// a quote or a space in it. 'Key' may contain '.', as in "HttpRequest.QueryString".
    ///
    /// Pre: The first character of the slice is the first character of the key - 'K' in the
    /// above examples.
    fn next_kvp(self) -> KVPParseResult<'s> {
        let no_kvp = KVPParseResult {
            remaining_slice: self,
            kvp: None
        };

        if self.is_empty() { return no_kvp; };

        // Scan forward looking for the equals sign. If we hit a whitespace character instead,
        // then we don't actually have a KVP. It MAY be the log-level in the prologue or we
        // might just be looking at some random text in the message.
        const LOG_LEVEL_LENGTH: usize = 7;
        let idx = self.iter().position(|&c| c == b'=' || c.is_whitespace());
        if self.len() >= LOG_LEVEL_LENGTH && (idx.is_none() || self[*idx.as_ref().unwrap()] != b'=') {
            let possible_log_level = &self[0..LOG_LEVEL_LENGTH];
            if LOG_LEVELS.contains(&possible_log_level) {
                //println!("  >> Returning Log Level {:?}", possible_log_level.to_string());
                let mut kvp = KVP::new(possible_log_level, b"");
                kvp.is_log_level = true;
                return KVPParseResult {
                    remaining_slice: &self[LOG_LEVEL_LENGTH..],
                    kvp: Some(kvp)
                };
            }
        }

        if idx.is_none() { return no_kvp };
        let idx = idx.unwrap_or(0);

        let key_slice = &self[0..idx];
        if key_slice.is_empty() || self[idx] != b'=' { return no_kvp };

        // The value should start immediately after the '=' with no intervening whitespace.
        let value_slice = &self[idx..].trim_left();
        //println!("  >> Case2, key_slice = {:?}, value_slice = {:?}", key_slice.to_string(), value_slice.to_string());

        if value_slice.is_empty() {
            // This is the pathological case where we reached the end of the input such as: "....Key="
            // In practice we should never reach here except with badly formed lines because such trailing
            // KVPs should be consumed by `prev_kvp`.
            return KVPParseResult {
                remaining_slice: value_slice,
                kvp: Some(KVP::new(key_slice, value_slice))
            };
        }

        //println!("  >> Case3, key_slice = {:?}, value_slice = {:?}", key_slice.to_string(), value_slice.to_string());

        // Now extract the value.
        if value_slice[0] == b'"' {
            // We have a value in double quotes. Strip off the leading quote.
            let value_slice = value_slice.trim_left();
            if value_slice.is_empty() {
                // We reached the end of input with the very last bytes being: 'Key="'
                return KVPParseResult {
                    remaining_slice: b"",
                    kvp: Some(KVP::new(key_slice, b""))
                };
            };

            // Find the closing quote. Don't allow values to span lines.
            let idx = value_slice.iter().position(|&c| c == b'"' || c == b'\r' || c == b'\n');
            if idx.is_none() {
                // We reached the end of input with the vert last bytes being: 'Key="unfinished'
                return KVPParseResult {
                    remaining_slice: b"",
                    kvp: Some(KVP::new(key_slice, value_slice))
                };
            }

            let idx = idx.unwrap();
            let hit_ws = value_slice[idx] != b'"';
            // 1 for the equals sign and 2 for the double quotes.
            let extra = if hit_ws { 2 } else {3 };
            let value_slice = &value_slice[0..idx];
            KVPParseResult {
                remaining_slice: &self[key_slice.len() + value_slice.len() + extra..],
                kvp: Some(KVP::new(key_slice, value_slice))
            }
        } else if value_slice[0].is_whitespace() {
            // We have an empty value ("Key= "). Make an empty slice.
            // 1 for the equals sign.
            KVPParseResult {
                remaining_slice: &self[key_slice.len() + 1..],
                kvp: Some(KVP::new(key_slice, b""))
            }
        } else {
            // We have a KVP of the form "Key=Value". Find the next whitespace character.
            let idx = value_slice.iter().position(|&c| c.is_whitespace()).unwrap_or(value_slice.len());
            let value_slice = &value_slice[0..idx];
            // 1 for the equals sign.
            KVPParseResult {
                remaining_slice: &self[key_slice.len() + 1 + value_slice.len()..],
                kvp: Some(KVP::new(key_slice, value_slice))
            }
        }
    }

    /// Attempts to extract a Key-Value pair from a slice, starting at the end of the slice
    /// and reading backwards. There are several possible forms of a KVP:
    ///
    ///     Key=
    ///     Key=Value
    ///     Key="Value with space"
    ///
    /// These forms are guaranteed by the logging framework. In particular, there is guaranteed
    /// to be no space around the '=', and the value will be wrapped in double quotes if it has
    /// a quote or a space in it. 'Key' may contain '.', as in "HttpRequest.QueryString".
    ///
    /// Pre: The last character of the slice is the last character of the value - '=', 'e' or
    /// '"' in the above examples. In particular, it is not a whitespace value: you must trim
    /// trailing whitespace before calling.
    fn prev_kvp(self) -> KVPParseResult<'s> {
        let no_kvp = KVPParseResult { remaining_slice: self, kvp: None };
        if self.is_empty() { return no_kvp; };

        // No test cases for this, it is just checking the pre-conditions.
        let last_char = self[self.len() - 1];
        if last_char.is_whitespace() { return no_kvp };

        let extract_key = |value_slice: &'s [u8], index_of_equals: usize| -> KVPParseResult<'s> {
            // index_of_equals is an index into self (the original slice).
            let no_kvp = KVPParseResult { remaining_slice: self, kvp: None };
            if index_of_equals == 0 { return no_kvp; };

            // We expect the character immediately before the '=' to be non-ws.
            let key_end_index = index_of_equals - 1;
            if self[key_end_index].is_whitespace() { return no_kvp; };

            // Looks like we have a valid KVP. Find the start of the key.
            let mut key_start_index = key_end_index;
            while key_start_index > 0 && !self[key_start_index].is_whitespace() {
                key_start_index -= 1;
            }
            if self[key_start_index].is_whitespace() {
                key_start_index += 1;
            }

            let key_slice = &self[key_start_index..=key_end_index];
            let remaining_slice = &self[..key_start_index];
            //println!("  >> extract_key, self={:?}, key={:?}, value={:?}, remaining={:?}",
            //    self.to_string(), key_slice.to_string(), value_slice.to_string(), remaining_slice.to_string());

            KVPParseResult {
                remaining_slice: remaining_slice,
                kvp: Some(KVP::new(key_slice, value_slice))
            }
        };

        match last_char {
            b'=' => {
                // We possibly have an empty KVP of the form 'Key='.
                let value_slice = b"";
                let index_of_equals = self.len() - 1;
                extract_key(value_slice, index_of_equals)
            },
            b'"' => {
                // We possibly a KVP of the form 'Key="some value with spaces"'.
                // Find the previous double quote (trim off the trailing double quote to make it easier).
                let search_slice = &self[0..self.len() - 1];
                let index_of_leading_double_quote = search_slice.iter().rposition(|&c| c == b'"');
                //println!("  >> Case quoted, index_of_leading_double_quote = {:?}", index_of_leading_double_quote);
                if index_of_leading_double_quote.is_none() { return no_kvp };
                let index_of_leading_double_quote = index_of_leading_double_quote.unwrap();

                let value_slice = &self[index_of_leading_double_quote + 1..self.len() - 1];
                //println!("  >> Case quoted, value_slice = {:?}", value_slice.to_string());

                if index_of_leading_double_quote == 0 {
                    // This is for when self is something like '"A message in quotes"'.
                    // See test 'for_quoted_message_only'.
                    // There are no actual log lines like this, but it can occur once leading kvps have been trimmed.
                    return no_kvp;
                }

                let index_of_equals = index_of_leading_double_quote - 1;
                // The previous value is expected to be '='.
                if self[index_of_equals] != b'=' { return no_kvp };
                extract_key(value_slice, index_of_equals)
            },
            _ => {
                // We possibly a KVP of the form 'Key=Value'. But we may also just be looking at
                // some random word. We expect to find an '=' before we hit any whitespace or a double quote.
                // Hitting a double quote first indicates a badly formed line - such as the 'unclosed_quote'
                // test cases - and we terminate with no_kvp in that case.
                let index_of_equals = self.iter().rposition(|&c| c == b'=' || c == b'"' || c.is_whitespace());
                if index_of_equals.is_none() { return no_kvp };

                // Check to see if we hit an '=' first.
                let index_of_equals = index_of_equals.unwrap();
                if self[index_of_equals] != b'=' { return no_kvp; }

                let value_slice = &self[index_of_equals + 1..];
                extract_key(value_slice, index_of_equals)
            },
        }
    }
}


#[cfg(test)]
mod kvp_collection_tests {
    use super::*;

    #[test]
    pub fn insert_does_not_add_if_strings_equal() {
        let mut sut = KVPCollection::default();
        sut.insert(KVP::new(b"car", b"ford"));
        sut.insert(KVP::new(b"car", b"volvo"));

        assert_eq!(sut.len(), 1);
        assert_eq!(sut.value(b"car"), b"ford");
    }

    #[test]
    pub fn insert_adds_if_strings_different() {
        let mut sut = KVPCollection::default();
        sut.insert(KVP::new(b"car", b"ford"));
        sut.insert(KVP::new(b"truck", b"volvo"));

        assert_eq!(sut.len(), 2);
        assert_eq!(sut.value(b"car"), b"ford");
        assert_eq!(sut.value(b"truck"), b"volvo");
    }

    #[test]
    pub fn get_value_works_case_insensitively() {
        let mut sut = KVPCollection::default();
        sut.insert(KVP::new(b"car", b"ford"));

        assert_eq!(sut.len(), 1);
        assert_eq!(sut.get_value(b"car").unwrap(), b"ford");
        assert_eq!(sut.get_value(b"Car").unwrap(), b"ford");
        assert_eq!(sut.get_value(b"XYZ"), None);
    }
}

#[cfg(test)]
mod next_kvp_tests {
    use super::*;

    #[test]
    pub fn for_empty_slice() {
        let slice = &b"";
        let result = slice.next_kvp();
        assert!(result.kvp.is_none());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_whitespace_slice() {
        let slice = &b" \r\n";
        let result = slice.next_kvp();
        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b" \r\n");
    }

    #[test]
    pub fn for_log_level_only() {
        let slice = &b"[DEBUG]";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"[DEBUG]");
        assert!(kvp.is_log_level);
        assert!(kvp.value.is_empty());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_log_level_and_cr() {
        let slice = &b"[DEBUG]\r";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"[DEBUG]");
        assert!(kvp.is_log_level);
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b"\r");
    }

    #[test]
    pub fn for_log_level_and_remainder() {
        let slice = &b"[DEBUG] | ";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"[DEBUG]");
        assert!(kvp.is_log_level);
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b" | ");
    }

    #[test]
    pub fn for_non_kvp_word_only() {
        let slice = &b"Car";
        let result = slice.next_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car");
    }

    #[test]
    pub fn for_non_kvp_word_and_cr() {
        let slice = &b"Car\r";
        let result = slice.next_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car\r");
    }

    #[test]
    pub fn for_non_kvp_word_and_remainder() {
        let slice = &b"Car REM";
        let result = slice.next_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car REM");
    }

    #[test]
    pub fn for_key_only() {
        let slice = &b"Car=";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_only_and_whitespce() {
        let slice = &b"Car= ";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b" ");
    }

    #[test]
    pub fn for_key_only_and_remainder() {
        let slice = &b"Car= REM";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b" REM");
    }

    #[test]
    pub fn for_key_only_and_cr() {
        let slice = &b"Car=\r";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b"\r");
    }

    #[test]
    pub fn for_key_and_value() {
        let slice = &b"Car=Ford";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_whitespace() {
        let slice = &b"Car=Ford ";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford");
        assert_eq!(result.remaining_slice, b" ");
    }

    #[test]
    pub fn for_key_and_value_and_remainder() {
        let slice = &b"Car=Ford REM";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford");
        assert_eq!(result.remaining_slice, b" REM");
    }

    #[test]
    pub fn for_key_and_value_and_cr() {
        let slice = &b"Car=Ford\r";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford");
        assert_eq!(result.remaining_slice, b"\r");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_only() {
        let slice = &b"Car=\"";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_whitespace() {
        let slice = &b"Car=\" ";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b" ");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder() {
        // This case turns out to be 'an unclosed double quote' case.
        let slice = &b"Car=\" REM";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b" REM");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder2() {
        let slice = &b"Car=\"For";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"For");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder3() {
        let slice = &b"Car=\"For a";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"For a");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder4() {
        let slice = &b"Car=\"For\r\n";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"For");
        assert_eq!(result.remaining_slice, b"\r\n");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_cr() {
        let slice = &b"Car=\"\r";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b"\r");
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_but_empty() {
        let slice = &b"Car=\"\"";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_but_whitespace() {
        let slice = &b"Car=\" \"";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b" ");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes() {
        let slice = &b"Car=\"Ford Fiesta\"";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford Fiesta");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_and_whitespace() {
        let slice = &b"Car=\"Ford Fiesta\" ";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford Fiesta");
        assert_eq!(result.remaining_slice, b" ");
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_and_remainder() {
        let slice = &b"Car=\"  Ford Fiesta  \" REM";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"  Ford Fiesta  ");
        assert_eq!(result.remaining_slice, b" REM");
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_and_intervening_cr() {
        let slice = &b"Car=\"  Ford\rFiesta  \"";
        let result = slice.next_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"  Ford");
        assert_eq!(result.remaining_slice, b"\rFiesta  \"");
    }
}

#[cfg(test)]
mod prev_kvp_tests {
    use super::*;

    #[test]
    pub fn for_empty_slice() {
        let slice = &b"";
        let result = slice.prev_kvp();
        assert!(result.kvp.is_none());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_non_kvp_word_only() {
        let slice = &b"Car";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car");
    }

    #[test]
    pub fn for_non_kvp_word_and_cr() {
        let slice = &b"\rCar";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"\rCar");
    }

    #[test]
    pub fn for_non_kvp_word_and_remainder() {
        let slice = &b"Car REM";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car REM");
    }

    #[test]
    pub fn for_key_only() {
        let slice = &b"Car=";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_only_and_whitespce() {
        let slice = &b" Car=";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b" ");
    }

    #[test]
    pub fn for_key_only_and_remainder() {
        let slice = &b"Car= REM";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car= REM");
    }

    #[test]
    pub fn for_key_only_and_cr() {
        let slice = &b"\rCar=";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert_eq!(result.remaining_slice, b"\r");
    }

    #[test]
    pub fn for_key_and_value_only() {
        let slice = &b"Car=Ford";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_value_and_whitespace() {
        let slice = &b" Car=Ford";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford");
        assert_eq!(result.remaining_slice, b" ");
    }

    #[test]
    pub fn for_key_and_value_and_remainder() {
        let slice = &b"REM Car=Ford";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford");
        assert_eq!(result.remaining_slice, b"REM ");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_only() {
        let slice = &b"Car=\"";
        let result = slice.prev_kvp();
        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car=\"");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_whitespace() {
        let slice = &b" Car=\"";
        let result = slice.prev_kvp();
        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b" Car=\"");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder() {
        let slice = &b"Car=\" REM";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car=\" REM");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder2() {
        let slice = &b"Car=\"For";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car=\"For");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder3() {
        let slice = &b"Car=\"For a";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car=\"For a");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_remainder4() {
        let slice = &b"\r\nCar=\"For";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"\r\nCar=\"For");
    }

    #[test]
    pub fn for_key_and_unclosed_quote_and_cr() {
        let slice = &b"\rCar=\"";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"\rCar=\"");
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_but_empty() {
        let slice = &b"Car=\"\"";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert!(kvp.value.is_empty());
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_but_whitespace() {
        let slice = &b"Car=\" \"";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b" ");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_only() {
        let slice = &b"Car=\"Ford Fiesta\"";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford Fiesta");
        assert!(result.remaining_slice.is_empty());
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_and_whitespace() {
        let slice = &b" Car=\"Ford Fiesta\"";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"Ford Fiesta");
        assert_eq!(result.remaining_slice, b" ");
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_and_remainder() {
        let slice = &b"Car=\"  Ford Fiesta  \" REM";
        let result = slice.prev_kvp();

        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice, b"Car=\"  Ford Fiesta  \" REM");
    }

    #[test]
    pub fn for_key_and_value_in_closed_quotes_and_remainder2() {
        let slice = &b"REM Car=\"  Ford Fiesta  \"";
        let result = slice.prev_kvp();

        let kvp = result.kvp.unwrap();
        assert_eq!(kvp.key, b"Car");
        assert_eq!(kvp.value, b"  Ford Fiesta  ");
        assert_eq!(result.remaining_slice, b"REM ");
    }

    #[test]
    pub fn for_quoted_message_only() {
        // This is a pathological case seen in a real log file. Once.
        let slice = &b"\"Case update sent successfully.\"";
        let result = slice.prev_kvp();
        assert!(result.kvp.is_none());
        assert_eq!(result.remaining_slice.to_vec(), b"\"Case update sent successfully.\"".to_vec());
    }
}
