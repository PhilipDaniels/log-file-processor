use std::io::prelude::Write;

#[inline(always)]
pub fn out<T: Write>(writer: &mut T, s: &[u8]) {
    writer.write(s).unwrap();
    writer.write(b"|").unwrap();
}

#[inline(always)]
pub fn out_line<T: Write>(writer: &mut T, s: &[u8]) {
    writer.write(s).unwrap();
    writer.write(b"\r\n").unwrap();
}
