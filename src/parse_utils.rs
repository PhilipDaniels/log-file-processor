/// This module contains low-level parsing functions that typically operate by
/// moving forwards and backwards over the line one character at a time.
/// One of the key differences between these functions and the built-in equivalents
/// is that these functions take a 'limit' parameter.

use std::borrow::Cow;

/// The set of possible log level emitted by the Fundamentals logging framework.
/// They are ordered by frequency of occurence, as this should give a (very small!)
/// performance boost when checking for them.
pub const LOG_LEVELS: [&'static str; 9] =
[
    "[INFO_]",
    "[DEBUG]",
    "[VRBSE]",
    "[WARNG]",
    "[ERROR]",
    "[FATAL]",
    "[UNDEF]",
    "[DEBG2]",
    "[DEBG1]",
];


/// The name of the built-in LogDate column.
pub const LOG_DATE: &str = "LogDate";

/// The name of the built-in LogLevel column.
pub const LOG_LEVEL: &str = "LogLevel";

/// The name of the built-in Message column.
pub const MESSAGE: &str = "Message";


/// A custom function corresponding to our definition of whitespace.
pub fn char_is_whitespace(c: char) -> bool {
    c == ' ' || c == '\r' || c == '\n' || c == '\t'
}

/// A function to be used when parsing KVPs.
pub fn char_is_kvp_terminator(c: char) -> bool {
    c == '=' || char_is_whitespace(c)
}

/// Increment index.
/// Returns. An error if the increment makes index exceed the last valid index of chars,
/// otherwise the new index.
#[inline(always)]
pub fn inc(chars: &[(usize, char)], mut index: usize) -> Option<usize> {
    index += 1;
    if index >= chars.len() {
        None
    } else {
        Some(index)
    }
}

/// Makes a string safe for CSV by replacing \r and \n with spaces.
pub fn safe_string(value: &str) -> String {
    value.replace(|c| c == '\r' || c == '\n', " ")
}

/// A convenience function for slicing into the original line, which ensures that we do
/// it correctly - start and end are indices into the chars[] array, NOT the line.
/// We get the actual bounds from the .0 element of the chars tuples.
/// 'unchecked' means that no checking is done for embedded new lines.
#[inline(always)]
pub fn unchecked_slice<'t>(line: &'t str, chars: &[(usize, char)], start: usize, end: usize) -> &'t str {
    &line[chars[start].0 ..= chars[end].0]
}

/// A convenience function for slicing into the original line, which ensures that we do
/// it correctly - start and end are indices into the chars[] array, NOT the line.
/// We get the actual bounds from the .0 element of the chars tuples.
/// 'checked' means that any embedded \r or \n characters will be replaced with spaces.
#[inline(always)]
pub fn checked_slice<'t>(line: &'t str, chars: &[(usize, char)], start: usize, end: usize) -> Cow<'t, str> {
    let slice = unchecked_slice(line, chars, start, end);
    if slice.contains(|c| c == '\r' || c == '\n') {
        Cow::Owned(safe_string(slice))
    } else {
        Cow::Borrowed(slice)
    }
}

// While PRED(c) is true, the index is advanced.
// So in effect, p is a function that says 'if c matches this, keep going'
#[inline(always)]
pub fn next<PRED>(chars: &[(usize, char)], mut current: usize, limit: usize, pred: PRED) -> Option<usize>
    where PRED: Fn(char) -> bool
{
    debug_assert!(current < chars.len(), "current = {}, chars.len() = {}", current, chars.len());
    debug_assert!(limit < chars.len(), "limit = {}, chars.len() = {}", limit, chars.len());
    debug_assert!(current <= limit, "current = {}, limit = {}", current, limit);

    while current < limit && pred(chars[current].1) {
        current += 1;
    }

    if pred(chars[current].1) { None } else { Some(current) }
}

// While PRED(c) is true, the index is shrunk.
// So in effect, p is a function that says 'if c matches this, keep going'
#[inline(always)]
pub fn prev<PRED>(chars: &[(usize, char)], mut current: usize, limit: usize, pred: PRED) -> Option<usize>
    where PRED: Fn(char) -> bool
{
    debug_assert!(current < chars.len(), "current = {}, chars.len() = {}", current, chars.len());
    debug_assert!(limit < chars.len(), "limit = {}, chars.len() = {}", limit, chars.len());
    debug_assert!(current >= limit, "current = {}, limit = {}", current, limit);

    while current > limit && pred(chars[current].1) {
        current -= 1;
    }

    if pred(chars[current].1) { None } else { Some(current) }
}

/// Pre: current and limit are valid indexes into chars[].
/// Returns: None if a ws character cannot be found within the limited range,
/// otherwise Ok(n) where n will be on the first whitespace character.
#[inline(always)]
pub fn next_ws(chars: &[(usize, char)], current: usize, limit: usize) -> Option<usize> {
    next(chars, current, limit, |c| !char_is_whitespace(c))
}

/// Pre: current and limit are valid indexes into chars[].
/// Returns: None if a non-ws character cannot be found within the limited range,
/// otherwise Ok(n) where n will be on the first non-whitespace character.
#[inline(always)]
pub fn next_none_ws(chars: &[(usize, char)], current: usize, limit: usize) -> Option<usize> {
    next(chars, current, limit, char_is_whitespace)
}

/// Like `next_none_ws`, but also skips pipe characters ('|'). Used when extracting the prologue.
#[inline(always)]
pub fn next_none_ws_or_pipe(chars: &[(usize, char)], current: usize, limit: usize) -> Option<usize> {
    next(chars, current, limit, |c| char_is_whitespace(c) || c == '|')
}

/// Pre: chars[current] has already been dealt with (e.g. it may be the inclusive end of a word).
/// Like `next_none_ws_or_pipe`, but always tries to move on to the next character.
#[inline(always)]
pub fn next_none_ws_or_pipe_after(chars: &[(usize, char)], current: usize, limit: usize) -> Option<usize> {
    let potential_next = inc(chars, current);
    if potential_next.is_none() {
        return None;
    }

    next_none_ws_or_pipe(chars, potential_next.unwrap(), limit)
}

/// Pre: current and limit are valid indexes into chars[].
/// Returns: None if a non-ws character cannot be found within the limited range,
/// otherwise Ok(n) where n will be on the last non-whitespace character.
#[inline(always)]
pub fn prev_none_ws(chars: &[(usize, char)], current: usize, limit: usize) -> Option<usize> {
    prev(chars, current, limit, char_is_whitespace)
}

/// Pre: current and limit are valid indexes into chars[].
/// Returns: None if a ws character cannot be found within the limited range,
/// otherwise Ok(n) where n will be on the first whitespace character.
#[inline(always)]
pub fn prev_ws(chars: &[(usize, char)], current: usize, limit: usize) -> Option<usize> {
    prev(chars, current, limit, |c| !char_is_whitespace(c))
}

/// Because we are using inclusive ranges, we need to add 1 to the difference
/// in order to calculate the number of characters still available. e.g.
/// For "2018" with current of 0 and limit of 3 (the '8'), there are 3 + 1 - 0
/// characters available.
/// Returns: the number of characters available within the window.
#[inline(always)]
pub fn num_chars_available(current: usize, limit: usize) -> usize {
    (limit + 1) - current
}
