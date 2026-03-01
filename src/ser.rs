use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    rc::Rc,
    sync::Arc,
};

use crate::{Arena, Array, Failed, Item, Key, Table};

/// I don't like this name, but as is it's easily grepable
pub struct ToContext<'a> {
    pub arena: &'a Arena,
    pub(crate) error: Option<Cow<'static, str>>,
}
impl<'a> ToContext<'a> {
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

pub trait ToItem {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        optional_to_required(self.to_optional_item(ctx), ctx)
    }
    fn to_optional_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Option<Item<'a>>, Failed> {
        required_to_optional(self.to_item(ctx))
    }
}

impl<K: ToItem> ToItem for BTreeSet<K> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut array) = Array::try_with_capacity(self.len(), ctx.arena) else {
            return ctx.report_error("Length of array exceeded maximum capacity");
        };
        for item in self {
            array.push(
                match item.to_item(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(array.into_item())
    }
}

impl<K: AsRef<str>, V: ToItem> ToItem for BTreeMap<K, V> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut table) = Table::try_with_capacity(self.len(), &ctx.arena) else {
            return ctx.report_error("Length of table exceeded maximum capacity");
        };
        for (k, v) in self {
            table.insert(
                Key::anon(k.as_ref()),
                match v.to_item(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(table.into_item())
    }
}

impl<K: AsRef<str>, V: ToItem, H> ToItem for HashMap<K, V, H> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut table) = Table::try_with_capacity(self.len(), &ctx.arena) else {
            return ctx.report_error("Length of table exceeded maximum capacity");
        };
        for (k, v) in self {
            table.insert(
                Key::anon(k.as_ref()),
                match v.to_item(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(table.into_item())
    }
}

impl<K: ToItem, H> ToItem for std::collections::HashSet<K, H> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut array) = Array::try_with_capacity(self.len(), ctx.arena) else {
            return ctx.report_error("Length of array exceeded maximum capacity");
        };
        for item in self {
            array.push(
                match item.to_item(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(array.into_item())
    }
}

impl<const N: usize, T: ToItem> ToItem for [T; N] {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        self.as_slice().to_item(ctx)
    }
}

impl<T: ToItem> ToItem for Option<T> {
    fn to_optional_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Option<Item<'a>>, Failed> {
        match self {
            Some(value) => value.to_optional_item(ctx),
            None => Ok(None),
        }
    }
}

impl ToItem for str {
    fn to_item<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::string(self))
    }
}

impl ToItem for String {
    fn to_item<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::string(self))
    }
}

impl<T: ToItem> ToItem for Box<T> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToItem>::to_item(&*self, ctx)
    }
}

impl<T: ToItem> ToItem for [T] {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let Some(mut array) = Array::try_with_capacity(self.len(), ctx.arena) else {
            return ctx.report_error("Length of array exceeded maximum capacity");
        };
        for item in self {
            array.push(
                match item.to_item(ctx) {
                    Ok(it) => it,
                    Err(_) => return Err(Failed),
                },
                ctx.arena,
            );
        }
        Ok(array.into_item())
    }
}

impl<T: ToItem> ToItem for Vec<T> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        self.as_slice().to_item(ctx)
    }
}

impl ToItem for f32 {
    fn to_item<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::from(*self as f64))
    }
}

impl ToItem for f64 {
    fn to_item<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::from(*self))
    }
}

impl ToItem for bool {
    fn to_item<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(Item::from(*self))
    }
}

impl<T: ToItem + ?Sized> ToItem for &T {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToItem>::to_item(self, ctx)
    }
}

impl<T: ToItem + ?Sized> ToItem for &mut T {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToItem>::to_item(self, ctx)
    }
}

impl<T: ToItem> ToItem for Rc<T> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToItem>::to_item(self, ctx)
    }
}

impl<T: ToItem> ToItem for Arc<T> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToItem>::to_item(self, ctx)
    }
}

impl<'b, T: ToItem + Clone> ToItem for Cow<'b, T> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        <T as ToItem>::to_item(self, ctx)
    }
}

impl ToItem for char {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        let mut buf = [0; 4];
        Ok(Item::string(
            ctx.arena.alloc_str(self.encode_utf8(&mut buf)),
        ))
    }
}

impl ToItem for std::path::Path {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        match self.to_str() {
            Some(s) => return Ok(Item::string(s)),
            None => return ctx.report_error("path containes invalid UTF-8 characters"),
        }
    }
}

impl ToItem for std::path::PathBuf {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        self.as_path().to_item(ctx)
    }
}

impl ToItem for Array<'_> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(self.clone_in(ctx.arena).into_item())
    }
}

impl ToItem for Table<'_> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(self.clone_in(ctx.arena).into_item())
    }
}

impl ToItem for Item<'_> {
    fn to_item<'a>(&'a self, ctx: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
        Ok(self.clone_in(ctx.arena))
    }
}

macro_rules! direct_upcast_integers {
    ($($tt:tt),*) => {
        $(impl ToItem for $tt {
            fn to_item<'a>(&'a self, _: &mut ToContext<'a>) -> Result<Item<'a>, Failed> {
                Ok(Item::from(*self as i64))
            }
        })*
    };
}

direct_upcast_integers!(u8, i8, i16, u16, i32, u32, i64);
