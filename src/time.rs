
use libc;

const TIME_FORMAT: &[u8] = b"%Y-%m-%d %H:%M:%S\0";

#[derive(Debug)]
pub enum TimeError {
    SystemTimeError(std::time::SystemTimeError),
    FormatError(std::ffi::FromBytesWithNulError),
    ConversionError,
    NullError(std::ffi::NulError),
}
impl std::error::Error for TimeError {}
impl std::fmt::Display for TimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl From<std::time::SystemTimeError> for TimeError {
    fn from(value: std::time::SystemTimeError) -> Self {
        TimeError::SystemTimeError(value)
    }
}

pub fn format_time(
    time: &std::time::SystemTime
) -> Result<String, TimeError> {
    let seconds: libc::time_t = time
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(TimeError::SystemTimeError)?
        .as_secs() as libc::time_t;

    unsafe {
        let mut tm = libc::tm {
            tm_sec: 0, tm_min: 0, tm_hour: 0,
            tm_mday: 0, tm_mon: 0, tm_year: 0,
            tm_wday: 0, tm_yday: 0, tm_isdst: 0,
            tm_zone: std::ptr::null(),
            tm_gmtoff: 0,
        };
        libc::localtime_r(
            &seconds, 
            &mut tm
        );
            
        let mut buffer = [0u8; 64];
        let format = std::ffi::CStr::from_bytes_with_nul(
            TIME_FORMAT
        ).map_err(TimeError::FormatError)?;
        libc::strftime(
            buffer.as_mut_ptr() as *mut libc::c_char,
            buffer.len(),
            format.as_ptr(),
            &tm
        );

        let cstr = std::ffi::CStr::from_ptr(
            buffer.as_ptr() as *const libc::c_char
        );
        
        cstr.to_str()
            .map(|s| s.to_string())
            .map_err(|_| TimeError::ConversionError)
    }
}

pub fn parse_time(
    s: &str
) -> Result<std::time::SystemTime, TimeError> {
    unsafe {
        let cstr = std::ffi::CString::new(s)
            .map_err(TimeError::NullError)?;
        let mut tm = libc::tm {
            tm_sec: 0, tm_min: 0, tm_hour: 0,
            tm_mday: 0, tm_mon: 0, tm_year: 0,
            tm_wday: 0, tm_yday: 0, tm_isdst: 0,
            tm_zone: std::ptr::null(),
            tm_gmtoff: 0,
        };
        let format = std::ffi::CStr::from_bytes_with_nul(
            TIME_FORMAT
        ).map_err(TimeError::FormatError)?;

        if libc::strptime(
            cstr.as_ptr(),
            format.as_ptr(),
            &mut tm
        ).is_null() {
            return Err(TimeError::ConversionError);
        }

        let timestamp = libc::mktime(&mut tm);
        if timestamp == -1 {
            return Err(TimeError::ConversionError);
        }

        Ok(std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64))
    }
}

