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
    // pub log_date: String,
    // pub log_level: String,
    // pub message: String,
    // pub kvps: KVPCollection
}

/// The result of parsing a line is one of these types.
pub type ParseLineResult<'f> = Result<ParsedLine2<'f>, ParsedLineError<'f>>;


impl<'f> ParsedLine2<'f> {
    pub fn new(line_num: usize, line: &[u8]) -> ParseLineResult {
        if line.is_empty() {
            return Err(ParsedLineError{ line_num, line, message: "Line is empty".to_string()});
        }

        let line = trim_start_ws(line);
        if line.is_empty() {
            return Err(ParsedLineError{ line_num, line, message: "Line is empty".to_string()});
        }

        Ok(ParsedLine2::default())
    }
}

fn trim_start_ws(line: &[u8]) -> &[u8] {
    // TODO: Based on next_none_ws
    line
}