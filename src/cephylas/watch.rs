
use super::log;
use super::time;

const INITIAL_WAIT_MILLIS: u128 = 1000;

pub enum WatchError {
    TimeError(std::time::SystemTimeError),
    IOError(std::io::Error),
    JsonError(json::Error),
}

/// next log (summarize) timing,
/// milliseconds from 1970/1/1 00:00:00.000
struct TargetTimes {
    pub daily: u128,
    pub weekly: u128,
    pub monthly: u128,
    pub yearly: u128,
}


pub struct DailySettings {
    pub log_count: u8,
    pub unit_duration: std::time::Duration,
}
impl std::default::Default for DailySettings {
    fn default() -> Self {
        DailySettings {
            log_count: 2,
            unit_duration: std::time::Duration::from_secs(10),
        }
    }
}

pub struct WeeklySettings {
    pub log_count: u8,
    pub unit_duration: std::time::Duration,
}
impl std::default::Default for WeeklySettings {
    fn default() -> Self {
        WeeklySettings {
            log_count: 2,
            unit_duration: std::time::Duration::from_secs(60*10),
        }
    }
}

pub struct MonthlySettings {
    pub log_count: u8,
    pub unit_duration: std::time::Duration,
}
impl std::default::Default for MonthlySettings {
    fn default() -> Self {
        MonthlySettings {
            log_count: 2,
            unit_duration: std::time::Duration::from_secs(60*60),
        }
    }
}

pub struct YearlySettings {
    pub log_count: u8,
    pub unit_duration: std::time::Duration,
}
impl std::default::Default for YearlySettings {
    fn default() -> Self {
        YearlySettings {
            log_count: 2,
            unit_duration: std::time::Duration::from_secs(60*60*24),
        }
    }
}

pub struct DurationSettings {
    pub daily: DailySettings,
    pub weekly: WeeklySettings,
    pub monthly: MonthlySettings,
    pub yearly: YearlySettings,
}

impl std::default::Default for DurationSettings {
    fn default() -> Self {
        DurationSettings {
            daily: DailySettings::default(),
            weekly: WeeklySettings::default(),
            monthly: MonthlySettings::default(),
            yearly: YearlySettings::default(),
        }
    }
}

/// initialize target timings
///
/// inspired by cron_sync
fn sync(
    durations: &DurationSettings
) -> Result<TargetTimes, WatchError> {
    // daily log target time initialization
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(WatchError::TimeError)?
        .as_millis();

    let daily = now + (
        durations.daily.unit_duration.as_millis() 
      + INITIAL_WAIT_MILLIS - (now % 1000)
    ); 

    // weekly log target time initialization
    let weekly = now + (
        durations.weekly.unit_duration.as_millis()
    );
    if let Some(last_weekly) = get_last_timing("weekly")? {
        println!("last weekly found, {:?}", last_weekly);
    }
    let monthly = now + (
        durations.monthly.unit_duration.as_millis()
    );
    let yearly = now + (
        durations.yearly.unit_duration.as_millis()
    );

    Ok(TargetTimes {
        daily, weekly, monthly, yearly
    })
}

fn get_last_timing(
    path: &str
) -> Result<Option<std::time::SystemTime>, WatchError> {
    let mut file = std::fs::File::open(path)
        .map_err(WatchError::IOError)?;
    let mut contents = String::new();
    std::io::Read::read_to_string(
        &mut file, &mut contents
    ).map_err(WatchError::IOError)?;

    match contents.lines().last() {
        Some(line) => {
            let data = json::parse(line)
                .map_err(WatchError::JsonError)?;
            let timing = time::parse_time(
                &data["time"].to_string()
            ).expect("failed to convert time string to system time");
            Ok(Some(timing))
        },
        None => Ok(None),
    }
}

#[allow(unreachable_code)]
pub fn start(
    durations: &DurationSettings
) -> Result<(), WatchError> {
    let mut timings = sync(durations)?;

    loop {

        log::log_daily();

        let wait_millis = 
            timings.daily
            - std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap()
                .as_millis();
        std::thread::sleep(
            std::time::Duration::from_millis(wait_millis as u64)
        );
    }

    Ok(())
}

