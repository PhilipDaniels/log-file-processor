use std::fs::File;
use std::io::{BufReader,BufWriter};
use std::io::prelude::Write;
use std::thread;
use std::sync::mpsc::channel;
use needle;
use fast_logfile_iterator;
use parse_using_iterators::*;

pub fn process_using_channels(reader: BufReader<File>, mut writer: BufWriter<File>) {
    let source_finder = needle::BoyerMoore::new(&b"Source="[..]);
    let ckey_finder = needle::BoyerMoore::new(&b"CorrelationKey="[..]);
    let cret_finder = needle::BoyerMoore::new(&b"CallRecorderExecutionTime="[..]);

    let (tx_lines, rx_lines) = channel();
    let (tx_lines2, rx_lines2) = channel();
    let (tx_lines3, rx_lines3) = channel();
    let (tx_lines4, rx_lines4) = channel();
    let (tx_lines5, rx_lines5) = channel();

    // This basically handles reading the line into a new Vec.
    let handle1 = thread::spawn(move || {
        for buffer in fast_logfile_iterator::FastLogFileIterator::new(reader) {
            if tx_lines.send(buffer).is_err() {
                break;
            }
        }
    });

    // Then a second thread takes that Vector and parses out the main components
    // and passes them on.
    let handle2 = thread::spawn(move || {
        for line in rx_lines {
            let parsed_data = parse_main_block(&line);
            let result = (line, parsed_data);
            if tx_lines2.send(result).is_err() {
                break;
            }
        }
    });

    // The next thread parses out the Source and Message fields.
    let handle3 = thread::spawn(move || {
        for (line, mut parsed_data) in rx_lines2 {
            parse_source_and_msg(&line, &mut parsed_data, &source_finder);

            if tx_lines3.send((line, parsed_data)).is_err() {
                break;
            }
        }
    });

    // The next thread parses out the CallRecorderExecutionTime and CorrelationKey.
    let handle4 = thread::spawn(move || {
        for (line, mut parsed_data) in rx_lines3 {
            parse_ckey_and_cret(&line, &mut parsed_data, &ckey_finder, &cret_finder);

            if tx_lines4.send((line, parsed_data)).is_err() {
                break;
            }
        }
    });

    // The next thread gathers all the components into a single Vec so that we
    // can output them with one IO call rather than 10. It doesn't actually
    // make any noticeable difference in performance. I still like it though.
    let handle5 = thread::spawn(move || {
        for (line, mut parsed_data) in rx_lines4 {
            let mut v : Vec<u8> = Vec::with_capacity(256);

            v.extend_from_slice(&line[parsed_data.date.0..parsed_data.date.1]);
            v.push(b'|');
            v.extend_from_slice(&line[parsed_data.machine.0..parsed_data.machine.1]);
            v.push(b'|');
            v.extend_from_slice(&line[parsed_data.appname.0..parsed_data.appname.1]);
            v.push(b'|');
            v.extend_from_slice(&line[parsed_data.pid.0..parsed_data.pid.1]);
            v.push(b'|');
            v.extend_from_slice(&line[parsed_data.tid.0..parsed_data.tid.1]);
            v.push(b'|');
            v.extend_from_slice(&line[parsed_data.level.0..parsed_data.level.1]);
            v.push(b'|');
            v.extend_from_slice(&line[parsed_data.source.0..parsed_data.source.1]);
            v.push(b'|');

            if let Some((a, b)) = parsed_data.ckey {
                v.extend_from_slice(&line[a..b]);
            }
            v.push(b'|');

            if let Some((a, b)) = parsed_data.cret {
                v.extend_from_slice(&line[a..b]);
            }
            v.push(b'|');

            // The message can contain embedded \n characters, we need to remove them.
            let s = &line[parsed_data.message.0..parsed_data.message.1];
            let m = s.iter().filter_map(|&x| if x != b'\n' { Some(x) } else { None });
            v.extend(m);
            v.push(b'\r');
            v.push(b'\n');

            if tx_lines5.send(v).is_err() {
                break;
            }
        }
    });

    // The final thread just writes out the vec. It might be tempting to get rid
    // of this thread and just move the call to write() into the previous thread
    // but the runtime more than doubles if you do that.
    let handle6 = thread::spawn(move || {
        for line in rx_lines5 {
            writer.write(&line).unwrap();
        }
    });

    handle1.join().unwrap();
    handle2.join().unwrap();
    handle3.join().unwrap();
    handle4.join().unwrap();
    handle5.join().unwrap();
    handle6.join().unwrap();
}
