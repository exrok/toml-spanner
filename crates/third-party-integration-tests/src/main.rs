// Kept in sync with the jiff integration example under `<details>` in
// `src/time.rs`. Edits here should be mirrored there.

use toml_spanner::{
    Arena, Date, DateTime, Error as TomlError, FromToml, Item, Key, Span as TomlSpan, Table, Time,
    TimeOffset, ToToml, ToTomlError,
};

fn extract_date(
    datetime: &toml_spanner::DateTime,
    span: TomlSpan,
) -> Result<jiff::civil::Date, TomlError> {
    let Some(date) = datetime.date() else {
        return Err(TomlError::custom("Missing date component", span));
    };
    // toml_spanner guarantees the following inclusive ranges
    // year: 0-9999, month: 1-12, day: 1-31
    // making the as casts safe.
    match jiff::civil::Date::new(date.year as i16, date.month as i8, date.day as i8) {
        Ok(value) => Ok(value),
        Err(err) => Err(TomlError::custom(format!("Invalid date: {err}"), span)),
    }
}

fn extract_time(
    datetime: &toml_spanner::DateTime,
    span: TomlSpan,
) -> Result<jiff::civil::Time, TomlError> {
    let Some(time) = datetime.time() else {
        return Err(TomlError::custom("Missing time component", span));
    };
    // toml_spanner guarantees the following inclusive ranges
    // hour: 0-23, minute: 0-59, second: 0-60, nanosecond: 0-999999999
    // making the as casts safe.
    match jiff::civil::Time::new(
        time.hour as i8,
        time.minute as i8,
        time.second as i8,
        time.nanosecond as i32,
    ) {
        Ok(value) => Ok(value),
        Err(err) => Err(TomlError::custom(format!("Invalid time: {err}"), span)),
    }
}

fn extract_timezone(
    datetime: &toml_spanner::DateTime,
    span: TomlSpan,
) -> Result<jiff::tz::TimeZone, TomlError> {
    let Some(offset) = datetime.offset() else {
        return Err(TomlError::custom("Missing offset component", span));
    };
    match offset {
        toml_spanner::TimeOffset::Z => Ok(jiff::tz::TimeZone::UTC),
        toml_spanner::TimeOffset::Custom { minutes } => {
            match jiff::tz::Offset::from_seconds(minutes as i32 * 60) {
                Ok(jiff_offset) => Ok(jiff::tz::TimeZone::fixed(jiff_offset)),
                Err(err) => Err(TomlError::custom(format!("Invalid offset: {err}"), span)),
            }
        }
    }
}

fn to_jiff_date(item: &toml_spanner::Item<'_>) -> Result<jiff::civil::Date, TomlError> {
    let Some(datetime) = item.as_datetime() else {
        return Err(item.expected(&"date"));
    };

    if datetime.time().is_some() {
        return Err(TomlError::custom(
            "Expected lone date but found time",
            item.span(),
        ));
    };

    extract_date(datetime, item.span())
}

fn to_jiff_datetime(item: &toml_spanner::Item<'_>) -> Result<jiff::civil::DateTime, TomlError> {
    let Some(datetime) = item.as_datetime() else {
        return Err(item.expected(&"civil datetime"));
    };

    if datetime.offset().is_some() {
        return Err(TomlError::custom(
            "Expected naive timestamp but found offset",
            item.span(),
        ));
    };

    Ok(jiff::civil::DateTime::from_parts(
        extract_date(datetime, item.span())?,
        extract_time(datetime, item.span())?,
    ))
}

fn to_jiff_timestamp(item: &toml_spanner::Item<'_>) -> Result<jiff::Timestamp, TomlError> {
    let Some(datetime) = item.as_datetime() else {
        return Err(item.expected(&"timestamp"));
    };
    let civil = jiff::civil::DateTime::from_parts(
        extract_date(datetime, item.span())?,
        extract_time(datetime, item.span())?,
    );
    let timezone = extract_timezone(datetime, item.span())?;
    match timezone.to_timestamp(civil) {
        Ok(value) => Ok(value),
        Err(err) => Err(TomlError::custom(
            format!("Invalid timestamp: {err}"),
            item.span(),
        )),
    }
}

fn from_jiff_date(date: jiff::civil::Date) -> Result<Date, ToTomlError> {
    let year = date.year();
    if year < 0 {
        return Err(ToTomlError::from("year out of TOML range (0..=9999)"));
    }
    Date::new(year as u16, date.month() as u8, date.day() as u8)
        .ok_or_else(|| ToTomlError::from("date out of TOML range"))
}

fn from_jiff_time(time: jiff::civil::Time) -> Result<Time, ToTomlError> {
    Time::new(
        time.hour() as u8,
        time.minute() as u8,
        time.second() as u8,
        time.subsec_nanosecond() as u32,
    )
    .ok_or_else(|| ToTomlError::from("time out of TOML range"))
}

fn from_jiff_civil_datetime(dt: jiff::civil::DateTime) -> Result<DateTime, ToTomlError> {
    Ok(DateTime::local_datetime(
        from_jiff_date(dt.date())?,
        from_jiff_time(dt.time())?,
    ))
}

fn from_jiff_timestamp(ts: jiff::Timestamp) -> Result<DateTime, ToTomlError> {
    let civil = ts.to_zoned(jiff::tz::TimeZone::UTC).datetime();
    Ok(DateTime::offset_datetime(
        from_jiff_date(civil.date())?,
        from_jiff_time(civil.time())?,
        TimeOffset::Z,
    )
    .expect("TimeOffset::Z is always valid"))
}

#[derive(Debug, PartialEq)]
pub struct TimeConfig {
    pub date: jiff::civil::Date,
    pub datetime: jiff::civil::DateTime,
    pub timestamp: jiff::Timestamp,
}

impl<'de> FromToml<'de> for TimeConfig {
    fn from_toml(
        ctx: &mut toml_spanner::Context<'de>,
        value: &toml_spanner::Item<'de>,
    ) -> Result<Self, toml_spanner::Failed> {
        let mut th = value.table_helper(ctx)?;
        let config = TimeConfig {
            date: th.required_mapped("date", to_jiff_date)?,
            datetime: th.required_mapped("datetime", to_jiff_datetime)?,
            timestamp: th.required_mapped("timestamp", to_jiff_timestamp)?,
        };
        Ok(config)
    }
}

impl ToToml for TimeConfig {
    fn to_toml<'a>(&'a self, arena: &'a Arena) -> Result<Item<'a>, ToTomlError> {
        let Some(mut table) = Table::try_with_capacity(3, arena) else {
            return Err(ToTomlError::from("table capacity exceeded"));
        };
        table.insert_unique(
            Key::new("date"),
            Item::from(DateTime::local_date(from_jiff_date(self.date)?)),
            arena,
        );
        table.insert_unique(
            Key::new("datetime"),
            Item::from(from_jiff_civil_datetime(self.datetime)?),
            arena,
        );
        table.insert_unique(
            Key::new("timestamp"),
            Item::from(from_jiff_timestamp(self.timestamp)?),
            arena,
        );
        Ok(table.into_item())
    }
}

fn main() {
    let arena = toml_spanner::Arena::new();

    let toml_doc = r#"
        date = 1997-02-28
        datetime = 2066-01-30T14:45:00
        timestamp = 3291-12-01T00:45:00Z
    "#;
    let mut doc = toml_spanner::parse(toml_doc, &arena).unwrap();
    let config: TimeConfig = doc.to().unwrap();
    println!("parsed = {config:#?}");

    let emitted = toml_spanner::to_string(&config).unwrap();
    println!("\nemitted =\n{emitted}");

    let round_trip_arena = toml_spanner::Arena::new();
    let mut round_trip_doc = toml_spanner::parse(&emitted, &round_trip_arena).unwrap();
    let round_trip: TimeConfig = round_trip_doc.to().unwrap();
    assert_eq!(config, round_trip, "round-trip through ToToml must match");
    println!("\nround-trip ok");
}
