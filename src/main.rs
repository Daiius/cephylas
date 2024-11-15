use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::thread;

#[derive(Debug)]
struct CpuInfo {
    user: i64,
    nice: i64,
    system: i64,
    idle: i64,
    iowait: Option<i64>,     // since Linux 2.5.41
    irq: Option<i64>,        // since Linux 2.6.0
    softirq: Option<i64>,    // since Linux 2.6.0
    steal: Option<i64>,      // since Linux 2.6.11
    guest: Option<i64>,      // since Linux 2.6.24
    guest_nice: Option<i64>, // since Linux 2.6.33
}

fn sub_optional<T>(a: Option<T>, b: Option<T>) -> Option<T> 
where
    T: std::ops::Sub<Output = T>
{
    match (a, b) {
        (Some(x), Some(y)) => Some(x - y),
        _ => None,
    }
}

impl std::ops::Sub for CpuInfo {
    type Output = Self;
    fn sub(self, rhs: CpuInfo) -> Self::Output {
        CpuInfo {
            user: self.user - rhs.user,
            nice: self.nice - rhs.nice,
            system: self.system - rhs.system,
            idle: self.idle - rhs.idle,
            iowait: sub_optional(self.iowait, rhs.iowait),
            irq: sub_optional(self.irq, rhs.irq),
            softirq: sub_optional(self.softirq, rhs.softirq),
            steal: sub_optional(self.steal, rhs.steal),
            guest: sub_optional(self.guest, rhs.guest),
            guest_nice: sub_optional(self.guest_nice, rhs.guest_nice),
        }
    }
}

#[derive(Debug)]
enum ParseCpuInfoError {
    IOError(std::io::Error),
    EntryNotFound(String),
    InvalidEntry(String, std::num::ParseIntError),
}

fn parse_cpu_entry(name: &str, token: &str) -> Result<i64, ParseCpuInfoError> {
    token.parse().map_err(|e| 
        ParseCpuInfoError::InvalidEntry(name.to_string(), e)
    )
}

fn calc_cpu_usage(cpu_info: &CpuInfo) -> f32 {
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

impl FromStr for CpuInfo {
    type Err = ParseCpuInfoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        let cpu = tokens
            .next()
            .ok_or_else(|| ParseCpuInfoError::EntryNotFound("cpu".to_string()))?;
        if cpu != "cpu" { 
            return Err(ParseCpuInfoError::EntryNotFound(String::from("cpu"))); 
        }

        let user = parse_next_token(&mut tokens, "user")?;       
        let system = parse_next_token(&mut tokens, "system")?;
        let nice = parse_next_token(&mut tokens, "nice")?;
        let idle = parse_next_token(&mut tokens, "idle")?;

        let iowait = parse_next_token_optional(&mut tokens, "iowait")?;
        let irq = parse_next_token_optional(&mut tokens, "irq")?;
        let softirq = parse_next_token_optional(&mut tokens, "softirq")?;
        let steal = parse_next_token_optional(&mut tokens, "steal")?;
        let guest = parse_next_token_optional(&mut tokens, "guest")?;
        let guest_nice = parse_next_token_optional(&mut tokens, "guest_nice")?;

        Ok(CpuInfo { 
            user, system, nice, idle,
            iowait, irq, softirq, 
            steal, guest, guest_nice,
        })
    }
}

fn parse_next_token(tokens: &mut dyn Iterator<Item=&str>, name: &str) -> Result<i64, ParseCpuInfoError> {
    let token = tokens
        .next()
        .ok_or_else(|| ParseCpuInfoError::EntryNotFound(name.to_string()))?;
    parse_cpu_entry(name, token)
}

fn parse_next_token_optional(tokens: &mut dyn Iterator<Item=&str>, name: &str) -> Result<Option<i64>, ParseCpuInfoError> {
    tokens
        .next()
        .map(|t| parse_cpu_entry(name, t))
        .transpose()
}

fn get_cpu_info() -> Result<CpuInfo, ParseCpuInfoError> {
    let mut file: File = File::open("/proc/stat")
        .map_err(ParseCpuInfoError::IOError)?;
    let mut contents: String = String::new();
    file.read_to_string(&mut contents)
        .map_err(ParseCpuInfoError::IOError)?;

    // println!("{}", contents);

    let cpu_line = contents
        .lines()
        .find(|l| l.contains("cpu"))
        .ok_or_else(|| ParseCpuInfoError::EntryNotFound("cpu line".to_string()))?;

    println!("{}", cpu_line);

    cpu_line.parse()
}

fn main() -> Result<()> {
    println!("Hello, world!");

    let mut thread_handles: Vec<std::thread::JoinHandle<()>> = vec!();
    for _i in 0..1 {
        thread_handles.push(
            thread::spawn(|| {
                let target_time = 
                    std::time::SystemTime::now() 
                    + std::time::Duration::from_secs(5);

                let mut count: u64 = 0;
                println!("Loop started!");
                while std::time::SystemTime::now() < target_time {
                    count += 1;
                }
                println!("Loop count: {}", count);
            })
        );
    }


    let cpu_info_result = get_cpu_info();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let cpu_info_second = get_cpu_info().unwrap();

    match cpu_info_result {
        Ok(cpu_info) => {
            let diff = cpu_info_second - cpu_info;
            println!("{:?}", &diff);
            println!("cpu usage: {}", calc_cpu_usage(&diff));
        },
        Err(ParseCpuInfoError::IOError(e))
            => panic!("IOError, {:?}", e),
        Err(ParseCpuInfoError::EntryNotFound(name)) 
            => panic!("EntryNotFound, {}", name),
        Err(ParseCpuInfoError::InvalidEntry(name, parse_error)) 
            => panic!("InvalidEntry at {}, {:?}", name, parse_error), 
    }

    Ok(())
}
