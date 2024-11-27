
pub enum NetInfoError {
    IOError(std::io::Error),
    EntryNotFound(String),
    InvalidEntry(String, std::num::ParseIntError),
}

#[derive(Debug)]
pub struct NetInfo {
    pub rx: NetRxInfo,
    pub tx: NetTxInfo,
}

#[derive(Debug)]
pub struct NetRxInfo {
    pub bytes:      i64,
    pub packets:    i64,
    pub errs:       i64,
    pub drop:       i64,
    pub fifo:       i64,
    pub frame:      i64,
    pub compressed: i64,
    pub multicast:  i64,
}

#[derive(Debug, Clone, Copy)]
pub struct NetTxInfo {
    pub bytes:      i64,
    pub packets:    i64,
    pub errs:       i64,
    pub drop:       i64,
    pub fifo:       i64,
    pub colls:      i64,
    pub carrier:    i64,
    pub compressed: i64,
}

impl std::ops::Sub for NetRxInfo {
    type Output = NetRxInfo;
    fn sub(self, rhs: Self) -> Self::Output {
        NetRxInfo {
            bytes:      self.bytes - rhs.bytes,
            packets:    self.packets - rhs.packets,
            errs:       self.errs - rhs.errs,
            drop:       self.drop - rhs.drop,
            fifo:       self.fifo - rhs.fifo,
            frame:      self.frame - rhs.frame,
            compressed: self.compressed - rhs.compressed,
            multicast:  self.multicast - rhs.multicast,
        }
    }
}

impl std::ops::Sub for NetTxInfo {
    type Output = NetTxInfo;
    fn sub(self, rhs: Self) -> Self::Output {
        NetTxInfo {
            bytes:      self.bytes - rhs.bytes,
            packets:    self.packets - rhs.packets,
            errs:       self.errs - rhs.errs,
            drop:       self.drop - rhs.drop,
            fifo:       self.fifo - rhs.fifo,
            colls:      self.colls - rhs.colls,
            compressed: self.compressed - rhs.compressed,
            carrier:    self.carrier - rhs.carrier,
        }
    }
}

impl std::ops::Sub for NetInfo {
    type Output = Self;
    fn sub(self, rhs: NetInfo) -> Self::Output {
        NetInfo {
            rx: self.rx - rhs.rx,
            tx: self.tx - rhs.tx,
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

fn parse_token(
    tokens: &mut dyn Iterator<Item=&str>,
    entry_name: &str
) -> Result<i64, NetInfoError> {
    tokens.next()
        .ok_or(NetInfoError::EntryNotFound(entry_name.to_string()))?
        .parse()
        .map_err(|e| NetInfoError::InvalidEntry(entry_name.to_string(), e))
}

impl std::str::FromStr for NetInfo {
    type Err = NetInfoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        // omit first token (net device name)
        tokens.next();

        Ok(NetInfo { 
            rx: NetRxInfo {
                bytes:      parse_token(&mut tokens, "rx_bytes")?, 
                packets:    parse_token(&mut tokens, "rx_packets")?, 
                errs:       parse_token(&mut tokens, "rx_errs")?, 
                drop:       parse_token(&mut tokens, "rx_drop")?,
                fifo:       parse_token(&mut tokens, "rx_fifo")?,
                frame:      parse_token(&mut tokens, "rx_frame")?, 
                compressed: parse_token(&mut tokens, "rx_compressed")?,
                multicast:  parse_token(&mut tokens, "rx_multicast")?,
            },
            tx: NetTxInfo {
                bytes:      parse_token(&mut tokens, "tx_bytes")?,
                packets:    parse_token(&mut tokens, "tx_packets")?,
                errs:       parse_token(&mut tokens, "tx_errs")?,
                drop:       parse_token(&mut tokens, "tx_drop")?,
                fifo:       parse_token(&mut tokens, "tx_fifo")?,
                colls:      parse_token(&mut tokens, "tx_colls")?,
                carrier:    parse_token(&mut tokens, "tx_carrier")?,
                compressed: parse_token(&mut tokens, "tx_compressed")?,
            },
        })
    }
}

pub fn get_net_info(
    dev_name: &str,
) -> Result<NetInfo, NetInfoError> {
    let mut file = std::fs::File::open("/proc/net/dev")
        .map_err(NetInfoError::IOError)?;
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents)
        .map_err(NetInfoError::IOError)?;

    let dev_line = contents
        .lines()
        .find(|l| l.contains(dev_name))
        .ok_or(NetInfoError::EntryNotFound(dev_name.to_string()))?;

    dev_line.parse()
}



