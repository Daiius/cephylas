
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

#[derive(Debug)]
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
            carrier:  self.carrier - rhs.carrier,
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

impl std::str::FromStr for NetInfo {
    type Err = NetInfoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = s.split_ascii_whitespace();
        // omit first token (net device name)
        tokens.next();
        let rx_bytes: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_bytes".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_bytes".to_string(), e))?;
        let rx_packets: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_packets".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_packets".to_string(), e))?;
        let rx_errs: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_errs".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_errs".to_string(), e))?;
        let rx_drop: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_drop".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_drop".to_string(), e))?;
        let rx_fifo: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_fifo".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_fifo".to_string(), e))?;
        let rx_frame: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_frame".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_frame".to_string(), e))?;
        let rx_compressed: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_compressed".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_compressed".to_string(), e))?;
        let rx_multicast: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("rx_multicast".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("rx_multicast".to_string(), e))?;

        let tx_bytes: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_bytes".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_bytes".to_string(), e))?;
        let tx_packets: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_packets".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_packets".to_string(), e))?;
        let tx_errs: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_errs".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_errs".to_string(), e))?;
        let tx_drop: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_drop".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_drop".to_string(), e))?;
        let tx_fifo: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_fifo".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_fifo".to_string(), e))?;
        let tx_colls: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_colls".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_colls".to_string(), e))?;
        let tx_carrier: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_carrier".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_carrier".to_string(), e))?;
        let tx_compressed: i64 = tokens.next()
            .ok_or_else(|| NetInfoError::EntryNotFound("tx_compressed".to_string()))?
            .parse()
            .map_err(|e| NetInfoError::InvalidEntry("tx_compressed".to_string(), e))?;

        Ok(NetInfo { 
            rx: NetRxInfo {
                bytes: rx_bytes, packets: rx_packets, 
                errs: rx_errs, drop: rx_drop, fifo: rx_fifo, frame: rx_frame, 
                compressed: rx_compressed, multicast: rx_multicast
            },
            tx: NetTxInfo {
                bytes: tx_bytes, packets: tx_packets,
                errs: tx_errs, drop: tx_drop, fifo: tx_fifo, colls: tx_colls,
                carrier: tx_carrier, compressed: tx_compressed,
            },
        })
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



