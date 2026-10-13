
use std::collections::{ VecDeque, HashMap, };
use std::sync::{ Arc, RwLock, };

use super::log::option_to_string;
use super::time;

/// 1コンテナあたりに持つ点数の上限（24時間 × 10秒間隔）
/// 窓は時間で切るので、これは安全のための上限
pub const MAX_LOG_LENGTH: usize = 8640;
/// キャッシュに残す時間の幅。キャッシュ全体で最新の点から数える
pub const WINDOW_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// 1 tick・1コンテナ分の記録
///
/// 時刻は文字列ではなくエポックからのミリ秒で持ちます
/// (文字列のまま種別ごとに持つと、1コンテナ24時間分で数MBになる)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsagePoint {
    pub time_ms: i64,
    pub cpu: Option<f32>,
    pub memory: Option<f32>,
    pub read_kbps: Option<u32>,
    pub write_kbps: Option<u32>,
    pub recv_kbps: Option<u32>,
    pub send_kbps: Option<u32>,
}

/// APIが返す系列の種類
/// ダウンサンプリングで基準にする値と、返すjsonの形を決めます
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Series {
    Cpu,
    Memory,
    IoRead,
    IoWrite,
    NetRecv,
    NetSend,
}

impl Series {
    /// ダウンサンプリングの縦軸に使う値。欠損は0として扱います
    fn value(self, point: &UsagePoint) -> f64 {
        match self {
            Series::Cpu => point.cpu.unwrap_or_default() as f64,
            Series::Memory => point.memory.unwrap_or_default() as f64,
            Series::IoRead => point.read_kbps.unwrap_or_default() as f64,
            Series::IoWrite => point.write_kbps.unwrap_or_default() as f64,
            Series::NetRecv => point.recv_kbps.unwrap_or_default() as f64,
            Series::NetSend => point.send_kbps.unwrap_or_default() as f64,
        }
    }

    /// 1点をAPIの要素（jsonのオブジェクト）にします
    /// io/read と io/write、net/recv と net/send は同じ形です
    pub fn point_to_json(self, point: &UsagePoint) -> String {
        let time = time::millis_to_iso8601(point.time_ms);
        match self {
            Series::Cpu => format!(
                "{{\"time\":\"{}\",\"percentage\":{}}}",
                time,
                option_to_string(point.cpu),
            ),
            Series::Memory => format!(
                "{{\"time\":\"{}\",\"percentage\":{}}}",
                time,
                option_to_string(point.memory),
            ),
            Series::IoRead | Series::IoWrite => format!(
                "{{\"time\":\"{}\",\"readkBps\":{},\"writekBps\":{}}}",
                time,
                option_to_string(point.read_kbps),
                option_to_string(point.write_kbps),
            ),
            Series::NetRecv | Series::NetSend => format!(
                "{{\"time\":\"{}\",\"recvkBps\":{},\"sendkBps\":{}}}",
                time,
                option_to_string(point.recv_kbps),
                option_to_string(point.send_kbps),
            ),
        }
    }
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
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
) -> f64 {
    let x1 = p1.0 - p0.0;
    let y1 = p1.1 - p0.1;
    let x2 = p2.0 - p0.0;
    let y2 = p2.1 - p0.1;

    (x2 * y1 - x1 * y2).abs() * 0.5
}

/// [start, end) の点の平均（重心）を返します
fn average_point<F: Fn(usize) -> (f64, f64)>(
    xy: &F,
    start: usize,
    end: usize,
) -> (f64, f64) {
    let (sum_x, sum_y) = (start..end)
        .map(xy)
        .fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
    let count = (end - start) as f64;

    (sum_x / count, sum_y / count)
}

/// LTTB (Largest Triangle Three Buckets) で残す点の添字を返します
///
/// 先頭と末尾は必ず残し、間を nsample - 2 個のバケットに分けて、
/// 各バケットから「直前に選んだ点・候補・次のバケットの平均点」が作る
/// 三角形の面積が最大になる点を1つずつ選びます
/// n > nsample >= 3 を前提とし、ちょうど nsample 個を返します
fn lttb_indices<F: Fn(usize) -> (f64, f64)>(
    n: usize,
    nsample: usize,
    xy: F,
) -> Vec<usize> {
    let mut indices = Vec::with_capacity(nsample);
    // 先頭と末尾を除いた n - 2 点を、nsample - 2 個のバケットに分ける
    // 境界は整数で計算する（浮動小数の誤差で末尾の手前の点を取りこぼさないため）
    let bucket_start = |i: usize| {
        (i * (n - 2) / (nsample - 2) + 1).min(n - 1)
    };

    let mut selected = 0;
    indices.push(selected);
    for i in 0..(nsample - 2) {
        let start = bucket_start(i);
        let end = bucket_start(i + 1);
        // 次のバケット。最後のバケットの「次」は末尾の1点になる
        let next_end = bucket_start(i + 2).max(end + 1).min(n);
        let average = average_point(&xy, end, next_end);

        let previous = xy(selected);
        let mut max_area = -1.0;
        for j in start..end {
            let area = calculate_triangle_area(previous, xy(j), average);
            if area > max_area {
                max_area = area;
                selected = j;
            }
        }
        indices.push(selected);
    }
    indices.push(n - 1);

    indices
}

/// コンテナごとの直近の記録
pub struct UsageCache {
    map: HashMap<String, VecDeque<UsagePoint>>,
}

impl UsageCache {
    fn new() -> Self {
        UsageCache { map: HashMap::new() }
    }

    /// 1点を追加します。追加したら true を返します
    ///
    /// そのコンテナの最新の点より新しくない点は、追加せずに捨てます
    /// (系列を時刻の昇順に保つため)
    /// 時間で捨てる処理はしないので、1 tick 分を入れ終えたら prune を呼びます
    pub fn insert(&mut self, container_name: &str, point: UsagePoint) -> bool {
        if !self.map.contains_key(container_name) {
            self.map.insert(container_name.to_string(), VecDeque::new());
        }
        let points = self.map.get_mut(container_name)
            .expect("the entry was inserted just above");

        if points.back().is_some_and(|last| point.time_ms <= last.time_ms) {
            return false;
        }
        if points.len() >= MAX_LOG_LENGTH {
            points.pop_front();
        }
        if points.len() == points.capacity() {
            // VecDeque に任せると容量が倍々に増え、上限の約2倍を確保してしまう
            // 上限を超えない範囲で増やす
            let additional = points.len().max(16)
                .min(MAX_LOG_LENGTH - points.len());
            points.reserve_exact(additional);
        }
        points.push_back(point);

        true
    }

    /// キャッシュ全体で最新の点より WINDOW_MILLIS を超えて古い点を捨てます
    ///
    /// その tick でデータが来なかったコンテナにも掛かります
    /// 点が1つも無くなったコンテナは、一覧からも消えます
    pub fn prune(&mut self) {
        self.prune_from(None);
    }

    /// prune と同じですが、現在時刻も窓の基準に加えます
    /// (最新の点の時刻と現在時刻の、遅い方から数える)
    ///
    /// 新しい点が1つも来なくなっても、時間の経過で古い点が捨てられます
    /// 稼働中の tick から呼びます。復元では使いません
    /// (過去のログを読むときは、ログの最後の行が基準になる)
    pub fn prune_at(&mut self, now_ms: i64) {
        self.prune_from(Some(now_ms));
    }

    fn prune_from(&mut self, now_ms: Option<i64>) {
        let latest = self.map.values()
            .filter_map(|points| points.back())
            .map(|point| point.time_ms)
            .chain(now_ms)
            .max();
        let Some(latest) = latest else { return; };
        let oldest_to_keep = latest.saturating_sub(WINDOW_MILLIS);

        for points in self.map.values_mut() {
            while points.front()
                .is_some_and(|point| point.time_ms < oldest_to_keep)
            {
                points.pop_front();
            }
            while points.len() > MAX_LOG_LENGTH {
                points.pop_front();
            }
        }
        self.map.retain(|_, points| !points.is_empty());
    }

    /// 記録されたコンテナ名をソートしてから返します
    pub fn container_names(&self) -> Vec<&String> {
        let mut keys = self.map.keys().collect::<Vec<&String>>();
        keys.sort();

        keys
    }

    #[cfg(test)]
    pub fn get(&self, container_name: &str) -> Option<&VecDeque<UsagePoint>> {
        self.map.get(container_name)
    }

    /// 1コンテナ・1系列を、最大 nsample 点に間引いて返します
    /// 知らないコンテナ名なら None を返します
    ///
    /// 横軸は系列の先頭からの経過秒、縦軸は series の値です
    /// 系列ごとに独立して選ぶので、同じコンテナでも系列によって残る点が変わります
    pub fn downsample(
        &self,
        container_name: &str,
        downsample_option: &DownsampleOption,
        series: Series,
    ) -> Option<Vec<&UsagePoint>> {
        let data = self.map.get(container_name)?;
        let n = data.len();
        let nsample = downsample_option.nsample;
        if nsample >= n || nsample < 3 {
            return Some(data.iter().collect());
        }

        let origin = data[0].time_ms;
        let xy = |i: usize| {
            let point = &data[i];
            (
                point.time_ms.wrapping_sub(origin) as f64 / 1000.0,
                series.value(point),
            )
        };

        Some(
            lttb_indices(n, nsample, xy)
                .into_iter()
                .map(|i| &data[i])
                .collect()
        )
    }
}

pub type SharedUsageCache = Arc<RwLock<UsageCache>>;
pub fn create_shared_cache() -> SharedUsageCache {
    Arc::new(RwLock::new(UsageCache::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-01-01T00:00:00Z
    const BASE_MS: i64 = 1_767_225_600_000;
    const TICK_MS: i64 = 10_000;

    fn point(time_ms: i64, value: u32) -> UsagePoint {
        UsagePoint {
            time_ms,
            cpu: Some(value as f32),
            memory: Some(value as f32),
            read_kbps: Some(value),
            write_kbps: Some(value),
            recv_kbps: Some(value),
            send_kbps: Some(value),
        }
    }

    /// 1 tick 分（全コンテナ同じ時刻）を入れて prune する
    fn push_tick(cache: &mut UsageCache, names: &[&str], time_ms: i64) {
        for name in names {
            cache.insert(name, point(time_ms, 0));
        }
        cache.prune();
    }

    fn times(cache: &UsageCache, name: &str) -> Vec<i64> {
        cache.get(name).unwrap().iter().map(|p| p.time_ms).collect()
    }

    #[test]
    fn point_is_small() {
        // 時刻 8 + Option<f32> 8 × 2 + Option<u32> 8 × 4
        assert_eq!(std::mem::size_of::<UsagePoint>(), 56);
    }

    #[test]
    fn old_points_are_dropped_by_time() {
        let mut cache = UsageCache::new();
        // 1時間おきに 30 点
        for i in 0..30 {
            push_tick(&mut cache, &["web"], BASE_MS + i * 3_600_000);
        }
        let latest = BASE_MS + 29 * 3_600_000;
        let kept = times(&cache, "web");
        // ちょうど24時間前の点は残り、それより古い点は捨てられる
        assert_eq!(kept.len(), 25);
        assert_eq!(*kept.first().unwrap(), latest - WINDOW_MILLIS);
        assert_eq!(*kept.last().unwrap(), latest);

        // 間が空いたあとに1点来ると、それ以前は全部捨てられる
        push_tick(&mut cache, &["web"], latest + WINDOW_MILLIS + 1);
        assert_eq!(times(&cache, "web"), vec![latest + WINDOW_MILLIS + 1]);
    }

    #[test]
    fn point_count_is_capped() {
        let mut cache = UsageCache::new();
        // 1秒間隔なら 24 時間に 86400 点入るが、点数の上限で止まる
        let total = MAX_LOG_LENGTH as i64 + 100;
        for i in 0..total {
            push_tick(&mut cache, &["web"], BASE_MS + i * 1000);
        }
        let kept = times(&cache, "web");
        assert_eq!(kept.len(), MAX_LOG_LENGTH);
        assert_eq!(*kept.last().unwrap(), BASE_MS + (total - 1) * 1000);
        assert_eq!(*kept.first().unwrap(), BASE_MS + 100 * 1000);
        // 容量も上限を超えて確保しない
        assert!(
            cache.get("web").unwrap().capacity() <= MAX_LOG_LENGTH,
            "{}", cache.get("web").unwrap().capacity(),
        );
    }

    #[test]
    fn full_day_at_tick_interval_fits() {
        let mut cache = UsageCache::new();
        for i in 0..(MAX_LOG_LENGTH as i64 * 2) {
            push_tick(&mut cache, &["web"], BASE_MS + i * TICK_MS);
        }
        let kept = times(&cache, "web");
        assert_eq!(kept.len(), MAX_LOG_LENGTH);
        let span = kept.last().unwrap() - kept.first().unwrap();
        assert_eq!(span, WINDOW_MILLIS - TICK_MS);
    }

    #[test]
    fn stopped_container_is_pruned_and_then_removed() {
        let mut cache = UsageCache::new();
        // web と db が1時間動き、そのあと db だけ止まる
        for i in 0..360 {
            push_tick(&mut cache, &["web", "db"], BASE_MS + i * TICK_MS);
        }
        let db_last = BASE_MS + 359 * TICK_MS;
        assert_eq!(cache.container_names(), vec!["db", "web"]);

        // 23時間30分後: db にはデータが来ていないが、古い点は捨てられている
        let mut now = db_last;
        while now < BASE_MS + WINDOW_MILLIS + 30 * 60 * 1000 {
            now += TICK_MS;
            push_tick(&mut cache, &["web"], now);
        }
        let db = times(&cache, "db");
        assert!(db.len() < 360 && !db.is_empty(), "{}", db.len());
        assert!(db.iter().all(|t| *t >= now - WINDOW_MILLIS));
        assert_eq!(*db.last().unwrap(), db_last);
        assert_eq!(cache.container_names(), vec!["db", "web"]);

        // db の最後の点から24時間を過ぎると、db は一覧から消える
        while now <= db_last + WINDOW_MILLIS {
            now += TICK_MS;
            push_tick(&mut cache, &["web"], now);
        }
        assert!(cache.get("db").is_none());
        assert_eq!(cache.container_names(), vec!["web"]);
        assert!(
            cache.downsample("db", &DownsampleOption::default(), Series::Cpu)
                .is_none()
        );
    }

    #[test]
    fn prune_at_expires_points_without_new_data() {
        let mut cache = UsageCache::new();
        for i in 0..360 {
            push_tick(&mut cache, &["web", "db"], BASE_MS + i * TICK_MS);
        }
        let last = BASE_MS + 359 * TICK_MS;

        // 現在時刻が最新の点より前でも、最新の点が基準のまま（何も捨てない）
        cache.prune_at(BASE_MS - WINDOW_MILLIS);
        assert_eq!(times(&cache, "web").len(), 360);

        // 新しい点が来なくても、時間が経てば古い点から捨てられる
        cache.prune_at(BASE_MS + WINDOW_MILLIS + 100 * TICK_MS);
        assert_eq!(times(&cache, "web").len(), 260);
        assert_eq!(times(&cache, "db").len(), 260);
        // 最後の点からちょうど24時間までは残る
        cache.prune_at(last + WINDOW_MILLIS);
        assert_eq!(times(&cache, "web"), vec![last]);
        assert_eq!(cache.container_names(), vec!["db", "web"]);
        // 24時間を過ぎると、全コンテナが一覧から消える
        cache.prune_at(last + WINDOW_MILLIS + 1);
        assert!(cache.container_names().is_empty());
        // 空のキャッシュでも壊れない
        cache.prune_at(last);
        assert!(cache.container_names().is_empty());
    }

    #[test]
    fn points_not_newer_than_the_last_are_rejected() {
        let mut cache = UsageCache::new();
        assert!(cache.insert("web", point(BASE_MS + 20_000, 0)));
        assert!(!cache.insert("web", point(BASE_MS + 20_000, 1)));
        assert!(!cache.insert("web", point(BASE_MS + 10_000, 1)));
        assert!(cache.insert("web", point(BASE_MS + 30_000, 0)));
        // コンテナごとの判定なので、別のコンテナには入る
        assert!(cache.insert("db", point(BASE_MS + 10_000, 0)));
        assert_eq!(times(&cache, "web"), vec![BASE_MS + 20_000, BASE_MS + 30_000]);
    }

    #[test]
    fn prune_on_empty_cache_does_nothing() {
        let mut cache = UsageCache::new();
        cache.prune();
        assert!(cache.container_names().is_empty());
    }

    #[test]
    fn short_series_is_returned_as_is() {
        let mut cache = UsageCache::new();
        for i in 0..512 {
            cache.insert("web", point(BASE_MS + i * TICK_MS, (i % 5) as u32));
        }
        let option = DownsampleOption::default();
        assert_eq!(option.nsample, 512);
        let sampled = cache.downsample("web", &option, Series::Cpu).unwrap();
        assert_eq!(sampled.len(), 512);
        assert!(
            sampled.iter().zip(cache.get("web").unwrap())
                .all(|(a, b)| *a == b)
        );
        assert!(cache.downsample("nobody", &option, Series::Cpu).is_none());
    }

    #[test]
    fn long_series_keeps_first_and_last() {
        let mut cache = UsageCache::new();
        let n = MAX_LOG_LENGTH as i64;
        for i in 0..n {
            cache.insert("web", point(BASE_MS + i * TICK_MS, (i * 7 % 13) as u32));
        }
        for series in [
            Series::Cpu, Series::Memory, Series::IoRead,
            Series::IoWrite, Series::NetRecv, Series::NetSend,
        ] {
            let sampled = cache
                .downsample("web", &DownsampleOption::default(), series)
                .unwrap();
            assert_eq!(sampled.len(), 512);
            assert_eq!(sampled[0].time_ms, BASE_MS);
            assert_eq!(sampled[511].time_ms, BASE_MS + (n - 1) * TICK_MS);
            // 時刻の昇順で、同じ点を二度選ばない
            assert!(sampled.windows(2).all(|w| w[0].time_ms < w[1].time_ms));
        }
    }

    #[test]
    fn every_isolated_spike_survives_across_utc_midnight() {
        // 2026-01-01T18:00:00Z から24時間分。UTC の 0 時を途中でまたぐ
        let start = BASE_MS + 18 * 3_600_000;
        let n = MAX_LOG_LENGTH as i64;
        let midnight = BASE_MS + WINDOW_MILLIS;
        // 平らな系列に、0時の直前・直後を含むスパイクを置く
        // (1つのバケットは約17点。同じバケットや隣のバケットに重ならない間隔にする)
        let spikes: Vec<i64> = vec![
            start + 100 * TICK_MS,
            midnight - TICK_MS,
            midnight + 60 * TICK_MS,
            midnight + 2 * 3_600_000 + 5 * TICK_MS,
        ];
        let memory_spike = start + 5000 * TICK_MS;

        let mut cache = UsageCache::new();
        for i in 0..n {
            let t = start + i * TICK_MS;
            let mut p = point(t, 1);
            if spikes.contains(&t) {
                p.cpu = Some(90.0);
            }
            if t == memory_spike {
                p.memory = Some(80.0);
            }
            cache.insert("web", p);
        }
        assert!(spikes.iter().all(|t| *t > start && *t < start + n * TICK_MS));

        let sampled = cache
            .downsample("web", &DownsampleOption::default(), Series::Cpu)
            .unwrap();
        assert_eq!(sampled.len(), 512);
        for spike in &spikes {
            assert!(
                sampled.iter().any(|p| p.time_ms == *spike && p.cpu == Some(90.0)),
                "spike at {} was dropped", time::millis_to_iso8601(*spike),
            );
        }
        // 0時の前後で選ばれる点が偏らない（どちら側からも相応の数が選ばれる）
        let before = sampled.iter().filter(|p| p.time_ms < midnight).count();
        // 0時より前は全体の 1/4（6時間 / 24時間）
        assert!((118..=138).contains(&before), "{}", before);

        // 系列ごとに独立して選ぶ: memory のスパイクは memory の系列で残る
        let by_memory = cache
            .downsample("web", &DownsampleOption::default(), Series::Memory)
            .unwrap();
        assert!(by_memory.iter().any(|p| p.time_ms == memory_spike));
        let picked = |sampled: &Vec<&UsagePoint>| {
            sampled.iter().map(|p| p.time_ms).collect::<Vec<i64>>()
        };
        assert_ne!(picked(&sampled), picked(&by_memory));
    }

    #[test]
    fn average_point_is_sum_divided_by_its_own_count() {
        let data = [(0.0, 0.0), (1.0, 3.0), (2.0, 6.0), (3.0, 30.0), (9.0, 9.0)];
        let xy = |i: usize| data[i];
        assert_eq!(average_point(&xy, 1, 4), (2.0, 13.0));
        assert_eq!(average_point(&xy, 3, 5), (6.0, 19.5));
        // 1点だけのバケットの平均は、その点自身
        assert_eq!(average_point(&xy, 4, 5), (9.0, 9.0));
    }

    #[test]
    fn lttb_uses_the_average_of_the_next_bucket() {
        // 6点を4点に。バケットは [1,2] と [3,4]、最後の「次のバケット」は [5] の1点
        //
        // 2つ目のバケットでは、直前に選んだ点 (2,0) と次のバケットの平均点とで
        // 三角形を作る。正しい平均点は (5,10)（1点の合計 ÷ 1）
        //   添字3 (3,-4): |(-3)(-4) - (-1)(10)| / 2 = 11
        //   添字4 (4, 0): |(-3)( 0) - (-2)(10)| / 2 = 10   → 添字3 を選ぶ
        // 「次のバケットの合計 ÷ 現在のバケットの幅(2)」だと平均点が (2.5,5) になり、
        //   添字3: |(-0.5)(-4) - (-1)(5)| / 2 = 3.5
        //   添字4: |(-0.5)( 0) - (-2)(5)| / 2 = 5          → 添字4 を選んでしまう
        let data = [
            (0.0, 0.0), (1.0, 0.0), (2.0, 0.0),
            (3.0, -4.0), (4.0, 0.0), (5.0, 10.0),
        ];
        let indices = lttb_indices(data.len(), 4, |i| data[i]);
        assert_eq!(indices, vec![0, 2, 3, 5]);
    }

    /// 教科書どおりの LTTB を、点列をいったん全部並べる素朴な形で書いたもの
    fn reference_lttb(data: &[(f64, f64)], threshold: usize) -> Vec<usize> {
        let n = data.len();
        let mut picked = vec![0];
        let mut a = 0;
        for i in 0..(threshold - 2) {
            let avg_start = (i + 1) * (n - 2) / (threshold - 2) + 1;
            let avg_end = ((i + 2) * (n - 2) / (threshold - 2) + 1).min(n);
            let count = (avg_end - avg_start) as f64;
            let mut avg = (0.0, 0.0);
            for p in &data[avg_start..avg_end] {
                avg.0 += p.0;
                avg.1 += p.1;
            }
            avg = (avg.0 / count, avg.1 / count);

            let from = i * (n - 2) / (threshold - 2) + 1;
            let to = avg_start;
            let mut best = (-1.0, from);
            for j in from..to {
                let area = (
                    (data[a].0 - avg.0) * (data[j].1 - data[a].1)
                        - (data[a].0 - data[j].0) * (avg.1 - data[a].1)
                ).abs() * 0.5;
                if area > best.0 {
                    best = (area, j);
                }
            }
            a = best.1;
            picked.push(a);
        }
        picked.push(n - 1);
        picked
    }

    #[test]
    fn lttb_matches_the_reference_implementation() {
        // 線形合同法の疑似乱数（再現できるように固定の種）
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 33) % 1000) as f64
        };
        // バケットの幅が整数にならない組み合わせを含める
        for (n, nsample) in [(6, 4), (7, 3), (100, 7), (1000, 512), (8640, 512), (513, 512)] {
            let data: Vec<(f64, f64)> =
                (0..n).map(|i| (i as f64 * 10.0, next())).collect();
            let indices = lttb_indices(n, nsample, |i| data[i]);
            assert_eq!(indices.len(), nsample);
            assert_eq!(indices, reference_lttb(&data, nsample), "n={} nsample={}", n, nsample);
        }
    }

    #[test]
    fn point_to_json_shapes() {
        let p = UsagePoint {
            time_ms: BASE_MS + 10_023,
            cpu: Some(1.234),
            memory: None,
            read_kbps: Some(12),
            write_kbps: None,
            recv_kbps: Some(0),
            send_kbps: Some(3456),
        };
        assert_eq!(
            Series::Cpu.point_to_json(&p),
            r#"{"time":"2026-01-01T00:00:10.023Z","percentage":1.23}"#,
        );
        assert_eq!(
            Series::Memory.point_to_json(&p),
            r#"{"time":"2026-01-01T00:00:10.023Z","percentage":null}"#,
        );
        for series in [Series::IoRead, Series::IoWrite] {
            assert_eq!(
                series.point_to_json(&p),
                r#"{"time":"2026-01-01T00:00:10.023Z","readkBps":12,"writekBps":null}"#,
            );
        }
        for series in [Series::NetRecv, Series::NetSend] {
            assert_eq!(
                series.point_to_json(&p),
                r#"{"time":"2026-01-01T00:00:10.023Z","recvkBps":0,"sendkBps":3456}"#,
            );
        }
    }
}
