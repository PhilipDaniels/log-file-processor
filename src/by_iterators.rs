extern crate needle;

use std::fs::File;
use std::io::{BufReader,BufWriter};
use needle::{BoyerMoore, SearchIn};
use output::*;
use logfile_iterator::LogFileIterator;

pub fn process_using_iterators(reader: BufReader<File>, mut writer: BufWriter<File>) {
    let source_finder = BoyerMoore::new(&b"Source="[..]);
    let ckey_finder = BoyerMoore::new(&b"CorrelationKey="[..]);
    let cret_finder = BoyerMoore::new(&b"CallRecorderExecutionTime="[..]);

    // Now we are using my custom iterator. Amazingly, even though this
    // allocates a new Vec each time, it is just as quick (to within 0.01 seconds)
    // as the "raw_indexing" method!
    for buffer in LogFileIterator::new(reader) {
        let mut it = buffer.iter();
        let mut a = 0;
        let mut b = it.position(|&x| x == b'|').unwrap();
        let date = &buffer[0..b-1];
        out(&mut writer, date);
        
        a += 2 + b + it.position(|&x| x == b'=').unwrap();
        b = a + it.position(|&x| x == b' ').unwrap();
        let machine = &buffer[a..b];
        out(&mut writer, machine);

        a = 2 + b + it.position(|&x| x == b'=').unwrap();
        b = a + it.position(|&x| x == b' ').unwrap();
        let appname = &buffer[a..b];
        out(&mut writer, appname);

        a = 2 + b + it.position(|&x| x == b'=').unwrap();
        b = a + it.position(|&x| x == b' ').unwrap();
        let pid = &buffer[a..b];
        out(&mut writer, pid);

        a = 2 + b + it.position(|&x| x == b'=').unwrap();
        b = a + it.position(|&x| x == b' ').unwrap();
        let tid = &buffer[a..b];
        out(&mut writer, tid);

        a = 2 + b + it.position(|&x| x == b'[').unwrap();
        b = a + it.position(|&x| x == b']').unwrap();
        let level = &buffer[a..b];
        out(&mut writer, level);

        a = 3 + b + it.position(|&x| x == b'|').unwrap();
        let rest = &buffer[a..];

        let src_start = source_finder.find_first_in(rest).expect("Could not find the Source");
        let msg = &rest[0..src_start - 1];

        a = src_start + "Source=".len();
        let mut it = rest[a..].iter();
        b = a + it.position(|&x| x == b' ').unwrap();
        let source = &rest[a..b];
        out(&mut writer, source);

        // CorrelationKey might not exist.
        let i = ckey_finder.find_first_in(rest);
        let ckey = match i {
            None => b"",
            Some(i) => {
                let a = i + "CorrelationKey".len() + 1;
                let mut it = rest[a..].iter();
                let b = a + it.position(|&x| x == b' ').unwrap();
                &rest[a..b]
            }
        };
        out(&mut writer, ckey);

        // CallRecorderExecutionTime might not exist.
        let i = cret_finder.find_first_in(rest);
        let cret = match i {
            None => b"",
            Some(i) => {
                let a = i + "CallRecorderExecutionTime".len() + 1;
                let mut it = rest[a..].iter();
                // If position returns None, end of line has occurred.
                match it.position(|&x| x == b' ') {
                    None => &rest[a..],
                    Some(i) => &rest[a..i]
                }
            }
        };
        out(&mut writer, cret);

        out_line(&mut writer, msg);
    }
}
