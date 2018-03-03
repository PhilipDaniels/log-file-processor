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
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buffer = Vec::with_capacity(1024);

        let n = self.reader.read_until(b'\r', &mut buffer).unwrap();
        if n == 0 { return None; }
        
        // We need to read one more byte here to skip the \n which is after the \r.
        let mut b2 = [0; 1];
        self.reader.read(&mut b2).expect("Should be able to read.");

        if buffer.len() < 2 { return None; }

        Some(buffer)
    }
}
