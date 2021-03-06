use std::borrow::Cow;

pub trait ByteExtensions {
    fn is_whitespace(self) -> bool;
    fn is_whitespace_or_pipe(self) -> bool;
    fn is_decimal_digit(self) -> bool;
}

impl ByteExtensions for u8 {
    #[inline(always)]
    fn is_whitespace(self) -> bool {
        self == b' ' || self == b'\r' || self == b'\n' || self == b'\t'
    }

    #[inline(always)]
    fn is_whitespace_or_pipe(self) -> bool {
        self == b'|' || self.is_whitespace()
    }

    #[inline(always)]
    fn is_decimal_digit(self) -> bool {
        self >= 48 && self <= 57
    }
}

pub trait ByteSliceExtensions {
    fn trim_left(&self) -> &Self;

    fn trim_right(&self) -> &Self;

    fn trim_left_while<P>(&self, pred: P) -> &Self
        where P: Fn(u8) -> bool;

    fn trim_right_while<P>(&self, pred: P) -> &Self
        where P: Fn(u8) -> bool;

    fn trim_while<P>(&self, pred: P) -> &Self
        where P: Fn(u8) -> bool;

    fn to_string(&self) -> String;

    /// Makes the slice safe to write to CSV by checking for any embedded '\r' characters
    /// and replacing them with a space. Uses a Cow to avoid allocaions in most cases.
    //fn make_safe<'f>(&'f self) -> Cow<'f, [u8]>;
    fn make_safe(&self) -> Cow<[u8]>;
        // TODO: should be where T: std::clone::Clone, and Self is [T]
}

impl ByteSliceExtensions for [u8] {
    /// Trims one character from the left of the slice, unless the slice
    /// is empty in which case the empty slice is returned.
    fn trim_left(&self) -> &Self
    {
        if self.is_empty() { self } else { &self[1..] }
    }

    /// Trims one character from the right of the slice, unless the slice
    /// is empty in which case the empty slice is returned.
    fn trim_right(&self) -> &Self
    {
        if self.is_empty() { self } else { &self[0..self.len() - 1] }
    }

    /// Trims characters from the left of the slice while the predicate returns true,
    /// or until the slice is empty.
    fn trim_left_while<P>(&self, pred: P) -> &Self
        where P: Fn(u8) -> bool
    {
        let mut t = self;
        while !t.is_empty() && pred(t[0]) {
            t = &t[1..];
        }

        t
    }

    /// Trims characters from the right of the slice while the predicate returns true,
    /// or until the slice is empty.
    fn trim_right_while<P>(&self, pred: P) -> &Self
        where P: Fn(u8) -> bool
    {
        let mut t = self;

        while !t.is_empty() && pred(t[t.len() - 1]) {
            t = &t[..t.len() - 1];
        }

        t
    }

    /// Trims characters from both the left and right of the slice while the predicate
    /// returns true, or until the slice is empty.
    fn trim_while<P>(&self, pred: P) -> &Self
        where P: Fn(u8) -> bool
    {
        let s = self.trim_left_while(&pred);
        s.trim_right_while(pred)
    }

    /// Convert to a string, to help with debugging and testing.
    fn to_string(&self) -> String {
        String::from_utf8(self.to_vec()).unwrap()
    }

    /// Makes the slice safe to write to CSV by checking for any embedded '\r' characters
    /// and replacing them with a space. Uses a Cow to avoid allocaions in most cases.
    fn make_safe(&self) -> Cow<[u8]> {
        if self.contains(&b'\r') || self.contains(&b'\n') {
            let safe: Vec<_> = self.iter().map(|&c| if c == b'\r' || c == b'\n' { b' ' } else { c }).collect();
            safe.into()
        } else {
            self.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn trim_left() {
        let slice = &b"";
        assert_eq!(slice.trim_left(), b"");

        let slice = &b"ab";
        assert_eq!(slice.trim_left(), b"b");
    }

    #[test]
    pub fn trim_right() {
        let slice = &b"";
        assert_eq!(slice.trim_right(), b"");

        let slice = &b"ab";
        assert_eq!(slice.trim_right(), b"a");
    }

    #[test]
    pub fn trim_left_while() {
        let slice = &b"  a";
        let result = slice.trim_left_while(ByteExtensions::is_whitespace);
        assert_eq!(result, b"a");
        assert_eq!(slice, &b"  a", "Should not change the original");

        let slice = &b"";
        assert_eq!(slice.trim_left_while(ByteExtensions::is_whitespace), b"");

        let slice = &b"  ";
        assert_eq!(slice.trim_left_while(ByteExtensions::is_whitespace), b"");
    }

    #[test]
    pub fn trim_right_while() {
        let slice = &b"a  ";
        let result = slice.trim_right_while(ByteExtensions::is_whitespace);
        assert_eq!(result, b"a");
        assert_eq!(slice, &b"a  ", "Should not change the original");

        let slice = &b"";
        assert_eq!(slice.trim_right_while(ByteExtensions::is_whitespace), b"");

        let slice = &b"  ";
        assert_eq!(slice.trim_right_while(ByteExtensions::is_whitespace), b"");
    }

    #[test]
    pub fn trim_while() {
        let slice = &b"  a  ";
        let result = slice.trim_while(ByteExtensions::is_whitespace);
        assert_eq!(result, b"a");
        assert_eq!(slice, &b"  a  ", "Should not change the original");

        let slice = &b"";
        assert_eq!(slice.trim_while(ByteExtensions::is_whitespace), b"");

        let slice = &b"  ";
        assert_eq!(slice.trim_while(ByteExtensions::is_whitespace), b"");
    }
}