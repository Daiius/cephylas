
pub enum NetInfoError {
    IOError(std::io::Error),
    EntryNotFound(String),
    InvalidEntry(String, std::num::ParseIntError),
}

#[derive(Debug)]
pub struct NetInfo {
    pub rx_bytes:   i64,
    pub rx_packets: i64,
    pub tx_bytes:   i64,
    pub tx_packets: i64,
}

impl std::ops::Sub for NetInfo {
    type Output = Self;
    fn sub(self, rhs: NetInfo) -> Self::Output {
        NetInfo {
            rx_bytes   : self.rx_bytes   - rhs.rx_bytes,
            rx_packets : self.rx_packets - rhs.rx_packets,
            tx_bytes   : self.tx_bytes   - rhs.tx_bytes,
            tx_packets : self.tx_packets - rhs.tx_packets,
        }
    }
}

impl std::fmt::Display for NetInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetInfoError::IOError(e) =>
                write!(f,
                    "NetInfoError, IOError: {}",
                    e.to_string()
                ),
            NetInfoError::EntryNotFound(name) =>
                write!(f,
                    "NetInfoError, Entry {} not found",
                    name
                ),
            NetInfoError::InvalidEntry(name, e) =>
                write!(f,
                    "NetInfoError, Invalid entry {} : {}",
                    name, e.to_string()
                ),
        }
    }
}

impl std::fmt::Debug for NetInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl std::str::FromStr for NetInfo {
    type Err = NetInfoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        let rx_bytes: i64 = tokens.nth(1)
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_bytes".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_bytes".to_string(), e))?;
        let rx_packets: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_packets".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_packets".to_string(), e))?;
        let tx_bytes: i64 = tokens.nth(6)
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_bytes".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_bytes".to_string(), e))?;
        let tx_packets: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_packets".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_packets".to_string(), e))?;

        Ok(NetInfo { rx_bytes, rx_packets, tx_bytes, tx_packets })
    }
}

pub fn get_net_info(dev_name: &str) -> Result<NetInfo, NetInfoError> {
    let mut file = std::fs::File::open("/proc/net/dev")
        .map_err(NetInfoError::IOError)?;
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents)
        .map_err(NetInfoError::IOError)?;

    let dev_line = contents
        .lines()
        .find(|l| l.contains(dev_name))
        .ok_or_else(
            || NetInfoError::EntryNotFound(dev_name.to_string())
        )?;

    dev_line.parse()
}



