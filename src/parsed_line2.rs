#[derive(Debug, Default)]
pub struct ParsedLineError<'f> {
    /// The zero-based line number.
    pub line_num: usize,

    /// The entire original line with whitespace trimmed from the ends.
    pub line: &'f str,

    /// A message describing the error.
    pub message: String
}

#[derive(Debug, Default)]
pub struct ParsedLine2<'f> {
    /// The zero-based line number.
    pub line_num: usize,

    /// The entire original line with whitespace trimmed from the ends.
    pub line: &'f str,
    // pub log_date: String,
    // pub log_level: String,
    // pub message: String,
    // pub kvps: KVPCollection
}

/// The result of parsing a line is one of these types.
pub type ParseLineResult<'f> = Result<ParsedLine2<'f>, ParsedLineError<'f>>;
