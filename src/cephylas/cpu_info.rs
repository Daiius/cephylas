
pub enum CpuInfoError {
    IOError(std::io::Error),
    EntryNotFound(String),
    InvalidEntry(String, std::num::ParseIntError),
}

#[derive(Debug)]
pub struct CpuInfo {
    pub user:       i64,
    pub nice:       i64,
    pub system:     i64,
    pub idle:       i64,
    pub iowait:     Option<i64>, // since Linux 2.5.41
    pub irq:        Option<i64>, // since Linux 2.6.0
    pub softirq:    Option<i64>, // since Linux 2.6.0
    pub steal:      Option<i64>, // since Linux 2.6.11
    pub guest:      Option<i64>, // since Linux 2.6.24
    pub guest_nice: Option<i64>, // since Linux 2.6.33
}

impl std::ops::Sub for CpuInfo {
    type Output = Self;
    fn sub(self, rhs: CpuInfo) -> Self::Output {
        CpuInfo {
            user:       self.user - rhs.user,
            nice:       self.nice - rhs.nice,
            system:     self.system - rhs.system,
            idle:       self.idle - rhs.idle,
            iowait:     sub_optional(self.iowait, rhs.iowait),
            irq:        sub_optional(self.irq, rhs.irq),
            softirq:    sub_optional(self.softirq, rhs.softirq),
            steal:      sub_optional(self.steal, rhs.steal),
            guest:      sub_optional(self.guest, rhs.guest),
            guest_nice: sub_optional(self.guest_nice, rhs.guest_nice),
        }
    }
}

fn sub_optional<T>(a: Option<T>, b: Option<T>) -> Option<T> 
where T: std::ops::Sub<Output = T> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x - y),
        _ => None,
    }
}

impl std::fmt::Display for CpuInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CpuInfoError::IOError(e) => 
                write!(f, 
                   "CpuInfoError, IOError: {}", 
                   e.to_string()
                ),
            CpuInfoError::EntryNotFound(name) => 
                write!(f, 
                   "CpuInfoError, Entry {} not found",
                   name
               ),
            CpuInfoError::InvalidEntry(name, e) => 
                write!(f, 
                   "CpuInfoError, Failed to parse {} entry: {}", 
                   name, 
                   e.to_string()
                ),
        }
    }
}

impl std::fmt::Debug for CpuInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl std::error::Error for CpuInfoError {}

fn parse_cpu_entry(name: &str, token: &str) -> Result<i64, CpuInfoError> {
    token.parse().map_err(
        |e| CpuInfoError::InvalidEntry(name.to_string(), e)
    )
}

pub fn calc_cpu_usage(cpu_info: &CpuInfo) -> f32 {
    let CpuInfo { 
        user, nice, system, idle, 
        iowait, irq, softirq, 
        steal, guest, guest_nice 
    } = cpu_info;

    let total = (
        user + nice + system + idle
        + iowait.unwrap_or_default()
        + irq.unwrap_or_default()
        + softirq.unwrap_or_default()
        + steal.unwrap_or_default()
        + guest.unwrap_or_default()
        + guest_nice.unwrap_or_default()
    ) as f32;

    if total == 0.0 {
        0.0 // これは本当にCPU使用率が0であると誤解されるのでは？
    } else {
        (user * 100) as f32 / total
    }
}

impl std::str::FromStr for CpuInfo {
    type Err = CpuInfoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        let cpu = tokens
            .next()
            .ok_or(CpuInfoError::EntryNotFound("cpu".to_string()))?;
        if cpu != "cpu" { 
            return Err(
                CpuInfoError::EntryNotFound(String::from("cpu"))
            ); 
        }

        Ok(CpuInfo { 
            user:       parse_next_token(&mut tokens, "user")?,
            system:     parse_next_token(&mut tokens, "system")?,
            nice:       parse_next_token(&mut tokens, "nice")?,
            idle:       parse_next_token(&mut tokens, "idle")?,
            iowait:     parse_next_token_optional(&mut tokens, "iowait")?,
            irq:        parse_next_token_optional(&mut tokens, "irq")?, 
            softirq:    parse_next_token_optional(&mut tokens, "softirq")?, 
            steal:      parse_next_token_optional(&mut tokens, "steal")?, 
            guest:      parse_next_token_optional(&mut tokens, "guest")?, 
            guest_nice: parse_next_token_optional(&mut tokens, "guest_nice")?,
        })
    }
}

fn parse_next_token(
    tokens: &mut dyn Iterator<Item=&str>, 
    name: &str
) -> Result<i64, CpuInfoError> {
    let token = tokens
        .next()
        .ok_or(CpuInfoError::EntryNotFound(name.to_string()))?;
    parse_cpu_entry(name, token)
}

fn parse_next_token_optional(
    tokens: &mut dyn Iterator<Item=&str>, 
    name: &str
) -> Result<Option<i64>, CpuInfoError> {
    tokens
        .next()
        .map(|t| parse_cpu_entry(name, t))
        .transpose()
}

pub fn get_cpu_info() -> Result<CpuInfo, CpuInfoError> {
    let mut file = std::fs::File::open("/proc/stat")
        .map_err(CpuInfoError::IOError)?;
    let mut contents: String = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents)
        .map_err(CpuInfoError::IOError)?;

    let cpu_line = contents
        .lines()
        .find(|l| l.contains("cpu"))
        .ok_or(CpuInfoError::EntryNotFound("cpu line".to_string()))?;

    cpu_line.parse()
}

