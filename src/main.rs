extern crate regex;
extern crate needle;
extern crate elapsed;
extern crate memchr;

mod output;
mod logfile_iterator;
mod fast_logfile_iterator;
mod by_raw_indexing;
mod by_iterators;
mod by_memchr;
mod by_channels;
mod parse_using_iterators;

use elapsed::measure_time;
use std::fs::File;
use std::io::{BufReader,BufWriter};
use std::io::prelude::Write;
use by_raw_indexing::process_using_raw_indexing;
use by_iterators::process_using_iterators;
use by_memchr::process_using_memchr;
use by_channels::process_using_channels;

// The log files this program is designed to parse consist of lines separated by \r\n, however \n
// may appear in the middle of lines as a logical separator. This odd format was designed for clear
// ingest into Splunk. Everything up to the log level ([INFO_]) always appears, as does the Source
// CorrelationKey and CallRecorderExecutionTime are optional.
// 
// 2018-01-23 09:12:32.9869213 | MachineName=name.of.computer | AppName=Something.Host | pid=4696 | tid=13 | [INFO_] | SOME MESSAGE BEFORE SOURCE Source=SysRefManager ... CorrelationKey=6b9e8da8-8f84-46e1-9c7d-66f6ad1ad9d7 FURTHER KVPs...
// 2018-01-23 09:12:32.9899258 | MachineName=name.of.computer | AppName=Something.Host | pid=4696 | tid=13 | [VRBSE] | SOME MESSAGE BEFORE SOURCE Source=PervasiveCaseRepository ... CorrelationKey=6b9e8da8-8f84-46e1-9c7d-66f6ad1ad9d7  FURTHER KVPs... CallRecorderExecutionTime=0

// Conclusions
// * A 2.5GB file is processed in about 5 secs by the fastest algorithm, process_using_channels.
//   It takes Cygwin 'cp' about 30secs just to copy this file - so this is very good performance.
//   The difference is accounted for by the fact that the output file that this program writes
//   is about 20% of the size of the original. (Windows Explorer can copy the file in about
//   14 seconds though.)
// * I was able to shave about 1 second off the total time by optimising the stripping of \n
//   from the message (avoiding an intermediate vector, and using filter_map() instead of filter()
//   and map()).
// * Multi-threading is quite easy to get working.
// * When using channels, you must move variables into the closures. This is why I pass a struct
//   of usizes rather than slices (to avoid lifetimes issues) and do all the writing in the last
//   thread (to avoid moving the writer into more than one thread, which is impossible)
// * Iterators are almost as fast as hand-coding or memchr, and easier to understand and extend.
// * The Boyer-Moore matches account for about 25% of the total run time when single-threaded.
// * Allocations of the same size object are optimised using a cache. This means that in our
//   iterator, even though we are allocating a new Vec each time, it is still almost as fast
//   as a hand-coded version which re-uses the same buffer. See this article on jemalloc
//   https://www.facebook.com/notes/facebook-engineering/scalable-memory-allocation-using-jemalloc/480222803919
// * Watching Task Manager while the program runs indicates the program is not CPU-bound.

// Possible TODOs
// * For further speed ups, need to profile to find out where the time is actually going - which is
//   the busy thread? Actually it is probably disk-bound.
// * Can we use Aho-Corasick instead of Boyer-Moore?
// * Write an equivalent in C# and compare speeds (n.b. unlikely to be anywhere near close, since
//   this program is already faster than 'cp'!).

fn init() -> (BufReader<File>, BufWriter<File>) {
    let in_filename = std::env::args().nth(1).expect("Pass log file as the only command line parameter.");
    let out_filename = format!("{}.out", in_filename);

    let in_file = File::open(&in_filename).expect("Could not open the input log file");
    let out_file = File::create(&out_filename).expect(&format!("Could not open output file {}", &out_filename));
    println!("\nProcessing {} to output file {}", &in_filename, &out_filename);

    let reader = BufReader::new(in_file);
    let mut writer = BufWriter::new(out_file);
    writer.write(b"Date|Machine|AppName|PID|TID|Level|Source|CorrelationKey|CallRecorderExecutionTime|Message\r\n").unwrap();
    (reader, writer)
}

fn main() {
    // let (reader, writer) = init();
    // let (elapsed, _) = measure_time(|| {
    //     process_using_raw_indexing(reader, writer)
    // });
    // println!("process_using_raw_indexing elapsed = {}", elapsed);

    // let (reader, writer) = init();
    // let (elapsed, _) = measure_time(|| {
    //     process_using_iterators(reader, writer)
    // });
    // println!("process_using_iterators elapsed = {}", elapsed);

    // let (reader, writer) = init();
    // let (elapsed, _) = measure_time(|| {
    //     process_using_memchr(reader, writer)
    // });
    // println!("process_using_memchr elapsed = {}", elapsed);

    let (reader, writer) = init();
    let (elapsed, _) = measure_time(|| {
        process_using_channels(reader, writer)
    });
    println!("process_using_channels elapsed = {}", elapsed);
}
