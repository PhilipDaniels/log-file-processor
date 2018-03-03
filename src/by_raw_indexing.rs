use std::fs::File;
use std::io::{BufReader,BufRead,BufWriter};
use needle::{BoyerMoore, SearchIn};
use output::*;

pub fn process_using_raw_indexing(mut reader: BufReader<File>, mut writer: BufWriter<File>) {
    let source_finder = BoyerMoore::new(&b"Source="[..]);
    let ckey_finder = BoyerMoore::new(&b"CorrelationKey="[..]);
    let cret_finder = BoyerMoore::new(&b"CallRecorderExecutionTime="[..]);

    let mut buffer = Vec::new();

    // Read the file in chunks separated by '\r'.
    while let Ok(n) = reader.read_until(b'\r', &mut buffer) {
        if n == 0 { break; }

        // Remove \r and \n.
        buffer.retain(|&x| x != b'\r' && x != b'\n');

        // Exit if blank line detected.
        if buffer.len() < 2 { break; };

        {
            let i = find_end_of_data(&buffer, 0);
            let date = &buffer[0..i + 1];
            out(&mut writer, date);

            let (machine, i) = find_token_bounds(&buffer, i);
            out(&mut writer, machine);

            let (appname, i) = find_token_bounds(&buffer, i);
            out(&mut writer, appname);

            let (pid, i) = find_token_bounds(&buffer, i);
            out(&mut writer, pid);

            let (tid, i) = find_token_bounds(&buffer, i);
            out(&mut writer, tid);

            let i = find_start_of_data(&buffer, i);
            let j = find_end_of_data(&buffer, i);
            let log_level = &buffer[i..j + 1];
            out(&mut writer, log_level);

            let j = find_start_of_data(&buffer, j + 1);
            let rest = &buffer[j..];
            let i = source_finder.find_first_in(rest).expect("Could not find the Source");
            let msg = &rest[0..i - 1];

            let j = find(&rest, i, b' ');
            let source = &rest[i + 7..j];
            out(&mut writer, source);

            // CorrelationKey might not exist.
            let i = ckey_finder.find_first_in(rest);
            let ckey = match i {
                None => b"",
                Some(i) => {
                    let j = find(&rest, i, b' ');
                    &rest[i + 15..j]
                }
            };
            out(&mut writer, ckey);

            // CallRecorderExecutionTime might not exist.
            let i = cret_finder.find_first_in(rest);
            let cret = match i {
                None => b"",
                Some(i) => {
                    let j = find(&rest, i, b' ');
                    &rest[i + 26..j]
                }
            };
            out(&mut writer, cret);
            
            out_line(&mut writer, msg);
        }

        buffer.clear();
    }
}

#[inline(always)]
fn find(buffer: &[u8], start: usize, needle: u8) -> usize {
    let mut i = start;
    while i < buffer.len() && buffer[i] != needle {
        i += 1;
    }
    i
}

#[inline(always)]
fn go_back_to_data(buffer: &[u8], start: usize) -> usize {
    let mut i = start;
    while buffer[i] == b' ' || buffer[i] == b'|' {
        i -= 1;
    }
    i
}

#[inline(always)]
fn find_start_of_data(buffer: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < buffer.len() && (buffer[i] == b' ' || buffer[i] == b'|') {
        i += 1;
    }
    i
}

fn find_end_of_data(buffer: &[u8], start: usize) -> usize {
    let i = find(&buffer, start, b'|');
    go_back_to_data(&buffer, i)
}

fn find_token_bounds(buffer: &[u8], start: usize) -> (&[u8], usize) {
    let i = find(&buffer, start, b'=');
    let j = find_end_of_data(&buffer, i) + 1;
    let v = &buffer[i + 1..j];
    (v, j)
}
