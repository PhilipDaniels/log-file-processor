use std::str;
use crate::byte_extensions::{ByteExtensions, ByteSliceExtensions};

/// This module contains the representation of a Key-Value pair as parsed from the original line,
/// and some utility methods for doing that parsing.

/// The set of possible log level emitted by the Fundamentals logging framework.
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


/// Represents a single Key-Value pair as parsed from the log line.
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

        if self.len() == 0 { return no_kvp; };

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
    /// '"' in the above examples.
    fn prev_kvp(self) -> KVPParseResult<'s> {
        KVPParseResult::default()
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

// &b""
// &b" \r\n"

// b"[DEBUG]"
// b"[DEBUG]\n"
// b"[DEBUG] | "

// b"Car"
// b"Car\r"
// b"Car REM"

// b"Car="
// b"Car= "
// b"Car= REM"
// b"Car=\r"

// b"Car=Ford"
// b"Car=Ford "
// b"Car=Ford REM"
// b"Car=Ford\r"

// b"Car=\""
// b"Car=\" "
// b"Car=\" REM"
// b"Car=\"For"
// b"Car=\"For a"
// b"Car=\"For\r\n"
// b"Car=\"\r"

// b"Car=\"\""
// b"Car=\" \""
// b"Car=\"Ford Fiesta\"
// b"Car=\"  Ford Fiesta  \" REM"
// b"Car=\"  Ford\rFiesta  \""

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
