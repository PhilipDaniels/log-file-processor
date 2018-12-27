/// This module contains the representation of a Key-Value pair as parsed from the original line,
/// and some utility methods for doing that parsing.

use std::borrow::Cow;
use crate::parse_utils::*;

/// A Vec is probably as fast as a HashMap for the small number of KVPs we expect to see.
#[derive(Debug, Default)]
pub struct KVPCollection<'t> {
    kvps: Vec<KVPStrings<'t>>
}

impl<'t> KVPCollection<'t> {
    /// Insert a new KVP, but only if it does not already exist.
    pub fn insert(&mut self, new_kvp: KVPStrings<'t>) {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(new_kvp.key) {
                return;
            }
        }

        self.kvps.push(new_kvp);
    }

    /// Gets a value, looking it up case-insensitively by the specified key.
    /// Returns None if there is no value for that key.
    pub fn get_value(&self, key: &str) -> Option<&str> {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(key) {
                return Some(&kvp.value);
            }
        }

        None
    }

    /// Gets a value, looking it up case-insensitively by the specified key.
    /// Panics if the key is not in the collection. Helps keep tests short.
    #[cfg(test)]
    pub fn value(&self, key: &str) -> &str {
        for kvp in &self.kvps {
            if kvp.key.eq_ignore_ascii_case(key) {
                return &kvp.value;
            }
        }

        panic!("No value found for key {}", key)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.kvps.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.kvps.is_empty()
    }
}

/// Represents a KVP as string slices from the original line.
/// The value may be an original slice, or it may be a String if it required
/// cleanup, e.g. if the original value contained embedded newlines.
#[derive(Debug, Default)]
pub struct KVPStrings<'t> {
    pub key: &'t str,
    pub value: Cow<'t, str>
}

/// Represents a KVP as it is parsed out of a line. This is low-level information
/// and is used to construct a `KVPStrings` object.
#[derive(Debug, Default)]
pub struct KVPParseData {
    key_start_index: usize,
    key_end_index: usize,
    value_start_index: usize,
    value_end_index: usize,
    pub is_log_level: bool,
    pub value_is_quoted: bool
}

impl KVPParseData {
    pub fn has_key(&self) -> bool {
        self.key_end_index >= self.key_start_index
    }

    pub fn has_value(&self) -> bool {
        self.value_start_index > self.key_end_index && self.value_end_index >= self.value_start_index
    }

    pub fn get_kvp_strings<'t>(&self, line: &'t str, chars: &[(usize, char)]) -> KVPStrings<'t> {
        let key = unchecked_slice(line, &chars, self.key_start_index, self.key_end_index);
        let value = if self.has_value() {
            checked_slice(line, &chars, self.value_start_index, self.value_end_index)
        } else {
            "".into()
        };

        KVPStrings { key, value }
    }

    pub fn max_index(&self) -> usize {
        if self.has_value() {
            if self.value_is_quoted {
                self.value_end_index + 1
            } else {
                self.value_end_index
            }
        } else {
            self.key_end_index + 1
        }
    }

    pub fn min_index(&self) -> usize {
        self.key_start_index
    }
}

/// Attempts to extract a Key-Value pair starting at current and reading forward. There are
/// several possible forms of a KVP:
///
///     Key=
///     Key=Value
///     Key="Value with space"
///
/// These forms are guaranteed by the logging framework. In particular, there is guaranteed
/// to be no space around the '=', and the value will be wrapped in double quotes if it has
/// a quote or a space in it. 'Key' may contain '.', as in "HttpRequest.QueryString".
///
/// Pre: current is already on the start character ('K' in the above example) and limit
/// is at least at the end of the KVP expression.
pub fn next_kvp(chars: &[(usize, char)], current: usize, limit: usize) -> Option<KVPParseData> {
    debug_assert!(current < chars.len(), "current = {}, chars.len() = {}", current, chars.len());
    debug_assert!(limit < chars.len(), "limit = {}, chars.len() = {}", limit, chars.len());
    debug_assert!(current <= limit, "current = {}, limit = {}", current, limit);

    let key_start_index = current;
    let index_of_kvp_terminator = next(chars, key_start_index, limit, |c| !char_is_kvp_terminator(c));

    // Did we actually hit a non-equals first? In which case we do not have a KVP.
    // This may be the log-level in the prologue. Add some code to check the slice
    // is one of the log-levels and if so return it as the key.
    // The unwrap_or is when the message ends with the log level such as "...[INFO]".
    let index_of_kvp_terminator = index_of_kvp_terminator.unwrap_or(limit);
    if chars[index_of_kvp_terminator].1 != '=' {
        let possible_log_level: String = chars[current..].iter().map(|(_, c)| c).take(7).collect();
        if LOG_LEVELS.contains(&possible_log_level.as_str()) {
            // println!(">>>>> Returning Log Level {:?}", possible_log_level);
            let key_end_index = if index_of_kvp_terminator == limit { index_of_kvp_terminator } else { index_of_kvp_terminator - 1 };
            return Some(KVPParseData {
                key_start_index: key_start_index,
                key_end_index: key_end_index,
                is_log_level: true,
                .. KVPParseData::default() })
        };

        return None;
    }

    debug_assert!(chars[index_of_kvp_terminator].1 == '=', "If we are not looking at a space, we must be looking at an equals sign");
    let key_end_index = index_of_kvp_terminator - 1;

    // The value should start immediately after the '='.
    let value_start_index = inc(chars, index_of_kvp_terminator);
    if value_start_index.is_none() {
        // This is the pathological case where we reached the end of the input such as: "....Key="
        // In practice we should never reach here except with badly formed lines because such trailing
        // KVPs should be consumed by `prev_kvp`.
        return None;
    }
    let mut value_start_index = value_start_index.unwrap();

    // Now we can start looking for the value. The value may be a simple word, or it may be in double quotes.
    let mut value_is_quoted = false;
    let mut value_end_index = 0;
    if chars[value_start_index].1 == '"' {
        // Strip off the leading quote.
        let idx = inc(&chars, value_start_index);
        if idx.is_none() {
            return None;
        }
        value_start_index = idx.unwrap();

        let idx = next(chars, value_start_index, limit, |c| c != '"');
        if idx.is_none() {
            return None;
        }
        value_end_index = idx.unwrap() - 1;
        value_is_quoted = true;
    } else if char_is_whitespace(chars[value_start_index].1) {
        // We have an empty value ("Key= "). Make an empty slice.
        value_start_index = 0;
    } else {
        // We have a KVP of the form "Key=Value". Find the next whitespace character.
        value_end_index = next_ws(chars, value_start_index, limit).unwrap_or(limit) - 1;
    }

    Some(KVPParseData {
        key_start_index,
        key_end_index,
        value_start_index,
        value_end_index,
        value_is_quoted,
        .. KVPParseData::default() })
}

/// Attempts to extract a Key-Value pair starting at current and reading backwards. There are
/// several possible forms of a KVP:
///
///     Key=
///     Key=Value
///     Key="Value with space"
///
/// These forms are guaranteed by the logging framework. In particular, there is guaranteed
/// to be no space around the '=', and the value will be wrapped in double quotes if it has
/// a quote or a space in it. 'Key' may contain '.', as in "HttpRequest.QueryString".
///
/// Pre: current is already on the end character ('=', 'e' or '"' in the above examples) and
/// limit is at least at the beginning of the KVP expression.
pub fn prev_kvp(chars: &[(usize, char)], current: usize, limit: usize) -> Option<KVPParseData> {
    debug_assert!(current < chars.len(), "current = {}, chars.len() = {}", current, chars.len());
    debug_assert!(limit < chars.len(), "limit = {}, chars.len() = {}", limit, chars.len());
    debug_assert!(current >= limit, "current = {}, limit = {}", current, limit);

    fn extract_key(chars: &[(usize, char)], index_of_equals: usize, limit: usize, value_start_index: usize, value_end_index: usize) -> Option<KVPParseData> {
        // We expect the character immediately before the '=' to be non-ws.
        let key_end_index = index_of_equals - 1;
        if char_is_whitespace(chars[key_end_index].1) {
            return None;
        }

        let key_start_index = prev_ws(chars, key_end_index, limit);
        if key_start_index.is_none() {
            // TODO: This probably should never happen except in badly formed log files.
            return None;
        }
        let key_start_index = key_start_index.unwrap() + 1;

        return Some(KVPParseData {key_start_index, key_end_index, value_start_index, value_end_index, .. KVPParseData::default()})
    }


    let mut value_end_index = current;
    let current_char = chars[current].1;
    if char_is_whitespace(current_char) {
        return None;
    }

    match current_char {
        '=' => {
            // We possibly have an empty KVP of the form 'Key='.
            extract_key(chars, current, limit, 0, 0)
        },

        '"' => {
            // We possibly a KVP of the form 'Key="some value with spaces"'.
            // First trim off the trailing quote.
            value_end_index -= 1;

            // Find the previous double quote.
            let value_start_index = prev(chars, value_end_index, limit, |c| c != '"');
            if value_start_index.is_none() {
                // This indicates a badly formed line.
                return None;
            }

            // Trim the leading quote.
            let value_start_index = value_start_index.unwrap() + 1;
            let index_of_equals = value_start_index - 2;
            if chars[index_of_equals].1 != '=' {
                return None;
            }
            extract_key(chars, index_of_equals, limit, value_start_index, value_end_index)
        },
        _ => {
            // We possibly a KVP of the form 'Key=Value'. But we may also just be looking at
            // some random word. We expect to find an '=' before we hit any whitespace.
            let index_of_equals = prev(chars, current, limit, |c| !(c == '=' || char_is_whitespace(c)));
            if index_of_equals.is_none() {
                // This indicates a badly formed line.
                return None;
            }

            // Check to see if we hit an '=' first.
            let index_of_equals = index_of_equals.unwrap();
            if chars[index_of_equals].1 != '=' {
                return None;
            }

            let value_start_index = index_of_equals + 1;
            extract_key(chars, index_of_equals, limit, value_start_index, value_end_index)
        }
    }
}

#[cfg(test)]
mod kvp_collection_tests {
    use super::*;

    #[test]
    pub fn insert_does_not_add_if_strings_equal() {
        let mut sut = KVPCollection::default();
        sut.insert(KVPStrings { key: "car", value: "ford".into() });
        sut.insert(KVPStrings { key: "car", value: "volvo".into() });

        assert_eq!(sut.len(), 1);
        assert_eq!(sut.value("car"), "ford");
    }

    #[test]
    pub fn insert_adds_if_strings_different() {
        let mut sut = KVPCollection::default();
        sut.insert(KVPStrings { key: "car", value: "ford".into() });
        sut.insert(KVPStrings { key: "truck", value: "volvo".into() });

        assert_eq!(sut.len(), 2);
        assert_eq!(sut.value("car"), "ford");
        assert_eq!(sut.value("truck"), "volvo");
    }

    #[test]
    pub fn get_value_works_case_insensitively() {
        let mut sut = KVPCollection::default();
        sut.insert(KVPStrings { key: "car", value: "ford".into() });

        assert_eq!(sut.len(), 1);
        assert_eq!(sut.value("car"), "ford");
        assert_eq!(sut.value("Car"), "ford");
    }
}
