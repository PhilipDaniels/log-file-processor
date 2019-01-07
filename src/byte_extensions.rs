pub trait ByteExtensions {
    fn is_whitespace(self) -> bool;
    fn is_whitespace_or_pipe(self) -> bool;
    fn is_kvp_terminator(self) -> bool;
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
    fn is_kvp_terminator(self) -> bool {
        self == b'=' || self.is_whitespace()
    }
}

pub trait ByteSliceExtensions {
    fn trim_left(self) -> Self;

    fn trim_right(self) -> Self;

    fn trim_left_while<P>(self, pred: P) -> Self
        where P: Fn(u8) -> bool;

    fn trim_right_while<P>(self, pred: P) -> Self
        where P: Fn(u8) -> bool;

    fn trim_while<P>(self, pred: P) -> Self
        where P: Fn(u8) -> bool;
}

impl<'s> ByteSliceExtensions for &'s [u8] {
    /// Trims one character from the left of the slice, unless the slice
    /// is empty in which case the empty slice is returned.
    fn trim_left(self) -> Self
    {
        if self.is_empty() { self } else { &self[1..] }
    }

    /// Trims one character from the right of the slice, unless the slice
    /// is empty in which case the empty slice is returned.
    fn trim_right(self) -> Self
    {
        if self.is_empty() { self } else { &self[0..self.len() - 1] }
    }

    /// Trims characters from the left of the slice while the predicate returns true,
    /// or until the slice is empty.
    fn trim_left_while<P>(mut self, pred: P) -> Self
        where P: Fn(u8) -> bool
    {
        while !self.is_empty() && pred(self[0]) {
            self = &self[1..];
        }

        self
    }

    /// Trims characters from the right of the slice while the predicate returns true,
    /// or until the slice is empty.
    fn trim_right_while<P>(mut self, pred: P) -> Self
        where P: Fn(u8) -> bool
    {
        while !self.is_empty() && pred(self[self.len() - 1]) {
            self = &self[..self.len() - 1];
        }

        self
    }

    /// Trims characters from both the left and right of the slice while the predicate
    /// returns true, or until the slice is empty.
    fn trim_while<P>(self, pred: P) -> Self
        where P: Fn(u8) -> bool
    {
        self.trim_left_while(&pred).trim_right_while(pred)
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