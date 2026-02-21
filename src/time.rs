use std::{mem::MaybeUninit, str::FromStr};

#[cfg(test)]
#[path = "./time_tests.rs"]
mod tests;

/// A calendar date with year, month, and day components.
///
/// Represents the date portion of a TOML datetime value. Field ranges are
/// validated during parsing:
///
/// - `year`: 0–9999
/// - `month`: 1–12
/// - `day`: 1–31 (upper bound depends on month and leap year rules)
///
/// # Examples
///
/// ```
/// use toml_spanner::{Arena, DateTime};
///
/// let dt: DateTime = "2026-03-15".parse().unwrap();
/// let date = dt.date().unwrap();
/// assert_eq!(date.year, 2026);
/// assert_eq!(date.month, 3);
/// assert_eq!(date.day, 15);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    /// Calendar year (0–9999).
    pub year: u16,
    /// Month of the year (1–12).
    pub month: u8,
    /// Day of the month (1–31).
    pub day: u8,
}

/// A UTC offset attached to an offset date-time.
///
/// TOML offset date-times include a timezone offset suffix such as `Z`,
/// `+05:30`, or `-08:00`. This enum represents that offset.
///
/// # Examples
///
/// ```
/// use toml_spanner::{DateTime, TimeOffset};
///
/// let dt: DateTime = "2026-01-04T12:00:00Z".parse().unwrap();
/// assert_eq!(dt.offset(), Some(TimeOffset::Z));
///
/// let dt: DateTime = "2026-01-04T12:00:00+05:30".parse().unwrap();
/// assert_eq!(dt.offset(), Some(TimeOffset::Custom { minutes: 330 }));
///
/// let dt: DateTime = "2026-01-04T12:00:00-08:00".parse().unwrap();
/// assert_eq!(dt.offset(), Some(TimeOffset::Custom { minutes: -480 }));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOffset {
    /// UTC offset of zero, written as `Z` (or `z`) in the source.
    Z,
    /// A fixed offset from UTC, stored as a signed number of minutes.
    ///
    /// Positive values are east of UTC (e.g. `+05:30` = 330 minutes),
    /// negative values are west (e.g. `-08:00` = -480 minutes).
    Custom {
        /// Signed offset in minutes from UTC.
        minutes: i16,
    },
}

/// A time-of-day with optional sub-second precision.
///
/// Represents the time portion of a TOML datetime value. Field ranges are
/// validated during parsing:
///
/// - `hour`: 0–23
/// - `minute`: 0–59
/// - `second`: 0–60 (60 is permitted for leap seconds)
/// - `nanosecond`: 0–999999999
///
/// When seconds are omitted in the source (e.g. `12:30`), `second` defaults
/// to 0. Use [`has_seconds`](Self::has_seconds) to distinguish this from an
/// explicit `:00`.
///
/// # Examples
///
/// ```
/// use toml_spanner::DateTime;
///
/// let dt: DateTime = "14:30:05.123".parse().unwrap();
/// let time = dt.time().unwrap();
/// assert_eq!(time.hour, 14);
/// assert_eq!(time.minute, 30);
/// assert_eq!(time.second, 5);
/// assert_eq!(time.nanosecond, 123000000);
/// assert_eq!(time.subsecond_precision(), 3);
/// ```
#[derive(Clone, Copy)]
pub struct Time {
    flags: u8,
    /// Hour of the day (0–23).
    pub hour: u8,
    /// Minute of the hour (0–59).
    pub minute: u8,
    /// Second of the minute (0–60).
    pub second: u8,
    /// Sub-second component in nanoseconds (0–999999999).
    pub nanosecond: u32,
}

impl std::fmt::Debug for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Time")
            .field("hour", &self.hour)
            .field("minute", &self.minute)
            .field("second", &self.second)
            .field("nanosecond", &self.nanosecond)
            .finish()
    }
}

impl PartialEq for Time {
    fn eq(&self, other: &Self) -> bool {
        self.hour == other.hour
            && self.minute == other.minute
            && self.second == other.second
            && self.nanosecond == other.nanosecond
    }
}

impl Eq for Time {}

impl Time {
    /// Returns the number of fractional-second digits present in the source.
    ///
    /// Returns 0 when no fractional part was written (e.g. `12:30:00`),
    /// and 1–9 for `.1` through `.123456789`.
    pub fn subsecond_precision(&self) -> u8 {
        self.flags >> NANO_SHIFT
    }
    /// Returns `true` if seconds were explicitly written in the source.
    ///
    /// When the input omits seconds (e.g. `12:30`), [`second`](Self::second)
    /// is set to 0 but this method returns `false`.
    pub fn has_seconds(&self) -> bool {
        self.flags & HAS_SECONDS != 0
    }
}

/// Container for temporal values for TOML format, based on RFC 3339.
///
/// General bounds are in forced during parsing but leniently, so things like exact
/// leap second rules are not enforced, you should generally being converting
/// these time values, to a more complete time library like jiff before use.
///
/// The `DateTime` type is essentially more compact version of:
/// ```
/// use toml_spanner::{Date, Time, TimeOffset};
/// struct DateTime {
///     date: Option<Date>,
///     time: Option<Time>,
///     offset: Option<TimeOffset>,
/// }
/// ```
/// For more details on support formats inside TOML documents please reference the [TOML v1.1.0 Specification](https://toml.io/en/v1.1.0#offset-date-time).
///
/// Mapping [`DateTime`] to the TOML time kinds works like the following:
///
/// ```rust
/// #[rustfmt::skip]
/// fn datetime_to_toml_kind(value: &toml_spanner::DateTime) -> &'static str {
///     match (value.date(),value.time(),value.offset()) {
///           (Some(_date), Some(_time), Some(_offset)) => "Offset Date-Time",
///           (Some(_date), Some(_time), None         ) => "Local Date-Time",
///           (Some(_date), None       , None         ) => "Local Date",
///           (None       , Some(_time), None         ) => "Local Time",
///         _ => unreachable!("for a DateTime produced from the toml-spanner::parse"),
///     }
/// }
/// ```
///
/// # Constructing a `DateTime`
/// Generally, you should be parsing `DateTime` values from a TOML document, but for testing purposes,
/// `FromStr` is also implemented allowing for `"2026-01-04".parse::<DateTime>()`.
///
/// ```
/// use toml_spanner::{Date, Time, TimeOffset, DateTime};
/// let value: DateTime = "2026-01-04T12:30:45Z".parse().unwrap();
/// assert_eq!(value.date(), Some(Date { year: 2026, month: 1, day: 4 }));
/// assert_eq!(value.time().unwrap().minute, 30);
/// assert_eq!(value.offset(), Some(TimeOffset::Z));
/// ```
///
/// <details>
/// <summary>Toggle Jiff Conversions Examples</summary>
///
/// ```ignore
/// use jiff::civil::Date;
/// use toml_spanner::{Deserialize, Error as TomlError, Span as TomlSpan};
///
/// fn extract_date(
///     datetime: &toml_spanner::DateTime,
///     span: TomlSpan,
/// ) -> Result<jiff::civil::Date, TomlError> {
///     let Some(date) = datetime.date() else {
///         return Err(TomlError::custom("Missing date component", span));
///     };
///     // toml_spanner guartees the following inclusive ranges
///     // year: 0-9999, month: 1-12, day: 1-31
///     // making the as casts safe.
///     match jiff::civil::Date::new(date.year as i16, date.month as i8, date.day as i8) {
///         Ok(value) => Ok(value),
///         Err(err) => Err(TomlError::custom(format!("Invalid date: {err}"), span)),
///     }
/// }
///
/// fn extract_time(
///     datetime: &toml_spanner::DateTime,
///     span: TomlSpan,
/// ) -> Result<jiff::civil::Time, TomlError> {
///     let Some(time) = datetime.time() else {
///         return Err(TomlError::custom("Missing time component", span));
///     };
///     // toml_spanner guartees the following inclusive ranges
///     // hour: 0-23, minute: 0-59, second: 0-60, nanosecond: 0-999999999
///     // making the as casts safe.
///     match jiff::civil::Time::new(
///         time.hour as i8,
///         time.minute as i8,
///         time.second as i8,
///         time.nanosecond as i32,
///     ) {
///         Ok(value) => Ok(value),
///         Err(err) => Err(TomlError::custom(format!("Invalid time: {err}"), span)),
///     }
/// }
///
/// fn extract_timezone(
///     datetime: &toml_spanner::DateTime,
///     span: TomlSpan,
/// ) -> Result<jiff::tz::TimeZone, TomlError> {
///     let Some(offset) = datetime.offset() else {
///         return Err(TomlError::custom("Missing offset component", span));
///     };
///     match offset {
///         toml_spanner::TimeOffset::Z => Ok(jiff::tz::TimeZone::UTC),
///         toml_spanner::TimeOffset::Custom { minutes } => {
///             match jiff::tz::Offset::from_seconds(minutes as i32 * 60) {
///                 Ok(jiff_offset) => Ok(jiff::tz::TimeZone::fixed(jiff_offset)),
///                 Err(err) => Err(TomlError::custom(format!("Invalid offset: {err}"), span)),
///             }
///         }
///     }
/// }
///
/// fn to_jiff_date(item: &toml_spanner::Item<'_>) -> Result<jiff::civil::Date, TomlError> {
///     let Some(datetime) = item.as_datetime() else {
///         return Err(item.expected("date"));
///     };
///
///     if datetime.time().is_some() {
///         return Err(TomlError::custom(
///             "Expected lone date but found time",
///             item.span(),
///         ));
///     };
///
///     extract_date(datetime, item.span())
/// }
///
/// fn to_jiff_datetime(item: &toml_spanner::Item<'_>) -> Result<jiff::civil::DateTime, TomlError> {
///     let Some(datetime) = item.as_datetime() else {
///         return Err(item.expected("civil datetime"));
///     };
///
///     if datetime.offset().is_some() {
///         return Err(TomlError::custom(
///             "Expected naive timestamp but found offset",
///             item.span(),
///         ));
///     };
///
///     Ok(jiff::civil::DateTime::from_parts(
///         extract_date(datetime, item.span())?,
///         extract_time(datetime, item.span())?,
///     ))
/// }
///
/// fn to_jiff_timestamp(item: &toml_spanner::Item<'_>) -> Result<jiff::Timestamp, TomlError> {
///     let Some(datetime) = item.as_datetime() else {
///         return Err(item.expected("timestamp"));
///     };
///     let civil = jiff::civil::DateTime::from_parts(
///         extract_date(datetime, item.span())?,
///         extract_time(datetime, item.span())?,
///     );
///     let timezone = extract_timezone(datetime, item.span())?;
///     match timezone.to_timestamp(civil) {
///         Ok(value) => Ok(value),
///         Err(err) => Err(TomlError::custom(
///             format!("Invalid timestamp: {err}"),
///             item.span(),
///         )),
///     }
/// }
///
/// #[derive(Debug)]
/// pub struct TimeConfig {
///     pub date: Date,
///     pub datetime: jiff::civil::DateTime,
///     pub timestamp: jiff::Timestamp,
/// }
///
/// impl<'de> Deserialize<'de> for TimeConfig {
///     fn deserialize(
///         ctx: &mut toml_spanner::Context<'de>,
///         value: &toml_spanner::Item<'de>,
///     ) -> Result<Self, toml_spanner::Failed> {
///         let mut th = value.table_helper(ctx)?;
///         let config = TimeConfig {
///             date: th.required_mapped("date", to_jiff_date)?,
///             datetime: th.required_mapped("datetime", to_jiff_datetime)?,
///             timestamp: th.required_mapped("timestamp", to_jiff_timestamp)?,
///         };
///         Ok(config)
///     }
/// }
///
/// fn main() {
///     let arena = toml_spanner::Arena::new();
///
///     let toml_doc = r#"
///         date = 1997-02-28
///         datetime = 2066-01-30T14:45:00
///         timestamp = 3291-12-01T00:45:00Z
///     "#;
///     let mut root = toml_spanner::parse(toml_doc, &arena).unwrap();
///     let config: TimeConfig = root.deserialize().unwrap();
///     println!("{:#?}", config);
/// }
/// ```
///
/// </details>
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct DateTime {
    date: Date,

    flags: u8,

    hour: u8,
    minute: u8,
    seconds: u8,

    offset_minutes: i16,
    nanos: u32,
}

const HAS_DATE: u8 = 1 << 0;
const HAS_TIME: u8 = 1 << 1;
const HAS_SECONDS: u8 = 1 << 2;
const NANO_SHIFT: u8 = 4;

fn is_leap_year(year: u16) -> bool {
    (((year as u64 * 1073750999) as u32) & 3221352463) <= 126976
}

fn days_in_month(year: u16, month: u8) -> u8 {
    const DAYS: [u8; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if month == 2 && is_leap_year(year) {
        29
    } else {
        DAYS[month as usize]
    }
}

/// Error returned when parsing a [`DateTime`] from a string via [`FromStr`].
#[non_exhaustive]
#[derive(Debug)]
pub enum DateTimeError {
    /// The input string is not a valid TOML datetime.
    Invalid,
}

impl std::fmt::Display for DateTimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <DateTimeError as std::fmt::Debug>::fmt(self, f)
    }
}

impl std::error::Error for DateTimeError {}

impl FromStr for DateTime {
    type Err = DateTimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DateTime::munch(s.as_bytes())
            .filter(|(amount, _)| *amount == s.len())
            .map(|(_, dt)| dt)
            .ok_or(DateTimeError::Invalid)
    }
}

impl DateTime {
    /// Maximum number of bytes produced by [`DateTime::format`].
    ///
    /// Use this to size the [`MaybeUninit`] buffer passed to [`DateTime::format`].
    ///
    /// [`MaybeUninit`]: std::mem::MaybeUninit
    pub const MAX_FORMAT_LEN: usize = 40;
    /// Returns the time component, or [`None`] for a local-date value.
    pub fn time(&self) -> Option<Time> {
        if self.flags & HAS_TIME != 0 {
            Some(Time {
                flags: self.flags,
                hour: self.hour,
                minute: self.minute,
                second: self.seconds,
                nanosecond: self.nanos,
            })
        } else {
            None
        }
    }
    pub(crate) fn munch(input: &[u8]) -> Option<(usize, DateTime)> {
        enum State {
            Year,
            Month,
            Day,
            Hour,
            Minute,
            Second,
            Frac,
            OffHour,
            OffMin,
        }
        let mut state = match input {
            [_, _, b':', _, _, ..] => State::Hour,
            [_, _, _, _, b'-', _, _, b'-', ..] => State::Year,
            _ => return None,
        };

        let mut value = DateTime {
            date: Date {
                year: 0,
                month: 0,
                day: 0,
            },
            flags: 0,
            hour: 0,
            minute: 0,
            seconds: 0,
            offset_minutes: i16::MIN,
            nanos: 0,
        };

        let mut current = 0u32;
        let mut len = 0u32;
        let mut off_sign: i16 = 1;
        let mut off_hour: u8 = 0;
        let mut i = 0usize;
        let mut valid = false;

        'outer: loop {
            let byte = input.get(i).copied().unwrap_or(0);
            if byte.is_ascii_digit() {
                len += 1;
                if len <= 9 {
                    current = current * 10 + (byte - b'0') as u32;
                }
                i += 1;
                continue;
            }
            'next: {
                match state {
                    State::Year => {
                        if len != 4 || byte != b'-' {
                            break 'outer;
                        }
                        value.date.year = current as u16;
                        state = State::Month;
                        break 'next;
                    }
                    State::Month => {
                        let m = current as u8;
                        if len != 2 || byte != b'-' || m < 1 || m > 12 {
                            break 'outer;
                        }
                        value.date.month = m;
                        state = State::Day;
                        break 'next;
                    }
                    State::Day => {
                        let d = current as u8;
                        if len != 2 || d < 1 || d > days_in_month(value.date.year, value.date.month)
                        {
                            break 'outer;
                        }
                        value.date.day = d;
                        value.flags |= HAS_DATE;
                        if byte == b'T'
                            || byte == b't'
                            || (byte == b' '
                                && input.get(i + 1).is_some_and(|b| b.is_ascii_digit()))
                        {
                            state = State::Hour;
                            break 'next;
                        } else {
                            valid = true;
                            break 'outer;
                        }
                    }
                    State::Hour => {
                        let h = current as u8;
                        if len != 2 || byte != b':' || h > 23 {
                            break 'outer;
                        }
                        value.hour = h;
                        state = State::Minute;
                        break 'next;
                    }
                    State::Minute => {
                        let m = current as u8;
                        if len != 2 || m > 59 {
                            break 'outer;
                        }
                        value.minute = m;
                        value.flags |= HAS_TIME;
                        if byte == b':' {
                            state = State::Second;
                            break 'next;
                        } else {
                            // fallthorugh to check offset
                        }
                    }
                    State::Second => {
                        let s = current as u8;
                        // Note: Second is allowed to be 60, for leap second rule.
                        if len != 2 || s > 60 {
                            break 'outer;
                        }
                        value.seconds = s;
                        value.flags |= HAS_SECONDS;
                        if byte == b'.' {
                            state = State::Frac;
                            break 'next;
                        } else {
                            // fallthrough to check outer
                        }
                    }
                    State::Frac => {
                        if len == 0 {
                            break 'outer;
                        }
                        let digit_count = if len > 9 { 9u8 } else { len as u8 };
                        let mut nanos = current;
                        let mut s = digit_count;
                        while s < 9 {
                            nanos *= 10;
                            s += 1;
                        }
                        value.nanos = nanos;
                        value.flags |= digit_count << NANO_SHIFT;
                        // fallthrough to check outer
                    }
                    State::OffHour => {
                        let h = current as u8;
                        if len != 2 || byte != b':' || h > 23 {
                            break 'outer;
                        }
                        off_hour = h;
                        state = State::OffMin;
                        break 'next;
                    }
                    State::OffMin => {
                        if len != 2 || current > 59 {
                            break 'outer;
                        }
                        value.offset_minutes = off_sign * (off_hour as i16 * 60 + current as i16);
                        valid = true;
                        break 'outer;
                    }
                }
                match byte {
                    b'Z' | b'z' => {
                        value.offset_minutes = i16::MAX;
                        i += 1;
                        valid = true;
                        break 'outer;
                    }
                    b'+' => {
                        off_sign = 1;
                        state = State::OffHour;
                    }
                    b'-' => {
                        off_sign = -1;
                        state = State::OffHour;
                    }
                    _ => {
                        valid = true;
                        break 'outer;
                    }
                }
            }
            i += 1;
            current = 0;
            len = 0;
        }
        if !valid || (value.flags & HAS_DATE == 0 && value.offset_minutes != i16::MIN) {
            return None;
        }
        Some((i, value))
    }

    /// Formats this datetime into the provided buffer and returns the result as a `&str`.
    ///
    /// The output follows RFC 3339 formatting and matches the TOML serialization
    /// of the value. The caller must supply an uninitializebuffer of [`MAX_FORMAT_LEN`] bytes;
    /// the returned `&str` borrows from that buffer, starting from the beginning.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::mem::MaybeUninit;
    /// use toml_spanner::DateTime;
    ///
    /// let dt: DateTime = "2026-01-04T12:30:45Z".parse().unwrap();
    /// let mut buf = MaybeUninit::uninit();
    /// assert_eq!(dt.format(&mut buf), "2026-01-04T12:30:45Z");
    /// assert_eq!(size_of_val(&buf), DateTime::MAX_FORMAT_LEN);
    /// ```
    pub fn format<'a>(&self, buf: &'a mut MaybeUninit<[u8; DateTime::MAX_FORMAT_LEN]>) -> &'a str {
        #[inline(always)]
        fn write_byte(
            buf: &mut [MaybeUninit<u8>; DateTime::MAX_FORMAT_LEN],
            pos: &mut usize,
            b: u8,
        ) {
            buf[*pos].write(b);
            *pos += 1;
        }

        #[inline(always)]
        fn write_2(
            buf: &mut [MaybeUninit<u8>; DateTime::MAX_FORMAT_LEN],
            pos: &mut usize,
            val: u8,
        ) {
            buf[*pos].write(b'0' + val / 10);
            buf[*pos + 1].write(b'0' + val % 10);
            *pos += 2;
        }

        #[inline(always)]
        fn write_4(
            buf: &mut [MaybeUninit<u8>; DateTime::MAX_FORMAT_LEN],
            pos: &mut usize,
            val: u16,
        ) {
            buf[*pos].write(b'0' + (val / 1000) as u8);
            buf[*pos + 1].write(b'0' + ((val / 100) % 10) as u8);
            buf[*pos + 2].write(b'0' + ((val / 10) % 10) as u8);
            buf[*pos + 3].write(b'0' + (val % 10) as u8);
            *pos += 4;
        }

        #[inline(always)]
        fn write_frac(
            buf: &mut [MaybeUninit<u8>; DateTime::MAX_FORMAT_LEN],
            pos: &mut usize,
            nanos: u32,
            digit_count: u8,
        ) {
            let mut val = nanos;
            let mut i: usize = 8;
            loop {
                buf[*pos + i].write(b'0' + (val % 10) as u8);
                val /= 10;
                if i == 0 {
                    break;
                }
                i -= 1;
            }
            *pos += digit_count as usize;
        }

        // SAFETY: MaybeUninit<u8> has identical layout to u8
        let buf: &mut [MaybeUninit<u8>; Self::MAX_FORMAT_LEN] = unsafe {
            &mut *buf
                .as_mut_ptr()
                .cast::<[MaybeUninit<u8>; Self::MAX_FORMAT_LEN]>()
        };
        let mut pos: usize = 0;

        if self.flags & HAS_DATE != 0 {
            write_4(buf, &mut pos, self.date.year);
            write_byte(buf, &mut pos, b'-');
            write_2(buf, &mut pos, self.date.month);
            write_byte(buf, &mut pos, b'-');
            write_2(buf, &mut pos, self.date.day);

            if self.flags & HAS_TIME != 0 {
                write_byte(buf, &mut pos, b'T');
            }
        }

        if self.flags & HAS_TIME != 0 {
            write_2(buf, &mut pos, self.hour);
            write_byte(buf, &mut pos, b':');
            write_2(buf, &mut pos, self.minute);
            write_byte(buf, &mut pos, b':');
            write_2(buf, &mut pos, self.seconds);

            if self.flags & HAS_SECONDS != 0 {
                let digit_count = (self.flags >> NANO_SHIFT) & 0xF;
                if digit_count > 0 {
                    write_byte(buf, &mut pos, b'.');
                    write_frac(buf, &mut pos, self.nanos, digit_count);
                }
            }

            if self.offset_minutes != i16::MIN {
                if self.offset_minutes == i16::MAX {
                    write_byte(buf, &mut pos, b'Z');
                } else {
                    let (sign, abs) = if self.offset_minutes < 0 {
                        (b'-', (-self.offset_minutes) as u16)
                    } else {
                        (b'+', self.offset_minutes as u16)
                    };
                    write_byte(buf, &mut pos, sign);
                    write_2(buf, &mut pos, (abs / 60) as u8);
                    write_byte(buf, &mut pos, b':');
                    write_2(buf, &mut pos, (abs % 60) as u8);
                }
            }
        }

        // SAFETY: buf[..pos] has been fully initialized by the write calls above,
        // and all written bytes are valid ASCII digits/punctuation.
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(buf.as_ptr().cast(), pos))
        }
    }

    /// Returns the date component, or [`None`] for a local-time value.
    pub fn date(&self) -> Option<Date> {
        if self.flags & HAS_DATE != 0 {
            Some(self.date)
        } else {
            None
        }
    }

    /// Returns the UTC offset, or [`None`] for local date-times and local times.
    pub fn offset(&self) -> Option<TimeOffset> {
        match self.offset_minutes {
            i16::MAX => Some(TimeOffset::Z),
            i16::MIN => None,
            minutes => Some(TimeOffset::Custom { minutes }),
        }
    }
}
