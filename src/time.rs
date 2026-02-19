use std::{i16, mem::MaybeUninit};

#[derive(Clone, Copy)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOffset {
    /// A suffix which, when applied to a time, denotes a UTC offset of 00:00;
    /// often spoken “Zulu” from the ICAO phonetic alphabet representation of the letter “Z”.
    /// RFC 3339 section 2
    Z,
    /// Offset between local time and UTC
    Custom { minutes: i16 },
}

#[derive(Clone, Copy)]
pub struct Time {
    flags: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub nanosecond: u32,
}

impl Time {
    /// Number of digits in the original fractional seconds, if any. 0 if no
    pub fn subsecond_precision(&self) -> u8 {
        self.flags >> NANO_SHIFT
    }
    /// Defines whethers seconds are explictly present in the original input, or
    /// the 0 was just chosen as default.
    pub fn has_seconds(&self) -> bool {
        self.flags & HAS_SECONDS != 0
    }
}

/// Container for temporal times for TOML format, based on RFC 3339
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct Datetime {
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

pub const MAX_FORMAT_LEN: usize = 48;

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

#[inline(always)]
unsafe fn write_byte(ptr: *mut u8, pos: &mut usize, b: u8) {
    unsafe {
        *ptr.add(*pos) = b;
    }
    *pos += 1;
}

#[inline(always)]
unsafe fn write_2(ptr: *mut u8, pos: &mut usize, val: u8) {
    unsafe {
        *ptr.add(*pos) = b'0' + val / 10;
        *ptr.add(*pos + 1) = b'0' + val % 10;
    }
    *pos += 2;
}

#[inline(always)]
unsafe fn write_4(ptr: *mut u8, pos: &mut usize, val: u16) {
    unsafe {
        *ptr.add(*pos) = b'0' + (val / 1000) as u8;
        *ptr.add(*pos + 1) = b'0' + ((val / 100) % 10) as u8;
        *ptr.add(*pos + 2) = b'0' + ((val / 10) % 10) as u8;
        *ptr.add(*pos + 3) = b'0' + (val % 10) as u8;
    }
    *pos += 4;
}

#[inline(always)]
unsafe fn write_frac(ptr: *mut u8, pos: &mut usize, nanos: u32, nd: u8) {
    let mut val = nanos;
    let mut i: usize = 8;
    loop {
        unsafe {
            *ptr.add(*pos + i) = b'0' + (val % 10) as u8;
        }
        val /= 10;
        if i == 0 {
            break;
        }
        i -= 1;
    }
    *pos += nd as usize;
}

impl Datetime {
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
    pub(crate) fn munch(input: &[u8]) -> Option<(usize, Datetime)> {
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

        let mut value = Datetime {
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
                        if byte == b'T' || byte == b't' {
                            state = State::Hour;
                            break 'next;
                        } else if byte == b' '
                            && input.get(i + 1).is_some_and(|b| b.is_ascii_digit())
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
                        value.minute = m as u8;
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
                        let nd = if len > 9 { 9u8 } else { len as u8 };
                        let mut nanos = current;
                        let mut s = nd;
                        while s < 9 {
                            nanos *= 10;
                            s += 1;
                        }
                        value.nanos = nanos;
                        value.flags |= (nd as u8) << NANO_SHIFT;
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

    pub fn format<'a>(&self, buf: &'a mut MaybeUninit<[u8; MAX_FORMAT_LEN]>) -> &'a str {
        let ptr = buf.as_mut_ptr() as *mut u8;
        let mut pos: usize = 0;

        unsafe {
            if self.flags & HAS_DATE != 0 {
                write_4(ptr, &mut pos, self.date.year);
                write_byte(ptr, &mut pos, b'-');
                write_2(ptr, &mut pos, self.date.month);
                write_byte(ptr, &mut pos, b'-');
                write_2(ptr, &mut pos, self.date.day);

                if self.flags & HAS_TIME != 0 {
                    write_byte(ptr, &mut pos, b'T');
                }
            }

            if self.flags & HAS_TIME != 0 {
                write_2(ptr, &mut pos, self.hour);
                write_byte(ptr, &mut pos, b':');
                write_2(ptr, &mut pos, self.minute);
                write_byte(ptr, &mut pos, b':');
                write_2(ptr, &mut pos, self.seconds);

                if self.flags & HAS_SECONDS != 0 {
                    let nd = ((self.flags >> NANO_SHIFT) & 0xF) as u8;
                    if nd > 0 {
                        write_byte(ptr, &mut pos, b'.');
                        write_frac(ptr, &mut pos, self.nanos, nd);
                    }
                }

                if self.offset_minutes != i16::MIN {
                    if self.offset_minutes == 0 || self.offset_minutes == i16::MAX {
                        write_byte(ptr, &mut pos, b'Z');
                    } else {
                        let (sign, abs) = if self.offset_minutes < 0 {
                            (b'-', (-self.offset_minutes) as u16)
                        } else {
                            (b'+', self.offset_minutes as u16)
                        };
                        write_byte(ptr, &mut pos, sign);
                        write_2(ptr, &mut pos, (abs / 60) as u8);
                        write_byte(ptr, &mut pos, b':');
                        write_2(ptr, &mut pos, (abs % 60) as u8);
                    }
                }
            }

            std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, pos))
        }
    }

    pub fn date(&self) -> Option<Date> {
        if self.flags & HAS_DATE != 0 {
            Some(self.date)
        } else {
            None
        }
    }

    pub fn offset(&self) -> Option<TimeOffset> {
        match self.offset_minutes {
            i16::MAX => Some(TimeOffset::Z),
            i16::MIN => None,
            minutes => Some(TimeOffset::Custom { minutes }),
        }
    }
}

#[cfg(test)]
mod test {
    use std::mem::MaybeUninit;

    use super::*;

    fn roundtrip(input: &str) {
        let (amount, result) = Datetime::munch(input.as_bytes()).unwrap();
        assert_eq!(amount, input.len(), "consumed wrong amount for {input:?}");
        let mut buf = MaybeUninit::uninit();
        let output = result.format(&mut buf);
        assert_eq!(input, output, "roundtrip mismatch for {input:?}");
    }

    fn roundtrip_lossy(input: &str, expected: &str) {
        let (amount, result) = Datetime::munch(input.as_bytes()).unwrap();
        assert_eq!(amount, input.len(), "consumed wrong amount for {input:?}");
        let mut buf = MaybeUninit::uninit();
        let output = result.format(&mut buf);
        assert_eq!(expected, output, "roundtrip mismatch for {input:?}");
    }

    #[track_caller]
    fn expect_err(input: &str) {
        let result = Datetime::munch(input.as_bytes());
        assert!(result.is_none(), "for {input:?}");
    }

    fn parse_ok(input: &str) -> (usize, Datetime) {
        Datetime::munch(input.as_bytes()).unwrap()
    }

    // ── exact roundtrip ─────────────────────────────────────────────

    #[test]
    fn perfect_roundtrip_examples() {
        let inputs = &[
            "1979-05-27T07:32:00Z",
            "1979-05-27T07:32:00Z",
            "1979-05-27T00:32:00-23:00",
            "2000-12-17T00:32:00.5-07:00",
            "1979-05-27T00:32:00.999999+21:20",
            "1979-05-27T07:32:00Z",
            "1979-05-27T07:32:00",
            "1979-05-27T07:32:00.5",
            "1979-05-27T07:32:00.999999999",
            "1979-05-27T07:32:00.123456789",
            "1979-05-27",
            "07:32:00",
            "00:32:00.5",
            "00:32:00.999999",
        ];
        for input in inputs {
            roundtrip(input);
        }
    }

    #[test]
    fn lossy_roundtrip() {
        // Spaces aren't preserved; we always separate with 'T'
        roundtrip_lossy("1979-05-27 07:32:00Z", "1979-05-27T07:32:00Z");
        roundtrip_lossy("2000-01-01 00:00:00", "2000-01-01T00:00:00");
        roundtrip_lossy("1999-12-31 23:59:59.9", "1999-12-31T23:59:59.9");
        roundtrip_lossy("2024-02-29 12:00+05:30", "2024-02-29T12:00:00+05:30");

        // No-seconds inputs always format with :00
        roundtrip_lossy("1979-05-27T07:32Z", "1979-05-27T07:32:00Z");
        roundtrip_lossy("1979-05-27T07:32-07:00", "1979-05-27T07:32:00-07:00");
        roundtrip_lossy("9999-12-29T07:32", "9999-12-29T07:32:00");
        roundtrip_lossy("00:00", "00:00:00");
        roundtrip_lossy("23:59", "23:59:00");
        roundtrip_lossy("12:30", "12:30:00");

        // Lowercase t/z are accepted
        roundtrip_lossy("1987-07-05t17:45:00z", "1987-07-05T17:45:00Z");
        roundtrip_lossy("1987-07-05t17:45:00", "1987-07-05T17:45:00");
    }

    // ── EOF / too-short inputs ──────────────────────────────────────

    #[test]
    fn eof_on_empty() {
        expect_err("");
    }

    #[test]
    fn eof_on_short_inputs() {
        expect_err("1");
        expect_err("12");
        expect_err("12:");
        expect_err("1979");
    }

    // ── date-only parsing ───────────────────────────────────────────

    #[test]
    fn date_only_basic() {
        roundtrip("2000-01-01");
        roundtrip("9999-12-31");
        roundtrip("0000-01-01");
        roundtrip("0001-06-15");
    }

    #[test]
    fn date_all_months() {
        let days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        for (m, &max_day) in days.iter().enumerate() {
            let month = m + 1;
            let s = format!("2023-{month:02}-{max_day:02}");
            roundtrip(&s);
            let s = format!("2023-{month:02}-01");
            roundtrip(&s);
        }
    }

    #[test]
    fn date_leap_year_feb29() {
        roundtrip("2000-02-29"); // divisible by 400
        roundtrip("2024-02-29"); // divisible by 4, not 100
        roundtrip("1600-02-29");
        roundtrip("0004-02-29");
    }

    #[test]
    fn date_non_leap_year_feb29() {
        expect_err("2023-02-29");
        expect_err("1900-02-29"); // divisible by 100 not 400
        expect_err("2100-02-29");
    }

    #[test]
    fn date_month_out_of_range() {
        expect_err("2023-00-01");
        expect_err("2023-13-01");
    }

    #[test]
    fn date_day_out_of_range() {
        expect_err("2023-01-00");
        expect_err("2023-01-32");
        expect_err("2023-04-31");
        expect_err("2023-06-31");
        expect_err("2023-02-30");
    }

    #[test]
    fn date_invalid_separators() {
        expect_err("2023/01/01");
        expect_err("2023-01/01");
        expect_err("20230101"); // no separator after 4 digits
    }

    #[test]
    fn date_wrong_digit_counts() {
        expect_err("202-01-01"); // 3-digit year
        expect_err("2023-1-01"); // 1-digit month
        expect_err("2023-01-1"); // 1-digit day
    }

    // ── time-only parsing ───────────────────────────────────────────

    #[test]
    fn time_only_basic() {
        roundtrip("00:00:00");
        roundtrip("23:59:59");
        roundtrip("12:30:45");
    }

    #[test]
    fn time_only_no_seconds() {
        roundtrip_lossy("00:00", "00:00:00");
        roundtrip_lossy("23:59", "23:59:00");
        roundtrip_lossy("12:30", "12:30:00");
    }

    #[test]
    fn time_only_with_frac() {
        roundtrip("12:30:45.1");
        roundtrip("12:30:45.12");
        roundtrip("12:30:45.123");
        roundtrip("12:30:45.1234");
        roundtrip("12:30:45.12345");
        roundtrip("12:30:45.123456");
        roundtrip("12:30:45.1234567");
        roundtrip("12:30:45.12345678");
        roundtrip("12:30:45.123456789");
    }

    #[test]
    fn time_hour_out_of_range() {
        expect_err("24:00:00");
        expect_err("99:00:00");
    }

    #[test]
    fn time_minute_out_of_range() {
        expect_err("00:60:00");
        expect_err("00:99:00");
    }

    #[test]
    fn time_second_out_of_range() {
        expect_err("00:00:60");
        expect_err("00:00:99");
    }

    #[test]
    fn time_only_rejects_offset() {
        expect_err("07:32:00Z");
        expect_err("07:32:00+00:00");
        expect_err("07:32:00-05:00");
        expect_err("07:32Z");
        expect_err("07:32+01:00");
        expect_err("12:00:00.5Z");
        expect_err("12:00:00.5+00:00");
    }

    #[test]
    fn time_missing_colon() {
        expect_err("0732:00"); // no colon after HH
    }

    #[test]
    fn time_empty_frac() {
        expect_err("12:30:45."); // dot but no digits
    }

    // ── date-time combinations ──────────────────────────────────────

    #[test]
    fn datetime_t_separator() {
        roundtrip_lossy("2023-06-15T12:30", "2023-06-15T12:30:00");
        roundtrip("2023-06-15T12:30:45");
        roundtrip("2023-06-15T12:30:45.5");
    }

    #[test]
    fn datetime_space_separator() {
        roundtrip_lossy("2023-06-15 12:30", "2023-06-15T12:30:00");
        roundtrip_lossy("2023-06-15 12:30:45", "2023-06-15T12:30:45");
    }

    #[test]
    fn datetime_with_z_offset() {
        roundtrip_lossy("2023-06-15T12:30Z", "2023-06-15T12:30:00Z");
        roundtrip("2023-06-15T12:30:45Z");
        roundtrip("2023-06-15T12:30:45.123Z");
    }

    #[test]
    fn datetime_with_positive_offset() {
        roundtrip_lossy("2023-06-15T12:30+05:30", "2023-06-15T12:30:00+05:30");
        roundtrip("2023-06-15T12:30:45+23:59");
        roundtrip("2023-06-15T12:30:45.5+00:01");
    }

    #[test]
    fn datetime_with_negative_offset() {
        roundtrip_lossy("2023-06-15T12:30-05:00", "2023-06-15T12:30:00-05:00");
        roundtrip("2023-06-15T12:30:45-12:00");
        roundtrip("2023-06-15T12:30:45.5-00:01");
    }

    #[test]
    fn datetime_offset_hour_out_of_range() {
        expect_err("2023-06-15T12:30+24:00");
        expect_err("2023-06-15T12:30-99:00");
    }

    #[test]
    fn datetime_offset_minute_out_of_range() {
        expect_err("2023-06-15T12:30+00:60");
        expect_err("2023-06-15T12:30-01:99");
    }

    // ── fractional second edge cases ────────────────────────────────

    #[test]
    fn frac_all_zeros() {
        roundtrip("2023-01-01T00:00:00.000000000");
    }

    #[test]
    fn frac_all_nines() {
        roundtrip("2023-01-01T00:00:00.999999999");
    }

    #[test]
    fn frac_single_digit_values() {
        for d in 0..=9 {
            let s = format!("2023-01-01T00:00:00.{d}");
            roundtrip(&s);
        }
    }

    #[test]
    fn frac_beyond_9_digits_truncates() {
        // >9 frac digits: first 9 are kept, rest consumed but not stored
        let input = "2023-01-01T00:00:00.1234567891111";
        let (consumed, val) = parse_ok(input);
        assert_eq!(consumed, input.len());
        assert_eq!(val.nanos, 123456789);
        let mut buf = MaybeUninit::uninit();
        let output = val.format(&mut buf);
        assert_eq!(output, "2023-01-01T00:00:00.123456789");
    }

    #[test]
    fn frac_10_digits() {
        let input = "2023-01-01T00:00:00.0000000001";
        let (consumed, _) = parse_ok(input);
        assert_eq!(consumed, input.len());
    }

    // ── consumed byte count / trailing data ─────────────────────────

    #[test]
    fn trailing_data_after_date() {
        let (consumed, _) = parse_ok("2023-06-15hello");
        assert_eq!(consumed, 10);
    }

    #[test]
    fn trailing_data_after_time() {
        let (consumed, _) = parse_ok("12:30:45world");
        assert_eq!(consumed, 8);
    }

    #[test]
    fn trailing_data_after_datetime() {
        let (consumed, _) = parse_ok("2023-06-15T12:30stuff");
        assert_eq!(consumed, 16);
    }

    #[test]
    fn trailing_data_after_datetime_seconds() {
        let (consumed, _) = parse_ok("2023-06-15T12:30:45stuff");
        assert_eq!(consumed, 19);
    }

    #[test]
    fn trailing_data_after_frac() {
        let (consumed, _) = parse_ok("2023-06-15T12:30:45.123stuff");
        assert_eq!(consumed, 23);
    }

    #[test]
    fn trailing_data_after_z_offset() {
        let (consumed, _) = parse_ok("2023-06-15T12:30Zstuff");
        assert_eq!(consumed, 17);
    }

    #[test]
    fn trailing_data_after_numeric_offset() {
        let (consumed, _) = parse_ok("2023-06-15T12:30:45+05:30,next");
        assert_eq!(consumed, 25);
        let (consumed, _) = parse_ok("2023-06-15T12:30:45+05:30x");
        assert_eq!(consumed, 25);
        let (consumed, _) = parse_ok("2023-06-15T12:30:45+05:30 ");
        assert_eq!(consumed, 25);
    }

    #[test]
    fn trailing_data_after_time_no_seconds() {
        let (consumed, _) = parse_ok("23:59xyz");
        assert_eq!(consumed, 5);
    }

    // ── field accessor methods ──────────────────────────────────────

    #[test]
    fn date_accessor_present() {
        let (_, val) = parse_ok("2023-06-15");
        let d = val.date().unwrap();
        assert_eq!(d.year, 2023);
        assert_eq!(d.month, 6);
        assert_eq!(d.day, 15);
    }

    #[test]
    fn date_accessor_absent() {
        let (_, val) = parse_ok("12:30:00");
        assert!(val.date().is_none());
    }

    #[test]
    fn offset_accessor_present() {
        let (_, val) = parse_ok("2023-06-15T12:30Z");
        assert_eq!(val.offset(), Some(TimeOffset::Z));

        let (_, val) = parse_ok("2023-06-15T12:30+05:30");
        assert_eq!(val.offset(), Some(TimeOffset::Custom { minutes: 330 }));

        let (_, val) = parse_ok("2023-06-15T12:30-01:15");
        assert_eq!(val.offset(), Some(TimeOffset::Custom { minutes: -75 }));
    }

    #[test]
    fn offset_accessor_absent() {
        let (_, val) = parse_ok("2023-06-15T12:30:00");
        assert!(val.offset().is_none());

        let (_, val) = parse_ok("12:30:00");
        assert!(val.offset().is_none());
    }

    // ── invalid structures ──────────────────────────────────────────

    #[test]
    fn failures() {
        let failures = &[("07:32:00Z")];
        for input in failures {
            assert!(Datetime::munch(input.as_bytes()).is_none(),)
        }
    }

    #[test]
    fn garbage_input() {
        expect_err("hello");
        expect_err("ABCDE");
        expect_err("--:--");
    }

    #[test]
    fn truncated_date() {
        expect_err("2023-"); // month missing
        expect_err("2023-06"); // no second dash
        expect_err("2023-06-"); // day missing
    }

    #[test]
    fn truncated_time_after_date() {
        expect_err("2023-06-15T"); // T then sentinel 0 in Hour state
        expect_err("2023-06-15T1"); // 1 digit hour
        expect_err("2023-06-15T12"); // no colon
        expect_err("2023-06-15T12:"); // no minute digits
        expect_err("2023-06-15T12:3"); // 1 digit minute
    }

    #[test]
    fn truncated_seconds() {
        expect_err("2023-06-15T12:30:"); // colon but no digits
        expect_err("2023-06-15T12:30:4"); // 1 digit second
    }

    #[test]
    fn truncated_offset() {
        expect_err("2023-06-15T12:30+"); // sign but no digits
        expect_err("2023-06-15T12:30+0");
        expect_err("2023-06-15T12:30+05"); // no colon
        expect_err("2023-06-15T12:30+05:"); // no minute digits
        expect_err("2023-06-15T12:30+05:3");
    }

    #[test]
    fn letters_in_digit_fields() {
        expect_err("XXXX-01-01");
        expect_err("2023-XX-01");
        expect_err("2023-01-XX");
        expect_err("XX:00:00");
    }

    // ── boundary values ─────────────────────────────────────────────

    #[test]
    fn min_max_hour_minute_second() {
        roundtrip("00:00:00");
        roundtrip("23:59:59");
        roundtrip("2023-01-01T00:00:00");
        roundtrip("2023-01-01T23:59:59");
    }

    #[test]
    fn year_zero() {
        roundtrip("0000-01-01");
        roundtrip("0000-01-01T00:00:00Z");
    }

    #[test]
    fn year_max() {
        roundtrip("9999-12-31");
        roundtrip("9999-12-31T23:59:59.999999999+23:59");
    }

    #[test]
    fn offset_boundaries() {
        roundtrip_lossy("2023-01-01T00:00+00:00", "2023-01-01T00:00:00Z"); // +00:00 normalizes to Z
        roundtrip_lossy("2023-01-01T00:00+23:59", "2023-01-01T00:00:00+23:59");
        roundtrip_lossy("2023-01-01T00:00-23:59", "2023-01-01T00:00:00-23:59");
        roundtrip_lossy("2023-01-01T00:00+00:01", "2023-01-01T00:00:00+00:01");
        roundtrip_lossy("2023-01-01T00:00-00:01", "2023-01-01T00:00:00-00:01");
    }

    // ── every month last-day (non-leap, leap) ───────────────────────

    #[test]
    fn last_day_of_every_month_non_leap() {
        let expected = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        for (m, &day) in expected.iter().enumerate() {
            let month = m + 1;
            roundtrip(&format!("2023-{month:02}-{day:02}"));
            // one past the last day should fail
            expect_err(&format!("2023-{month:02}-{:02}", day + 1));
        }
    }

    #[test]
    fn last_day_of_every_month_leap() {
        let expected = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        for (m, &day) in expected.iter().enumerate() {
            let month = m + 1;
            roundtrip(&format!("2024-{month:02}-{day:02}"));
            expect_err(&format!("2024-{month:02}-{:02}", day + 1));
        }
    }

    // ── randomized roundtrip ────────────────────────────────────────

    #[test]
    fn randomized_roundtrip_date_only() {
        let mut rng = oorandom::Rand32::new(1);
        for _ in 0..5000 {
            let year = (rng.rand_u32() % 10000) as u16;
            let month = (rng.rand_u32() % 12) as u8 + 1;
            let max_day = days_in_month(year, month);
            let day = (rng.rand_u32() % max_day as u32) as u8 + 1;
            let s = format!("{year:04}-{month:02}-{day:02}");
            roundtrip(&s);
        }
    }

    #[test]
    fn randomized_roundtrip_time_only() {
        let mut rng = oorandom::Rand32::new(2);
        for _ in 0..5000 {
            let hour = (rng.rand_u32() % 24) as u8;
            let minute = (rng.rand_u32() % 60) as u8;
            let has_seconds = rng.rand_u32() % 2 == 0;
            if has_seconds {
                let second = (rng.rand_u32() % 60) as u8;
                let nd = rng.rand_u32() % 10; // 0 = no frac, 1-9 = frac digits
                if nd == 0 {
                    roundtrip(&format!("{hour:02}:{minute:02}:{second:02}"));
                } else {
                    let max_val = 10u32.pow(nd);
                    let frac = rng.rand_u32() % max_val;
                    let s = format!(
                        "{hour:02}:{minute:02}:{second:02}.{frac:0>width$}",
                        width = nd as usize
                    );
                    roundtrip(&s);
                }
            } else {
                let input = format!("{hour:02}:{minute:02}");
                let expected = format!("{hour:02}:{minute:02}:00");
                roundtrip_lossy(&input, &expected);
            }
        }
    }

    #[test]
    fn randomized_roundtrip_full_datetime() {
        let mut rng = oorandom::Rand32::new(3);
        for _ in 0..10000 {
            let year = (rng.rand_u32() % 10000) as u16;
            let month = (rng.rand_u32() % 12) as u8 + 1;
            let max_day = days_in_month(year, month);
            let day = (rng.rand_u32() % max_day as u32) as u8 + 1;
            let hour = (rng.rand_u32() % 24) as u8;
            let minute = (rng.rand_u32() % 60) as u8;

            let mut s = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}");
            let mut expected = s.clone();

            let has_seconds = rng.rand_u32() % 2 == 0;
            if has_seconds {
                let second = (rng.rand_u32() % 60) as u8;
                let sec_str = format!(":{second:02}");
                s += &sec_str;
                expected += &sec_str;
                let nd = rng.rand_u32() % 10;
                if nd > 0 {
                    let max_val = 10u32.pow(nd);
                    let frac = rng.rand_u32() % max_val;
                    let frac_str = format!(".{frac:0>width$}", width = nd as usize);
                    s += &frac_str;
                    expected += &frac_str;
                }
            } else {
                expected += ":00";
            }

            // Random offset: none, Z, or +/-HH:MM
            match rng.rand_u32() % 4 {
                0 => {} // no offset
                1 => {
                    s += "Z";
                    expected += "Z";
                }
                _ => {
                    let sign = if rng.rand_u32() % 2 == 0 { '+' } else { '-' };
                    let oh = (rng.rand_u32() % 24) as u8;
                    let om = (rng.rand_u32() % 60) as u8;
                    // +00:00 roundtrips as Z, so avoid that
                    if oh == 0 && om == 0 {
                        s += "Z";
                        expected += "Z";
                    } else {
                        let off_str = format!("{sign}{oh:02}:{om:02}");
                        s += &off_str;
                        expected += &off_str;
                    }
                }
            }

            roundtrip_lossy(&s, &expected);
        }
    }

    #[test]
    fn randomized_trailing_data() {
        // Trailing data is allowed after all datetime forms.
        let mut rng = oorandom::Rand32::new(4);
        // Date suffixes must not start with 'T'/'t' or ' ' (date-time separators).
        // Datetime suffixes must not start with offset chars (Z/z/+/-) or
        // digits that extend a field, or ':' / '.' which extend to seconds/frac.
        let date_suffixes = [",next", "\ttab", "\n", "xyz", ";end"];
        let time_suffixes = [",next", "\ttab", "\n", "xyz", ";end"];
        for _ in 0..5000 {
            let year = (rng.rand_u32() % 10000) as u16;
            let month = (rng.rand_u32() % 12) as u8 + 1;
            let max_day = days_in_month(year, month);
            let day = (rng.rand_u32() % max_day as u32) as u8 + 1;
            let hour = (rng.rand_u32() % 24) as u8;
            let minute = (rng.rand_u32() % 60) as u8;
            let second = (rng.rand_u32() % 60) as u8;

            // Date-only with trailing data
            let base = format!("{year:04}-{month:02}-{day:02}");
            let base_len = base.len();
            let suffix = date_suffixes[rng.rand_u32() as usize % date_suffixes.len()];
            let full = format!("{base}{suffix}");
            let (consumed, _) = parse_ok(&full);
            assert_eq!(consumed, base_len, "wrong consumed for {full:?}");

            // Datetime (no offset) with trailing data
            let base = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
            let base_len = base.len();
            let suffix = time_suffixes[rng.rand_u32() as usize % time_suffixes.len()];
            let full = format!("{base}{suffix}");
            let (consumed, _) = parse_ok(&full);
            assert_eq!(consumed, base_len, "wrong consumed for {full:?}");
        }
    }

    // ── randomized invalid inputs (fuzz-lite) ───────────────────────

    #[test]
    fn randomized_reject_invalid() {
        let mut rng = oorandom::Rand32::new(5);
        for _ in 0..10000 {
            // Generate a random byte sequence of length 5..30
            let len = 5 + (rng.rand_u32() % 26) as usize;
            let bytes: Vec<u8> = (0..len).map(|_| (rng.rand_u32() % 256) as u8).collect();
            // Most random byte sequences should fail; just ensure no panic
            let _ = Datetime::munch(&bytes);
        }
    }

    #[test]
    fn randomized_mutate_valid_input() {
        // Take a valid input, flip one byte, verify no panics
        let mut rng = oorandom::Rand32::new(6);
        let valid = b"2023-06-15T12:30:45.123+05:30";
        for _ in 0..5000 {
            let mut mutated = *valid;
            let pos = rng.rand_u32() as usize % mutated.len();
            mutated[pos] = (rng.rand_u32() % 256) as u8;
            let _ = Datetime::munch(&mutated);
        }
    }

    // ── leap year correctness ───────────────────────────────────────

    #[test]
    fn leap_year_known_values() {
        // Leap years
        for y in [0, 4, 400, 800, 1600, 2000, 2400, 2024, 1996] {
            assert!(is_leap_year(y), "{y} should be a leap year");
        }
        // Non-leap years
        for y in [1, 100, 200, 300, 500, 1900, 2100, 2023, 2025] {
            assert!(!is_leap_year(y), "{y} should not be a leap year");
        }
    }

    #[test]
    fn leap_year_exhaustive() {
        fn is_leap_naive(y: u16) -> bool {
            (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
        }
        for y in 0..=9999 {
            assert_eq!(
                is_leap_year(y),
                is_leap_naive(y),
                "is_leap_year disagreed for year {y}"
            );
        }
    }

    // ── nanos / frac digit preservation ─────────────────────────────

    #[test]
    fn frac_preserves_leading_zeros() {
        roundtrip("2023-01-01T00:00:00.001");
        roundtrip("2023-01-01T00:00:00.000001");
        roundtrip("2023-01-01T00:00:00.000000001");
        roundtrip("2023-01-01T00:00:00.100000000");
        roundtrip("2023-01-01T00:00:00.010000000");
    }

    #[test]
    fn frac_trailing_zeros_preserved() {
        // "0.10" and "0.1" are distinct: nd=2 vs nd=1
        roundtrip("2023-01-01T00:00:00.10");
        roundtrip("2023-01-01T00:00:00.1");
        let (_, v1) = parse_ok("2023-01-01T00:00:00.10");
        let (_, v2) = parse_ok("2023-01-01T00:00:00.1");
        assert_eq!(v1.nanos, v2.nanos); // same nanos value
        let mut b1 = MaybeUninit::uninit();
        let mut b2 = MaybeUninit::uninit();
        // but formatted differently
        assert_ne!(v1.format(&mut b1), v2.format(&mut b2));
    }
}
