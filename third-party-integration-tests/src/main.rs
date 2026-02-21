use jiff::civil::Date;
use toml_spanner::{Deserialize, Error as TomlError, Span as TomlSpan};

fn extract_date(
    datetime: &toml_spanner::DateTime,
    span: TomlSpan,
) -> Result<jiff::civil::Date, TomlError> {
    let Some(date) = datetime.date() else {
        return Err(TomlError::custom("Missing date component", span));
    };
    // toml_spanner guartees the following inclusive ranges
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
    // toml_spanner guartees the following inclusive ranges
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
        return Err(item.expected("date"));
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
        return Err(item.expected("civil datetime"));
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
        return Err(item.expected("timestamp"));
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

#[derive(Debug)]
pub struct TimeConfig {
    pub date: Date,
    pub datetime: jiff::civil::DateTime,
    pub timestamp: jiff::Timestamp,
}

impl<'de> Deserialize<'de> for TimeConfig {
    fn deserialize(
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

fn main() {
    let arena = toml_spanner::Arena::new();

    let toml_doc = r#"
        date = 1997-02-28
        datetime = 2066-01-30T14:45:00
        timestamp = 3291-12-01T00:45:00Z
    "#;
    let mut root = toml_spanner::parse(toml_doc, &arena).unwrap();
    let config: TimeConfig = root.deserialize().unwrap();
    println!("{:#?}", config);
}
