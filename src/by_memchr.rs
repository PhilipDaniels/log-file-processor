use std::fs::File;
use std::io::{BufReader,BufWriter};
use needle::{BoyerMoore, SearchIn};
use output::*;
use logfile_iterator::LogFileIterator;
use memchr::memchr;

pub fn process_using_memchr(reader: BufReader<File>, mut writer: BufWriter<File>) {
    let source_finder = BoyerMoore::new(&b"Source="[..]);
    let ckey_finder = BoyerMoore::new(&b"CorrelationKey="[..]);
    let cret_finder = BoyerMoore::new(&b"CallRecorderExecutionTime="[..]);

    // Surprisingly, using memchr is slightly slower than the version using iterators!
    for buffer in LogFileIterator::new(reader) {
        let slice = &buffer;
        let mut a = 0;
        let mut b = memchr(b'|', slice).unwrap();
        let date = &slice[a..b-1];
        out(&mut writer, date);
        
        let slice = &slice[b + 2..];
        a = memchr(b'=', slice).unwrap();
        b = memchr(b' ', slice).unwrap();
        let machine = &slice[a + 1..b];
        out(&mut writer, machine);

        let slice = &slice[b + 3..];
        a = memchr(b'=', slice).unwrap();
        b = memchr(b' ', slice).unwrap();
        let appname = &slice[a + 1..b];
        out(&mut writer, appname);

        let slice = &slice[b + 3..];
        a = memchr(b'=', slice).unwrap();
        b = memchr(b' ', slice).unwrap();
        let pid = &slice[a + 1..b];
        out(&mut writer, pid);

        let slice = &slice[b + 3..];
        a = memchr(b'=', slice).unwrap();
        b = memchr(b' ', slice).unwrap();
        let tid = &slice[a + 1..b];
        out(&mut writer, tid);

        let slice = &slice[b + 3..];
        a = memchr(b'[', slice).unwrap();
        b = memchr(b']', slice).unwrap();
        let level = &slice[a + 1..b];
        out(&mut writer, level);

        let slice = &slice[b..];
        a = memchr(b'|', slice).unwrap();
        let slice = &slice[a + 2..];
        let rest = slice;       // Keep this for later.

        let src_start = source_finder.find_first_in(slice).expect("Could not find the Source");
        let msg = &slice[0..src_start - 1];

        let slice = &slice[src_start + "Source=".len()..];
        a = 0;
        b = memchr(b' ', slice).unwrap();
        let source = &slice[a..b];
        out(&mut writer, source);

        // CorrelationKey might not exist.
        let i = ckey_finder.find_first_in(rest);
        let ckey = match i {
            None => b"",
            Some(i) => {
                a = i + "CorrelationKey".len() + 1;
                let slice = &rest[a..];
                b = memchr(b' ', slice).unwrap_or(slice.len());
                &slice[0..b]
            }
        };
        out(&mut writer, ckey);

        // CallRecorderExecutionTime might not exist.
        let i = cret_finder.find_first_in(rest);
        let ckey = match i {
            None => b"",
            Some(i) => {
                a = i + "CallRecorderExecutionTime".len() + 1;
                let slice = &rest[a..];
                b = memchr(b' ', slice).unwrap_or(slice.len());
                &slice[0..b]
            }
        };
        out(&mut writer, ckey);

        out_line(&mut writer, msg);
    }
}
