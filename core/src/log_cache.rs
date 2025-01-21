
use std::collections::{ VecDeque, HashMap, };
use std::sync::{ Arc, RwLock, };

use super::log::option_to_string;

pub const MAX_LOG_LENGTH: usize = 8640;

/// 最大 MAX_LOG_LENGTH 個の要素を記録するVecのwrapper
pub struct LogVec<T> {
    v: VecDeque<T>
}
impl<T> LogVec<T> 
    where T: ToString
{
    /// MAX_LOG_LENGTHのcapacityを設定した状態で初期化します
    pub fn new() -> Self {
        let v = VecDeque::with_capacity(MAX_LOG_LENGTH);
        LogVec { v }
    }
    /// 末尾にデータを追加し、MAX_LOG_LENGTHを超えるならば
    /// 先頭データを削除します
    pub fn push(self: &mut Self, d: T) {
        if self.v.len() >= MAX_LOG_LENGTH {
            self.v.pop_front();
        }
        self.v.push_back(d);
    }
    //pub fn data(self: &Self) -> &VecDeque<T> { 
    //    &self.v
    //}
    pub fn to_json(self: &Self) -> String {
        format!(
            "[{}]",
            self.v.iter()
                .map(|d| d.to_string())
                .collect::<Vec<String>>()
                .join(",")
        )
    }
}

pub struct UsageCacheMap<T> {
    map: HashMap<String, LogVec<T>>
}
impl<T> UsageCacheMap<T> 
    where T: ToString
{
    pub fn new() -> Self {
        UsageCacheMap {
            map: HashMap::<String, LogVec<T>>::new()
        }
    }
    pub fn insert(
        &mut self, 
        container_name: String, 
        usage: T
    ) {
        self.map.entry(container_name)
            .or_insert(
                LogVec::<T>::new()
            ).push(usage);
    }
    pub fn container_names(&self) -> Vec<&String> {
        self.map.keys().collect::<Vec<&String>>()
    }
    pub fn get(
        &self,
        container_name: &str
    ) -> Option<&LogVec<T>> {
        self.map.get(container_name)
    }
}

pub struct TimedCpuUsage {
    pub time: String,
    pub percentage: Option<f32>,
    //TODO 他のフィールドも
}
impl std::fmt::Display for TimedCpuUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f, 
            "{{\"time\":\"{}\",\"percentage\":{}}}", 
            self.time, 
            option_to_string(self.percentage)
        )
    }
}
pub struct TimedMemoryUsage {
    pub time: String,
    pub percentage: Option<f32>,
    //TODO 他のフィールドも
}
impl std::fmt::Display for TimedMemoryUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"time\":\"{}\",\"percentage\":{}}}",
            self.time,
            option_to_string(self.percentage)
        )
    }
}
#[allow(non_snake_case)]
pub struct TimedIoUsage {
    pub time: String,
    pub readkBps: Option<u32>,
    pub writekBps: Option<u32>,
    //TODO 他のフィールドも
}
impl std::fmt::Display for TimedIoUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"time\":\"{}\",\"readkBps\":{},\"writekBps\":{}}}",
            self.time,
            option_to_string(self.readkBps),
            option_to_string(self.writekBps)
        )
    }
}
#[allow(non_snake_case)]
pub struct TimedNetUsage {
    pub time: String,
    pub recvkBps: Option<u32>,
    pub sendkBps: Option<u32>,
}
impl std::fmt::Display for TimedNetUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"time\":\"{}\",\"recvkBps\":{},\"sendkBps\":{}}}",
            self.time,
            option_to_string(self.recvkBps),
            option_to_string(self.sendkBps),
        )
    }
}

pub struct UsageCache {
    pub cpu: UsageCacheMap<TimedCpuUsage>,
    pub memory: UsageCacheMap<TimedMemoryUsage>,
    pub io: UsageCacheMap<TimedIoUsage>,
    pub net: UsageCacheMap<TimedNetUsage>,
}
impl UsageCache {
    fn new() -> Self {
        UsageCache {
            cpu: UsageCacheMap::<TimedCpuUsage>::new(),
            memory: UsageCacheMap::<TimedMemoryUsage>::new(),
            io: UsageCacheMap::<TimedIoUsage>::new(),
            net: UsageCacheMap::<TimedNetUsage>::new(),
        }
    }
}

pub type SharedUsageCache = Arc<RwLock<UsageCache>>;
pub fn create_shared_cache() -> SharedUsageCache {
    Arc::new(RwLock::new(UsageCache::new()))
}
