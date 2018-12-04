use std::io;

pub struct FastLogFileIterator<T> {
    reader: T
}

impl<'a, T> FastLogFileIterator<T> {
    pub fn new(reader: T) -> FastLogFileIterator<T> {
        FastLogFileIterator { 
            reader: reader
        }
    }
}

impl <T: io::BufRead> Iterator for FastLogFileIterator<T> {
    type Item = (u64, String);

    fn next(&mut self) -> Option<Self::Item> {
        let mut buffer = Vec::with_capacity(1024);
        
        let n = self.reader.read_until(b'\r', &mut buffer).unwrap();
        if n == 0 { return None; }

        // We need to read one more byte here to skip the \n which is after the \r.
        let mut b2 = [0; 1];
        self.reader.read(&mut b2).expect("Should be able to read to skip the \\n byte.");

        if buffer.len() < 2 { return None; }

        let bytes_read = buffer.len() + 1;

        // Remove the trailing \r from the first read.
        buffer.pop();

        // We assume we can convert to UTF-8. Makes downstream usage easier.
        let s = String::from_utf8(buffer).expect("Found an invalid UTF-8 sequence.");

        Some((bytes_read as u64, s))
    }
}
