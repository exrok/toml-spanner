use crate::{
    Deserialize, Error, ErrorKind,
    str::Str,
    value::{self, Item},
};

impl<'de> Deserialize<'de> for String {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        value.take_string(None).map(String::from)
    }
}

impl<'de> Deserialize<'de> for Str<'de> {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        value.take_string(None)
    }
}

impl<'de> Deserialize<'de> for std::borrow::Cow<'de, str> {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        value.take_string(None).map(std::borrow::Cow::from)
    }
}

impl<'de> Deserialize<'de> for bool {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        match value.as_bool() {
            Some(b) => Ok(b),
            None => Err(value.expected("a bool")),
        }
    }
}

fn deser_integer(
    value: &mut Item<'_>,
    min: i64,
    max: i64,
    name: &'static str,
) -> Result<i64, Error> {
    let span = value.span();
    match value.as_integer() {
        Some(i) if i >= min && i <= max => Ok(i),
        Some(_) => Err(Error {
            kind: ErrorKind::OutOfRange(name),
            span,
        }),
        None => Err(value.expected("an integer")),
    }
}

macro_rules! integer {
    ($($num:ty),+) => {$(
        impl<'de> Deserialize<'de> for $num {
            fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
                match deser_integer(value, <$num>::MIN as i64, <$num>::MAX as i64, stringify!($num)) {
                    Ok(i) => Ok(i as $num),
                    Err(e) => Err(e),
                }
            }
        }
    )+};
}

integer!(i8, i16, i32, isize, u8, u16, u32);

impl<'de> Deserialize<'de> for i64 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        deser_integer(value, i64::MIN, i64::MAX, "i64")
    }
}

impl<'de> Deserialize<'de> for u64 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        match deser_integer(value, 0, i64::MAX, "u64") {
            Ok(i) => Ok(i as u64),
            Err(e) => Err(e),
        }
    }
}

impl<'de> Deserialize<'de> for usize {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        const MAX: i64 = if usize::BITS < 64 {
            usize::MAX as i64
        } else {
            i64::MAX
        };
        match deser_integer(value, 0, MAX, "usize") {
            Ok(i) => Ok(i as usize),
            Err(e) => Err(e),
        }
    }
}

impl<'de> Deserialize<'de> for f32 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        match value.as_float() {
            Some(f) => Ok(f as f32),
            None => Err(value.expected("a float")),
        }
    }
}

impl<'de> Deserialize<'de> for f64 {
    fn deserialize(value: &mut Item<'de>) -> Result<Self, Error> {
        match value.as_float() {
            Some(f) => Ok(f),
            None => Err(value.expected("a float")),
        }
    }
}

impl<'de, T> Deserialize<'de> for Vec<T>
where
    T: Deserialize<'de>,
{
    fn deserialize(value: &mut value::Item<'de>) -> Result<Self, Error> {
        let value::ValueMut::Array(arr) = value.value_mut() else {
            return Err(value.expected("an array"));
        };
        let arr = std::mem::take(arr);

        let mut s = Vec::new();
        for mut v in arr {
            s.push(T::deserialize(&mut v)?);
        }

        Ok(s)
    }
}
