use needle::{self, SearchIn};

#[derive(Default)]
pub struct ParsedData {
    pub date: (usize, usize),
    pub machine: (usize, usize),
    pub appname: (usize, usize),
    pub pid: (usize, usize),
    pub tid: (usize, usize),
    pub level: (usize, usize),
    pub source: (usize, usize),
    pub message: (usize, usize),
    pub ckey: Option<(usize, usize)>,
    pub cret: Option<(usize, usize)>
}

pub fn parse_main_block(buffer: &[u8]) -> ParsedData {
    let mut it = buffer.iter();

    let date_a = 0;
    let date_b = it.position(|&x| x == b'|').unwrap() - 1;
    
    let machine_a = date_b + 3 + it.position(|&x| x == b'=').unwrap();
    let machine_b = machine_a + it.position(|&x| x == b' ').unwrap();

    let appname_a = machine_b + 2 + it.position(|&x| x == b'=').unwrap();
    let appname_b = appname_a + it.position(|&x| x == b' ').unwrap();

    let pid_a = appname_b + 2 + it.position(|&x| x == b'=').unwrap();
    let pid_b = pid_a + it.position(|&x| x == b' ').unwrap();

    let tid_a = pid_b + 2 + it.position(|&x| x == b'=').unwrap();
    let tid_b = tid_a + it.position(|&x| x == b' ').unwrap();

    let level_a = tid_b + 2 + it.position(|&x| x == b'[').unwrap();
    let level_b = level_a + it.position(|&x| x == b']').unwrap();

    ParsedData {
        date: (date_a, date_b),
        machine: (machine_a, machine_b),
        appname: (appname_a, appname_b),
        pid: (pid_a, pid_b),
        tid: (tid_a, tid_b),
        level: (level_a, level_b),
        ..Default::default()
    }
}

pub fn parse_source_and_msg(buffer: &[u8], pd: &mut ParsedData, finder: &needle::BoyerMoore<u8>) {
    let start_point = pd.level.1;
    let rest = &buffer[pd.level.1..];

    let source_a = start_point + finder.find_first_in(rest).expect("Could not find the Source") + "Source=".len();
    let mut it = buffer[source_a..].iter();
    pd.source = (source_a, source_a + it.position(|&x| x == b' ').unwrap());
    
    let mut i = source_a - "Source=".len();
    while buffer[i] == b' ' || buffer[i] == b'\n' {
        i = i - 1;
    }
    pd.message = (start_point + 4, i);
}

pub fn parse_ckey_and_cret(buffer: &[u8], pd: &mut ParsedData,
    ckey_finder: &needle::BoyerMoore<u8>,
    cret_finder: &needle::BoyerMoore<u8>)
{
    let start_point = pd.level.1;
    let rest = &buffer[pd.level.1..];

    // CorrelationKey might not exist.
    match ckey_finder.find_first_in(rest) {
        None => {
            pd.ckey = None;
        },
        Some(i) => {
            let a = start_point + i + "CorrelationKey".len() + 1;
            pd.ckey = Some((a, a + 36));
        }
    };

    // CallRecorderExecutionTime might not exist.
    match cret_finder.find_first_in(rest) {
        None => {
            pd.cret = None;
        }
        Some(i) => {
            let a = start_point + i + "CallRecorderExecutionTime".len() + 1;

            let mut it = buffer[a..].iter();
            let b = match it.position(|&x| x == b' ' || x == b'\r' || x == b'\n') {
                None => buffer.len(),
                Some(x) => a + x
            };

            pd.cret = Some((a, b));
        }
    };
}
