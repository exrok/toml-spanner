use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    rc::Rc,
    sync::Arc,
};

use crate::{Arena, Array, Failed, Item, Key, Table, item::Value};

/// I don't like this name, but as is it's easily grepable
pub struct ToContext<'a> {
    pub arena: &'a Arena,
    pub(crate) error: Option<Cow<'static, str>>,
}
impl<'a> ToContext<'a> {
    pub fn new(arena: &'a Arena) -> ToContext<'a> {
        ToContext { arena, error: None }
    }
    #[cold]
    pub fn report_error(&mut self, message: &'static str) -> Result<Item<'a>, Failed> {
        self.error = Some(Cow::Borrowed(message));
        Err(Failed)
    }
}

/// extracted out to avoid code bloat
fn optional_to_required<'a>(
    optional: Result<Option<Item<'a>>, Failed>,
    _ctx: &mut ToContext<'a>,
) -> Result<Item<'a>, Failed> {
    match optional {
        Ok(Some(item)) => Ok(item),
        Ok(None) => Err(Failed), // Todo add message
        Err(_) => Err(Failed),
    }
}

fn required_to_optional<'a>(
    required: Result<Item<'a>, Failed>,
) -> Result<Option<Item<'a>>, Failed> {
    if let Ok(item) = required {
        Ok(Some(item))
    } else {
        Err(Failed)
    }
}

pub trait ToToml {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        optional_to_required(self.to_optional_toml(ctx), ctx)
    }
    fn to_optional_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Option<Item<'a>>, Failed> {
        required_to_optional(self.to_toml(ctx))
    }
}

impl<K: ToToml> ToToml for BTreeSet<K> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut array) = Array::try_with_capacity(self.len(), ctx.arena) else {
            return ctx.report_error("Length of array exceeded maximum capacity");
        };
        for item in self {
            array.push(
                match item.to_toml(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(array.into_item())
    }
}

impl<K: ToToml, H> ToToml for std::collections::HashSet<K, H> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut array) = Array::try_with_capacity(self.len(), ctx.arena) else {
            return ctx.report_error("Length of array exceeded maximum capacity");
        };
        for item in self {
            array.push(
                match item.to_toml(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(array.into_item())
    }
}

impl<const N: usize, T: ToToml> ToToml for [T; N] {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        self.as_slice().to_toml(ctx)
    }
}

impl<T: ToToml> ToToml for Option<T> {
    fn to_optional_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Option<Item<'a>>, Failed> {
        match self {
            Some(value) => value.to_optional_toml(ctx),
            None => Ok(None),
        }
    }
}

impl ToToml for str {
    fn to_toml<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::string(self))
    }
}

impl ToToml for String {
    fn to_toml<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::string(self))
    }
}

impl<T: ToToml> ToToml for Box<T> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToToml>::to_toml(&*self, ctx)
    }
}

impl<T: ToToml> ToToml for [T] {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut array) = Array::try_with_capacity(self.len(), ctx.arena) else {
            return ctx.report_error("Length of array exceeded maximum capacity");
        };
        for item in self {
            array.push(
                match item.to_toml(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(array.into_item())
    }
}

impl<T: ToToml> ToToml for Vec<T> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        self.as_slice().to_toml(ctx)
    }
}

impl ToToml for f32 {
    fn to_toml<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::from(*self as f64))
    }
}

impl ToToml for f64 {
    fn to_toml<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::from(*self))
    }
}

impl ToToml for bool {
    fn to_toml<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::from(*self))
    }
}

impl<T: ToToml + ?Sized> ToToml for &T {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToToml>::to_toml(self, ctx)
    }
}

impl<T: ToToml + ?Sized> ToToml for &mut T {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToToml>::to_toml(self, ctx)
    }
}

impl<T: ToToml> ToToml for Rc<T> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToToml>::to_toml(self, ctx)
    }
}

impl<T: ToToml> ToToml for Arc<T> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToToml>::to_toml(self, ctx)
    }
}

impl<'b, T: ToToml + Clone> ToToml for Cow<'b, T> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToToml>::to_toml(self, ctx)
    }
}

impl ToToml for char {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let mut buf = [0; 4];
        Ok(Item::string(
            ctx.arena.alloc_str(self.encode_utf8(&mut buf)),
        ))
    }
}

impl ToToml for std::path::Path {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        match self.to_str() {
            Some(s) => return Ok(Item::string(s)),
            None => return ctx.report_error("path containes invalid UTF-8 characters"),
        }
    }
}

impl ToToml for std::path::PathBuf {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        self.as_path().to_toml(ctx)
    }
}

impl ToToml for Array<'_> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(self.clone_in(ctx.arena).into_item())
    }
}

impl ToToml for Table<'_> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(self.clone_in(ctx.arena).into_item())
    }
}

impl ToToml for Item<'_> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(self.clone_in(ctx.arena))
    }
}

macro_rules! direct_upcast_integers {
    ($($tt:tt),*) => {
        $(impl ToToml for $tt {
            fn to_toml<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
                Ok(Item::from(*self as i64))
            }
        })*
    };
}

direct_upcast_integers!(u8, i8, i16, u16, i32, u32, i64);

pub trait ToFlattened {
    fn to_flattened<'a>(
        &'a self,
        ctx: &mut ToContext<'a>,
        table: &mut Table<'a>,
    ) -> Result<(), Failed>;
}

/// Serializes a map key to a TOML key string via `ToToml`.
fn key_to_str<'a>(item: &Item<'a>) -> Option<&'a str> {
    match item.value() {
        Value::String(s) => Some(*s),
        _ => None,
    }
}

impl<K: ToToml, V: ToToml> ToFlattened for BTreeMap<K, V> {
    fn to_flattened<'a>(
        &'a self,
        ctx: &mut ToContext<'a>,
        table: &mut Table<'a>,
    ) -> Result<(), Failed> {
        for (k, v) in self {
            let key_item = k.to_toml(ctx)?;
            let Some(key_str) = key_to_str(&key_item) else {
                return Err(Failed);
            };
            table.insert(
                Key::anon(key_str),
                v.to_toml(ctx)?,
                ctx.arena,
            );
        }
        Ok(())
    }
}

impl<K: ToToml, V: ToToml, H> ToFlattened for HashMap<K, V, H> {
    fn to_flattened<'a>(
        &'a self,
        ctx: &mut ToContext<'a>,
        table: &mut Table<'a>,
    ) -> Result<(), Failed> {
        for (k, v) in self {
            let key_item = k.to_toml(ctx)?;
            let Some(key_str) = key_to_str(&key_item) else {
                return Err(Failed);
            };
            table.insert(
                Key::anon(key_str),
                v.to_toml(ctx)?,
                ctx.arena,
            );
        }
        Ok(())
    }
}

impl<K: ToToml, V: ToToml> ToToml for BTreeMap<K, V> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut table) = Table::try_with_capacity(self.len(), &ctx.arena) else {
            return ctx.report_error("Length of table exceeded maximum capacity");
        };
        if let Err(err) = self.to_flattened(ctx, &mut table) {
            return Err(err);
        }
        Ok(table.into_item())
    }
}

impl<K: ToToml, V: ToToml, H> ToToml for HashMap<K, V, H> {
    fn to_toml<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut table) = Table::try_with_capacity(self.len(), &ctx.arena) else {
            return ctx.report_error("Length of table exceeded maximum capacity");
        };
        if let Err(err) = self.to_flattened(ctx, &mut table) {
            return Err(err);
        }
        Ok(table.into_item())
    }
}
