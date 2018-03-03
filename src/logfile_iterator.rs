use std::io::{BufRead};

pub struct LogFileIterator<T> {
    // The thing we are reading from.
    reader: T,

    // A buffer containing one line, purged of \r and \n characters.
    // The intention was to keep one buffer, and return a slice into it
    // thus avoiding allocation (in the same way that the raw read_until loop
    // avoids allocations. However, I could not figure out the lifetimes.)
    // By returning a Vec, we avoid lifetimes and it is still just as fast!
    // If you check the lines iterator in the Rust source, it does the
    // same thing (allocates a new Vec for each line).
    //buffer: Vec<u8> 
}

impl<'a, T> LogFileIterator<T> {
    pub fn new(reader: T) -> LogFileIterator<T> {
        LogFileIterator { 
            reader: reader
        }
    }
}

impl <T: BufRead> Iterator for LogFileIterator<T> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buffer = Vec::with_capacity(1024);
        buffer.clear();

        let n = self.reader.read_until(b'\r', &mut buffer).unwrap();
        if n == 0 { return None; }
        
        // Remove \r and \n. This is surprisingly slow, but can't be avoided.
        buffer.retain(|&x| x != b'\r' && x != b'\n');

        if buffer.len() < 2 { return None; }

        Some(buffer)
    }
}
