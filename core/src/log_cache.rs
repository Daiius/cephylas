
use std::collections::{ VecDeque, HashMap, };
use std::sync::Arc;
use tokio::sync::RwLock;

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
}

pub struct DownsampleOption {
    pub nsample: usize,
}
impl Default for DownsampleOption {
    fn default() -> Self {
        DownsampleOption { nsample: 512 }
    }
}

fn calculate_triangle_area(
    p0: &(f32, f32),
    p1: &(f32, f32),
    p2: &(f32, f32),
) -> f32 {
    let x1 = p1.0 - p0.0;
    let y1 = p1.1 - p0.1;
    let x2 = p2.0 - p0.0;
    let y2 = p2.1 - p0.1;

    (x2 * y1 - x1 * y2).abs() * 0.5
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
    /// 記録されたコンテナ名をソートしてから返します
    pub fn container_names(&self) -> Vec<&String> {
        let mut keys = self.map.keys().collect::<Vec<&String>>();
        keys.sort();

        keys
    }
    pub fn get(
        &self,
        container_name: &str
    ) -> Option<&LogVec<T>> {
        self.map.get(container_name)
    }
    pub fn downsample<F: Fn(&T) -> (f32, f32)>(
        &self,
        container_name: &str,
        downsample_option: &DownsampleOption,
        fxy: F, // データ型からXY座標を取得するための関数 
    ) -> Option<Vec<&T>> {
        let data = &self.map.get(container_name)?.v;
        let n = data.len();
        let nsample = downsample_option.nsample;
        if nsample >= n || nsample < 3 {
            return Some(data.iter().collect());
        }
        let mut samples: Vec<&T> = Vec::with_capacity(nsample);
        let bucket_size = (n - 2) as f32 / (nsample - 2) as f32;
        samples.push(&data[0]);

        // 本来LTTBアルゴリズムでは次のバケットの平均点を用いるが
        // 間違って今のバケットの平均値を使っても一応動く
        // (Largest Triangle in Three Bukets じゃなくて
        //  Single Buckets になる?
        // )
        let mut last_point = None;
        for i in 0..(nsample - 2) {
            let mut max_area = -1.0;
            let mut max_area_point = None;

            let istart = 
                ((i as f32 * bucket_size).floor() as usize)
                .max(1);
            let iend = 
                ((i as f32 + 1.0) * bucket_size).floor() as usize;
            let istart_next =
                ((i as f32 + 1.0) * bucket_size).floor() as usize;
            let iend_next =
                (((i as f32 + 2.0) * bucket_size).floor() as usize)
                .min(n - 1);
            let (average_x, average_y) = 
                data.iter()
                .skip(istart_next)
                .take(iend_next - istart_next)
                .map(|d| fxy(d))
                .reduce(|acc, curr| (acc.0 + curr.0, acc.1 + curr.1))
                .map(|s| (
                    s.0 / (iend - istart) as f32, 
                    s.1 / (iend - istart) as f32
                ))
                .unwrap_or(fxy(&data[0]));
            for j in istart..iend {
                let area = calculate_triangle_area(
                    &fxy(last_point.unwrap_or(&data[0])),
                    &fxy(&data[j]),
                    &(average_x, average_y),
                );
                if area > max_area {
                    max_area = area;
                    max_area_point = Some(&data[j]);
                }
            }
            if let Some(point) = max_area_point {
                samples.push(point);
                last_point = Some(point);
            }
        }
        // 最後の点を追加
        samples.push(&data[n - 1]);

        Some(samples)
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
