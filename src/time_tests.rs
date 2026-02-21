use std::mem::MaybeUninit;

use super::*;

fn roundtrip(input: &str) {
    let (amount, result) = DateTime::munch(input.as_bytes()).unwrap();
    assert_eq!(amount, input.len(), "consumed wrong amount for {input:?}");
    let mut buf = MaybeUninit::uninit();
    let output = result.format(&mut buf);
    assert_eq!(input, output, "roundtrip mismatch for {input:?}");
}

fn roundtrip_lossy(input: &str, expected: &str) {
    let (amount, result) = DateTime::munch(input.as_bytes()).unwrap();
    assert_eq!(amount, input.len(), "consumed wrong amount for {input:?}");
    let mut buf = MaybeUninit::uninit();
    let output = result.format(&mut buf);
    assert_eq!(expected, output, "roundtrip mismatch for {input:?}");
}

#[track_caller]
fn expect_err(input: &str) {
    let result = DateTime::munch(input.as_bytes());
    assert!(result.is_none(), "expected error for {input:?}");
}

fn parse_ok(input: &str) -> (usize, DateTime) {
    DateTime::munch(input.as_bytes()).unwrap()
}

#[test]
fn roundtrips() {
    // Exact roundtrips: output == input
    let exact = &[
        // full datetimes with offsets
        "1979-05-27T07:32:00Z",
        "1979-05-27T07:32:00+00:00",
        "1979-05-27T00:32:00-23:00",
        "2000-12-17T00:32:00.5-07:00",
        "1979-05-27T00:32:00.999999+21:20",
        // local datetimes
        "1979-05-27T07:32:00",
        "1979-05-27T07:32:00.5",
        "1979-05-27T07:32:00.999999999",
        "1979-05-27T07:32:00.123456789",
        "2023-06-15T12:30:45",
        "2023-06-15T12:30:45.5",
        "2023-06-15T12:30:45Z",
        "2023-06-15T12:30:45.123Z",
        "2023-06-15T12:30:45+23:59",
        "2023-06-15T12:30:45.5+00:01",
        "2023-06-15T12:30:45-12:00",
        "2023-06-15T12:30:45.5-00:01",
        "2023-01-01T00:00:00",
        "2023-01-01T23:59:59",
        // date only
        "1979-05-27",
        "2000-01-01",
        "9999-12-31",
        "0000-01-01",
        "0001-06-15",
        "2023-06-15",
        // time only
        "07:32:00",
        "00:32:00.5",
        "00:32:00.999999",
        "00:00:00",
        "23:59:59",
        "12:30:45",
        "12:30:45.1",
        "12:30:45.12",
        "12:30:45.123",
        "12:30:45.1234",
        "12:30:45.12345",
        "12:30:45.123456",
        "12:30:45.1234567",
        "12:30:45.12345678",
        "12:30:45.123456789",
        // boundary: year zero
        "0000-01-01T00:00:00Z",
        // boundary: year max
        "9999-12-31T23:59:59.999999999+23:59",
        // frac leading zeros preserved
        "2023-01-01T00:00:00.001",
        "2023-01-01T00:00:00.000001",
        "2023-01-01T00:00:00.000000001",
        "2023-01-01T00:00:00.100000000",
        "2023-01-01T00:00:00.010000000",
        // frac extremes
        "2023-01-01T00:00:00.000000000",
        "2023-01-01T00:00:00.999999999",
        "2023-01-01T00:00:00.10",
        "2023-01-01T00:00:00.1",
    ];
    for input in exact {
        roundtrip(input);
    }

    // Lossy roundtrips: input normalizes to expected
    let lossy: &[(&str, &str)] = &[
        // space separator -> T
        ("1979-05-27 07:32:00Z", "1979-05-27T07:32:00Z"),
        ("2000-01-01 00:00:00", "2000-01-01T00:00:00"),
        ("1999-12-31 23:59:59.9", "1999-12-31T23:59:59.9"),
        ("2024-02-29 12:00+05:30", "2024-02-29T12:00:00+05:30"),
        ("2023-06-15 12:30", "2023-06-15T12:30:00"),
        ("2023-06-15 12:30:45", "2023-06-15T12:30:45"),
        // no-seconds -> :00
        ("1979-05-27T07:32Z", "1979-05-27T07:32:00Z"),
        ("1979-05-27T07:32-07:00", "1979-05-27T07:32:00-07:00"),
        ("9999-12-29T07:32", "9999-12-29T07:32:00"),
        ("2023-06-15T12:30", "2023-06-15T12:30:00"),
        ("2023-06-15T12:30Z", "2023-06-15T12:30:00Z"),
        ("2023-06-15T12:30+05:30", "2023-06-15T12:30:00+05:30"),
        ("2023-06-15T12:30-05:00", "2023-06-15T12:30:00-05:00"),
        ("00:00", "00:00:00"),
        ("23:59", "23:59:00"),
        ("12:30", "12:30:00"),
        // lowercase t/z
        ("1987-07-05t17:45:00z", "1987-07-05T17:45:00Z"),
        ("1987-07-05t17:45:00", "1987-07-05T17:45:00"),
        // offset boundaries
        ("2023-01-01T00:00+23:59", "2023-01-01T00:00:00+23:59"),
        ("2023-01-01T00:00-23:59", "2023-01-01T00:00:00-23:59"),
        ("2023-01-01T00:00+00:01", "2023-01-01T00:00:00+00:01"),
        ("2023-01-01T00:00-00:01", "2023-01-01T00:00:00-00:01"),
    ];
    for (input, expected) in lossy {
        roundtrip_lossy(input, expected);
    }

    // frac single digit values 0..=9
    for d in 0..=9 {
        roundtrip(&format!("2023-01-01T00:00:00.{d}"));
    }
}

#[test]
fn rejects_invalid() {
    let cases: &[&str] = &[
        // EOF / too-short
        "",
        "1",
        "12",
        "12:",
        "1979",
        // garbage
        "hello",
        "ABCDE",
        "--:--",
        // date: month out of range
        "2023-00-01",
        "2023-13-01",
        // date: day out of range
        "2023-01-00",
        "2023-01-32",
        "2023-04-31",
        "2023-06-31",
        "2023-02-30",
        // date: invalid separators
        "2023/01/01",
        "2023-01/01",
        "20230101",
        // date: wrong digit counts
        "202-01-01",
        "2023-1-01",
        "2023-01-1",
        // date: non-leap feb 29
        "2023-02-29",
        "1900-02-29",
        "2100-02-29",
        // time: hour out of range
        "24:00:00",
        "99:00:00",
        // time: minute out of range
        "00:60:00",
        "00:99:00",
        // time: second out of range
        "00:00:61",
        "00:00:99",
        // time: rejects offset on time-only
        "07:32:00Z",
        "07:32:00+00:00",
        "07:32:00-05:00",
        "07:32Z",
        "07:32+01:00",
        "12:00:00.5Z",
        "12:00:00.5+00:00",
        // time: missing colon
        "0732:00",
        // time: empty frac
        "12:30:45.",
        // letters in digit fields
        "XXXX-01-01",
        "2023-XX-01",
        "2023-01-XX",
        "XX:00:00",
        // truncated date
        "2023-",
        "2023-06",
        "2023-06-",
        // truncated time after date
        "2023-06-15T",
        "2023-06-15T1",
        "2023-06-15T12",
        "2023-06-15T12:",
        "2023-06-15T12:3",
        // truncated seconds
        "2023-06-15T12:30:",
        "2023-06-15T12:30:4",
        // truncated offset
        "2023-06-15T12:30+",
        "2023-06-15T12:30+0",
        "2023-06-15T12:30+05",
        "2023-06-15T12:30+05:",
        "2023-06-15T12:30+05:3",
        // offset hour out of range
        "2023-06-15T12:30+24:00",
        "2023-06-15T12:30-99:00",
        // offset minute out of range
        "2023-06-15T12:30+00:60",
        "2023-06-15T12:30-01:99",
    ];
    for input in cases {
        expect_err(input);
    }
}

#[test]
fn trailing_data() {
    let cases: &[(&str, usize)] = &[
        ("2023-06-15hello", 10),
        ("12:30:45world", 8),
        ("2023-06-15T12:30stuff", 16),
        ("2023-06-15T12:30:45stuff", 19),
        ("2023-06-15T12:30:45.123stuff", 23),
        ("2023-06-15T12:30Zstuff", 17),
        ("2023-06-15T12:30:45+05:30,next", 25),
        ("2023-06-15T12:30:45+05:30x", 25),
        ("2023-06-15T12:30:45+05:30 ", 25),
        ("23:59xyz", 5),
    ];
    for (input, expected_consumed) in cases {
        let (consumed, _) = parse_ok(input);
        assert_eq!(consumed, *expected_consumed, "wrong consumed for {input:?}");
    }
}

#[test]
fn field_accessors() {
    // date accessor present
    let (_, val) = parse_ok("2023-06-15");
    let d = val.date().unwrap();
    assert_eq!((d.year, d.month, d.day), (2023, 6, 15));

    // date accessor absent on time-only
    let (_, val) = parse_ok("12:30:00");
    assert!(val.date().is_none(), "time-only should have no date");

    // offset accessor: Z
    let (_, val) = parse_ok("2023-06-15T12:30Z");
    assert_eq!(val.offset(), Some(TimeOffset::Z), "expected Z offset");

    // offset accessor: positive
    let (_, val) = parse_ok("2023-06-15T12:30+05:30");
    assert_eq!(
        val.offset(),
        Some(TimeOffset::Custom { minutes: 330 }),
        "expected +05:30"
    );

    // offset accessor: negative
    let (_, val) = parse_ok("2023-06-15T12:30-01:15");
    assert_eq!(
        val.offset(),
        Some(TimeOffset::Custom { minutes: -75 }),
        "expected -01:15"
    );

    // offset accessor absent on local datetime / time-only
    for input in ["2023-06-15T12:30:00", "12:30:00"] {
        let (_, val) = parse_ok(input);
        assert!(val.offset().is_none(), "expected no offset for {input:?}");
    }
}

#[test]
fn frac_edge_cases() {
    // >9 frac digits: first 9 kept, rest consumed but not stored
    let input = "2023-01-01T00:00:00.1234567891111";
    let (consumed, val) = parse_ok(input);
    assert_eq!(consumed, input.len(), "should consume all for {input:?}");
    assert_eq!(val.nanos, 123456789, "nanos mismatch for {input:?}");
    let mut buf = MaybeUninit::uninit();
    let output = val.format(&mut buf);
    assert_eq!(output, "2023-01-01T00:00:00.123456789");

    // 10 digits consumed fully
    let input = "2023-01-01T00:00:00.0000000001";
    let (consumed, _) = parse_ok(input);
    assert_eq!(consumed, input.len(), "should consume all for {input:?}");

    // "0.10" vs "0.1": same nanos, different formatting
    let (_, v1) = parse_ok("2023-01-01T00:00:00.10");
    let (_, v2) = parse_ok("2023-01-01T00:00:00.1");
    assert_eq!(v1.nanos, v2.nanos, "nanos should match");
    let mut b1 = MaybeUninit::uninit();
    let mut b2 = MaybeUninit::uninit();
    assert_ne!(
        v1.format(&mut b1),
        v2.format(&mut b2),
        ".10 and .1 should format differently"
    );
}

#[test]
fn last_day_of_every_month() {
    // Non-leap year (2023)
    let non_leap = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for (m, &day) in non_leap.iter().enumerate() {
        let month = m + 1;
        roundtrip(&format!("2023-{month:02}-{day:02}"));
        roundtrip(&format!("2023-{month:02}-01"));
        expect_err(&format!("2023-{month:02}-{:02}", day + 1));
    }

    // Leap year (2024)
    let leap = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for (m, &day) in leap.iter().enumerate() {
        let month = m + 1;
        roundtrip(&format!("2024-{month:02}-{day:02}"));
        expect_err(&format!("2024-{month:02}-{:02}", day + 1));
    }

    // Known leap years
    for date in ["2000-02-29", "2024-02-29", "1600-02-29", "0004-02-29"] {
        roundtrip(date);
    }
}

#[test]
fn leap_year_known_values() {
    for y in [0, 4, 400, 800, 1600, 2000, 2400, 2024, 1996] {
        assert!(is_leap_year(y), "{y} should be a leap year");
    }
    for y in [1, 100, 200, 300, 500, 1900, 2100, 2023, 2025] {
        assert!(!is_leap_year(y), "{y} should not be a leap year");
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn leap_year_exhaustive() {
    fn is_leap_naive(y: u16) -> bool {
        (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
    }
    for y in 0..=9999 {
        assert_eq!(
            is_leap_year(y),
            is_leap_naive(y),
            "is_leap_year disagreed for year {y}"
        );
    }
}

#[test]
fn randomized_roundtrip_date_only() {
    let mut rng = oorandom::Rand32::new(1);
    let iterations = if cfg!(miri) { 50 } else { 5000 };
    for _ in 0..iterations {
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
    let iterations = if cfg!(miri) { 50 } else { 5000 };
    for _ in 0..iterations {
        let hour = (rng.rand_u32() % 24) as u8;
        let minute = (rng.rand_u32() % 60) as u8;
        let has_seconds = rng.rand_u32().is_multiple_of(2);
        if has_seconds {
            let second = (rng.rand_u32() % 60) as u8;
            let digit_count = rng.rand_u32() % 10; // 0 = no frac, 1-9 = frac digits
            if digit_count == 0 {
                roundtrip(&format!("{hour:02}:{minute:02}:{second:02}"));
            } else {
                let max_val = 10u32.pow(digit_count);
                let frac = rng.rand_u32() % max_val;
                let s = format!(
                    "{hour:02}:{minute:02}:{second:02}.{frac:0>width$}",
                    width = digit_count as usize
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
    let iterations = if cfg!(miri) { 50 } else { 10000 };
    for _ in 0..iterations {
        let year = (rng.rand_u32() % 10000) as u16;
        let month = (rng.rand_u32() % 12) as u8 + 1;
        let max_day = days_in_month(year, month);
        let day = (rng.rand_u32() % max_day as u32) as u8 + 1;
        let hour = (rng.rand_u32() % 24) as u8;
        let minute = (rng.rand_u32() % 60) as u8;

        let mut s = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}");
        let mut expected = s.clone();

        let has_seconds = rng.rand_u32().is_multiple_of(2);
        if has_seconds {
            let second = (rng.rand_u32() % 60) as u8;
            let sec_str = format!(":{second:02}");
            s += &sec_str;
            expected += &sec_str;
            let digit_count = rng.rand_u32() % 10;
            if digit_count > 0 {
                let max_val = 10u32.pow(digit_count);
                let frac = rng.rand_u32() % max_val;
                let frac_str = format!(".{frac:0>width$}", width = digit_count as usize);
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
                let sign = if rng.rand_u32().is_multiple_of(2) { '+' } else { '-' };
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
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let date_suffixes = [",next", "\ttab", "\n", "xyz", ";end"];
    let time_suffixes = [",next", "\ttab", "\n", "xyz", ";end"];
    let iterations = if cfg!(miri) { 50 } else { 1000 };
    for _ in 0..iterations {
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

#[test]
fn randomized_reject_invalid() {
    let mut rng = oorandom::Rand32::new(0x1234beaf);
    let iterations = if cfg!(miri) { 50 } else { 1000 };
    for _ in 0..iterations {
        let len = 5 + (rng.rand_u32() % 26) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (rng.rand_u32() % 256) as u8).collect();
        let _ = DateTime::munch(&bytes);
    }
}

#[test]
fn randomized_mutate_valid_input() {
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let valid = b"2023-06-15T12:30:45.123+05:30";
    let iterations = if cfg!(miri) { 50 } else { 5000 };
    for _ in 0..iterations {
        let mut mutated = *valid;
        let pos = rng.rand_u32() as usize % mutated.len();
        mutated[pos] = (rng.rand_u32() % 256) as u8;
        let _ = DateTime::munch(&mutated);
    }
}
