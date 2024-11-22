
pub enum MemInfoError {
    IOError(std::io::Error),
    EntryNotFound(String),
    InvalidEntry(String, std::num::ParseIntError),
}
#[allow(dead_code)]
#[derive(Debug)]
pub struct MemInfo {
    pub total:       u64,
    pub free:        u64,
    pub available:   u64, // since Linux 3.14
    pub buffers:     u64,
    pub cached:      u64,
    pub swap_cached: u64,
    pub active:      u64, // since Linux 2.6.28
    pub inactive:    u64, // since Linux 2.6.28
    pub active_anon:  u64,
    pub inactive_anon: u64,
    pub active_file: u64,
    pub inactive_file: u64,
    pub unevictable: u64,
    pub mlocked: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub dirty: u64,
    pub writeback: u64,
    // TODO more memory infomation in /proc/meminfo
}

impl std::fmt::Display for MemInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemInfoError::IOError(e) =>
                write!(f,
                    "MemInfoError, IOError: {}",
                    e.to_string()
                ),
            MemInfoError::EntryNotFound(name) =>
                write!(f,
                    "MemInfoError, Entry {} not found",
                    name
                ),
            MemInfoError::InvalidEntry(name, e) =>
                write!(f,
                    "MemInfoError, Invalid entry {} : {}",
                    name, e.to_string()
                ),
        }
    }
}

impl std::fmt::Debug for MemInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl std::str::FromStr for MemInfo {
    type Err = MemInfoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lines: Vec<&str> = s.lines().collect();

        Ok(MemInfo {
            total:         parse_line(&lines, "MemTotal:")?, 
            free:          parse_line(&lines, "MemFree:")?, 
            available:     parse_line(&lines, "MemAvailable:")?, 
            buffers:       parse_line(&lines, "Buffers:")?, 
            cached:        parse_line(&lines, "Cached:")?,
            swap_cached:   parse_line(&lines, "SwapCached:")?,
            active:        parse_line(&lines, "Active:")?,
            inactive:      parse_line(&lines, "Inactive:")?,
            active_anon:   parse_line(&lines, "Active(anon):")?, 
            inactive_anon: parse_line(&lines, "Inactive(anon):")?, 
            active_file:   parse_line(&lines, "Active(file):")?, 
            inactive_file: parse_line(&lines, "Inactive(file):")?,
            unevictable:   parse_line(&lines, "Unevictable:")?, 
            mlocked:       parse_line(&lines, "Mlocked:")?, 
            swap_total:    parse_line(&lines, "SwapTotal:")?, 
            swap_free:     parse_line(&lines, "SwapFree:")?,
            dirty:         parse_line(&lines, "Dirty:")?, 
            writeback:     parse_line(&lines, "Writeback:")?,
        })
    }
}

fn parse_line(
    lines: &Vec<&str>, 
    signature: &str
) -> Result<u64, MemInfoError> {
    let line = lines.iter()
        .find(|l| l.starts_with(signature))
        .ok_or(MemInfoError::EntryNotFound(signature.to_string()))?;
    let mut tokens = line.split_ascii_whitespace();

    let proc_signature = tokens.next()
        .ok_or(MemInfoError::EntryNotFound(signature.to_string()))?;
    if proc_signature != signature {
        return Err(MemInfoError::EntryNotFound(signature.to_string()));
    }

    tokens.next()
        .ok_or(MemInfoError::EntryNotFound(signature.to_string()))?
        .parse()
        .map_err(|e| MemInfoError::InvalidEntry(signature.to_string(), e))
}

pub fn get_mem_info() -> Result<MemInfo, MemInfoError> {
    let mut file = std::fs::File::open("/proc/meminfo")
        .map_err(MemInfoError::IOError)?;
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents)
        .map_err(MemInfoError::IOError)?;

    contents.parse()
}

