// References:
// - https://github.com/torvalds/linux/blob/master/Documentation/admin-guide/iostats.rst
// - https://github.com/sysstat/sysstat/blob/master/iostat.c
//

pub const SECTOR_SIZE: i64 = 512;

pub enum DiskInfoError {
    IOError(std::io::Error),
    EntryNotFound(String),
    InvalidEntry(String, std::num::ParseIntError),
}
impl std::fmt::Display for DiskInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskInfoError::IOError(e) => 
                write!(f, 
                   "DiskInfoError, IOError: {}", 
                   e.to_string()
                ),
            DiskInfoError::EntryNotFound(name) => 
                write!(f, 
                   "DiskInfoError, Entry {} not found",
                   name
               ),
            DiskInfoError::InvalidEntry(name, e) => 
                write!(f, 
                   "DiskInfoError, Failed to parse {} entry: {}", 
                   name, 
                   e.to_string()
                ),
        }
    }
}
impl std::fmt::Debug for DiskInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
impl std::error::Error for DiskInfoError {}

#[allow(dead_code)]
#[derive(Debug)]
pub struct DiskInfo {
    pub reads_completed: i64,
    pub reads_merged: i64,
    pub sectors_read: i64,
    pub milliseconds_spent_reading: i64,
    pub writes_completed: i64,
    pub writes_merged: i64,
    pub sectors_written: i64,
    pub milliseconds_spent_writing: i64,
    pub ios_in_progress: i64,
    pub milliseconds_spent_ios: i64,
    pub weighted_milliseconds_spent_ios: i64,
    pub discards_completed: i64,
    pub discards_merged: i64,
    pub sectors_discarted: i64,
    pub flush_requests_completed: i64,
    pub milliseconds_spent_flushing: i64,
}

impl std::ops::Sub for DiskInfo {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        DiskInfo {
            reads_completed: 
                self.reads_completed - rhs.reads_completed,
            reads_merged: 
                self.reads_merged - rhs.reads_merged,
            sectors_read: 
                self.sectors_read - rhs.sectors_read,
            milliseconds_spent_reading:
                self.milliseconds_spent_reading - rhs.milliseconds_spent_reading,
            writes_completed:
                self.writes_completed - rhs.writes_completed,
            writes_merged:
                self.writes_merged - rhs.writes_merged,
            sectors_written:
                self.sectors_written - rhs.sectors_written,
            milliseconds_spent_writing:
                self.milliseconds_spent_writing - rhs.milliseconds_spent_writing,
            ios_in_progress:
                self.ios_in_progress - rhs.ios_in_progress,
            milliseconds_spent_ios:
                self.milliseconds_spent_ios - rhs.milliseconds_spent_ios,
            weighted_milliseconds_spent_ios:
                self.weighted_milliseconds_spent_ios - rhs.weighted_milliseconds_spent_ios,
            discards_completed:
                self.discards_completed - rhs.discards_completed,
            discards_merged:
                self.discards_merged - rhs.discards_merged,
            sectors_discarted:
                self.sectors_discarted - rhs.sectors_discarted,
            flush_requests_completed:
                self.flush_requests_completed - rhs.flush_requests_completed,
            milliseconds_spent_flushing:
                self.milliseconds_spent_flushing - rhs.milliseconds_spent_flushing,
        }
    }
}
impl std::str::FromStr for DiskInfo {
    type Err = DiskInfoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        // skip first 3 tokens (disk name)
        tokens.nth(2);
        
        // fortunately the order of struct field initialization
        // is the same as the order of the initializer you write
        Ok(DiskInfo {
            reads_completed:
                parse_token(&mut tokens, "reads_completed")?,
            reads_merged:
                parse_token(&mut tokens, "reads_merged")?,
            sectors_read:
                parse_token(&mut tokens, "sectors_read")?,
            milliseconds_spent_reading:
                parse_token(&mut tokens, "milliseconds_spent_reading")?,
            writes_completed:
                parse_token(&mut tokens, "writes_completed")?,
            writes_merged:
                parse_token(&mut tokens, "writes_merged")?,
            sectors_written:
                parse_token(&mut tokens, "sectors_written")?,
            milliseconds_spent_writing:
                parse_token(&mut tokens, "milliseconds_spent_writing")?,
            ios_in_progress:
                parse_token(&mut tokens, "ios_in_progress")?,
            milliseconds_spent_ios:
                parse_token(&mut tokens, "milliseconds_spent_ios")?,
            weighted_milliseconds_spent_ios:
                parse_token(&mut tokens, "weighted_milliseconds_spent_ios")?,
            discards_completed:
                parse_token(&mut tokens, "discards_completed")?,
            discards_merged:
                parse_token(&mut tokens, "discards_merged")?,
            sectors_discarted:
                parse_token(&mut tokens, "sectors_discarted")?,
            flush_requests_completed:
                parse_token(&mut tokens, "flush_requests_completed")?,
            milliseconds_spent_flushing:
                parse_token(&mut tokens, "milliseconds_spent_flushing")?,
            
        })
    }
}

fn parse_token(
    tokens: &mut dyn Iterator<Item=&str>,
    entry_name: &str
) -> Result<i64, DiskInfoError> {
    tokens.next()
        .ok_or(DiskInfoError::EntryNotFound(entry_name.to_string()))?
        .parse()
        .map_err(|e| DiskInfoError::InvalidEntry(entry_name.to_string(), e))
}

pub fn get_disk_info(disk_name: &str) -> Result<DiskInfo, DiskInfoError> {
    let mut file = std::fs::File::open("/proc/diskstats")
        .map_err(DiskInfoError::IOError)?;
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents)
        .map_err(DiskInfoError::IOError)?;
    
    let line = contents.lines()
        .find(|l| l.contains(disk_name))
        .ok_or(DiskInfoError::EntryNotFound(disk_name.to_string()))?;

    line.parse()
}

