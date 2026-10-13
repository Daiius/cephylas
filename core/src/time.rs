
//! ISO 8601 (UTC) の文字列と、エポックからのミリ秒との相互変換
//!
//! 外部 crate を使わないために自前で書いています
//! 扱うのは Docker が返す形（`2026-01-01T00:00:10.023034093Z`）だけです

use super::error;

const MILLIS_PER_SECOND: i64 = 1000;
const MILLIS_PER_DAY: i64 = 86_400 * MILLIS_PER_SECOND;

/// 西暦の年月日を、1970-01-01 からの日数に直します
/// (Howard Hinnant の days_from_civil。先発グレゴリオ暦)
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // 3月始まりの年として数えると、うるう日が年の最後に来る
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y.rem_euclid(400);
    let month_from_march = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

/// 1970-01-01 からの日数を、西暦の (年, 月, 日) に直します
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (
        day_of_era - day_of_era / 1460 + day_of_era / 36_524
            - day_of_era / 146_096
    ) / 365;
    let day_of_year =
        day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => if is_leap_year(year) { 29 } else { 28 },
    }
}

/// 数字だけからなるバイト列を数値にします（符号や空白は受け付けない）
fn parse_digits(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(bytes.iter().fold(0_i64, |acc, b| acc * 10 + (b - b'0') as i64))
}

fn parse_millis(time_str: &str) -> Option<i64> {
    let b = time_str.as_bytes();
    // "YYYY-MM-DDTHH:MM:SS" (19バイト) + 小数秒(任意) + "Z"
    if b.len() < 20 {
        return None;
    }
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T'
        || b[13] != b':' || b[16] != b':'
    {
        return None;
    }
    let year = parse_digits(&b[0..4])?;
    let month = parse_digits(&b[5..7])?;
    let day = parse_digits(&b[8..10])?;
    let hour = parse_digits(&b[11..13])?;
    let minute = parse_digits(&b[14..16])?;
    let second = parse_digits(&b[17..19])?;

    let (fraction, zone) = b[19..].split_at(b.len() - 20);
    if zone != b"Z" {
        return None;
    }
    let millis = match fraction {
        [] => 0,
        [b'.', digits @ ..] if (1..=9).contains(&digits.len()) => {
            // ミリ秒より細かい桁は切り捨てる
            let mut padded = [b'0'; 3];
            for (dst, src) in padded.iter_mut().zip(digits) {
                *dst = *src;
            }
            parse_digits(digits)?; // 4桁目以降も数字であること
            parse_digits(&padded)?
        }
        _ => return None,
    };

    if !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || hour > 23 || minute > 59 || second > 59
    {
        return None;
    }

    let seconds_of_day = (hour * 60 + minute) * 60 + second;
    Some(
        days_from_civil(year, month, day) * MILLIS_PER_DAY
            + seconds_of_day * MILLIS_PER_SECOND
            + millis
    )
}

/// ISO 8601 の文字列を、エポックからのミリ秒に直します
///
/// 受け付けるのは UTC で `Z` 終端のものだけです
/// 小数秒は 0〜9 桁で、ミリ秒より細かい桁は切り捨てます
pub fn iso8601_to_millis(time_str: &str) -> Result<i64, error::Error> {
    parse_millis(time_str).ok_or_else(|| error::Error::OtherError(
        format!("invalid time format: {:?}", time_str)
    ))
}

/// エポックからのミリ秒を、ミリ秒精度の ISO 8601 の文字列に直します
/// (例: `2026-01-01T00:00:10.023Z`)
pub fn millis_to_iso8601(millis: i64) -> String {
    let days = millis.div_euclid(MILLIS_PER_DAY);
    let millis_of_day = millis.rem_euclid(MILLIS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let seconds_of_day = millis_of_day / MILLIS_PER_SECOND;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day,
        seconds_of_day / 3600,
        (seconds_of_day / 60) % 60,
        seconds_of_day % 60,
        millis_of_day % MILLIS_PER_SECOND,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(time_str: &str) -> i64 {
        iso8601_to_millis(time_str).unwrap()
    }

    #[test]
    fn known_instants() {
        assert_eq!(millis("1970-01-01T00:00:00Z"), 0);
        assert_eq!(millis("1970-01-01T00:00:00.001Z"), 1);
        assert_eq!(millis("1970-01-02T00:00:00Z"), 86_400_000);
        assert_eq!(millis("2000-01-01T00:00:00Z"), 946_684_800_000);
        assert_eq!(millis("2026-01-01T00:00:00Z"), 1_767_225_600_000);
        // エポックより前も壊れない
        assert_eq!(millis("1969-12-31T23:59:59.999Z"), -1);
        assert_eq!(millis_to_iso8601(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn fraction_digits() {
        let base = millis("2026-01-01T00:00:10Z");
        // 0桁 / 3桁 / 9桁
        assert_eq!(millis_to_iso8601(base), "2026-01-01T00:00:10.000Z");
        assert_eq!(millis("2026-01-01T00:00:10.023Z"), base + 23);
        assert_eq!(millis("2026-01-01T00:00:10.023034093Z"), base + 23);
        // 桁が少ないときは右を0で埋める / ミリ秒未満は切り捨てる
        assert_eq!(millis("2026-01-01T00:00:10.5Z"), base + 500);
        assert_eq!(millis("2026-01-01T00:00:10.02Z"), base + 20);
        assert_eq!(millis("2026-01-01T00:00:10.9999Z"), base + 999);
        assert_eq!(millis("2026-01-01T00:00:10.000999999Z"), base);
    }

    #[test]
    fn round_trip() {
        for time_str in [
            "1970-01-01T00:00:00.000Z",
            "1999-12-31T23:59:59.999Z",
            "2024-02-29T12:34:56.789Z",
            "2026-01-01T00:00:10.023Z",
            "2026-12-31T23:59:59.000Z",
            "2100-03-01T00:00:00.001Z",
            "0001-01-01T00:00:00.000Z",
            "9999-12-31T23:59:59.999Z",
        ] {
            assert_eq!(millis_to_iso8601(millis(time_str)), time_str);
        }
        // 1日ずつ 600 年分を往復させる（うるう年の規則を全部通る）
        let start = millis("1900-01-01T00:00:00Z");
        for day in 0..(600 * 366) {
            let t = start + day * MILLIS_PER_DAY + 12_345;
            assert_eq!(millis(&millis_to_iso8601(t)), t);
        }
    }

    #[test]
    fn leap_years() {
        // 2024年はうるう年
        assert_eq!(
            millis("2024-03-01T00:00:00Z") - millis("2024-02-28T00:00:00Z"),
            2 * MILLIS_PER_DAY,
        );
        assert!(iso8601_to_millis("2024-02-29T00:00:00Z").is_ok());
        assert_eq!(
            millis_to_iso8601(millis("2024-02-28T23:59:59.999Z") + 1),
            "2024-02-29T00:00:00.000Z",
        );
        // 2100年は平年（100で割り切れ、400で割り切れない）
        assert_eq!(
            millis("2100-03-01T00:00:00Z") - millis("2100-02-28T00:00:00Z"),
            MILLIS_PER_DAY,
        );
        assert!(iso8601_to_millis("2100-02-29T00:00:00Z").is_err());
        assert_eq!(
            millis_to_iso8601(millis("2100-02-28T23:59:59.999Z") + 1),
            "2100-03-01T00:00:00.000Z",
        );
        // 2000年はうるう年（400で割り切れる）、2023年は平年
        assert!(iso8601_to_millis("2000-02-29T00:00:00Z").is_ok());
        assert!(iso8601_to_millis("2023-02-29T00:00:00Z").is_err());
    }

    #[test]
    fn year_boundary() {
        let last = millis("2025-12-31T23:59:59.999Z");
        let first = millis("2026-01-01T00:00:00.000Z");
        assert_eq!(first - last, 1);
        assert_eq!(millis_to_iso8601(last + 1), "2026-01-01T00:00:00.000Z");
        // うるう年の年またぎ（366日）
        assert_eq!(
            millis("2025-01-01T00:00:00Z") - millis("2024-01-01T00:00:00Z"),
            366 * MILLIS_PER_DAY,
        );
    }

    #[test]
    fn invalid_inputs_are_errors() {
        for time_str in [
            "",
            "not a time",
            "2026-01-01",
            "2026-01-01T00:00:00",            // Z が無い
            "2026-01-01T00:00:00+09:00",      // UTC 以外
            "2026-01-01T00:00:00z",
            "2026-01-01 00:00:00Z",           // T が無い
            "2026/01/01T00:00:00Z",
            "2026-01-01Txx:yy:zzZ",
            "2026-01-01T00:00:00.Z",          // 小数点だけ
            "2026-01-01T00:00:00.1234567890Z", // 小数秒が10桁
            "2026-01-01T00:00:00.12a4Z",
            "2026-01-01T00:00:00,123Z",
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-01-00T00:00:00Z",
            "2026-01-32T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:60Z",
            "+026-01-01T00:00:00Z",
            "2026-01-01T00:00:00Z ",
            " 2026-01-01T00:00:00Z",
            "２０２６-01-01T00:00:00Z",       // 全角（複数バイト文字）
        ] {
            assert!(iso8601_to_millis(time_str).is_err(), "{:?}", time_str);
        }
    }

    #[test]
    fn formatting_never_panics() {
        for t in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
            assert!(millis_to_iso8601(t).ends_with('Z'));
        }
    }
}
