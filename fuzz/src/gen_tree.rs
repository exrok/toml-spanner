use crate::{Gen, pick_unique_idx};
use toml_spanner::dev;
use toml_spanner::{Arena, Array, ArrayKind, Item, Key, Table, TableKind, Value};

pub const KEYS: [&str; 8] = ["a", "b", "c", "d", "e", "x", "y", "z"];
pub const N_KEYS: usize = KEYS.len();
const STRINGS: [&str; 5] = ["", "a", "b", "hello", "world"];

pub fn random_table_kind(g: &mut Gen<'_>) -> TableKind {
    match g.next() % 4 {
        0 => TableKind::Implicit,
        1 => TableKind::Dotted,
        2 => TableKind::Header,
        _ => TableKind::Inline,
    }
}

pub fn random_array_kind(g: &mut Gen<'_>) -> ArrayKind {
    match g.next() % 2 {
        0 => ArrayKind::Inline,
        _ => ArrayKind::Header,
    }
}

pub fn gen_item<'de>(g: &mut Gen<'_>, arena: &'de Arena, depth: u8) -> Item<'de> {
    let kind = if depth >= 3 {
        g.next() % 4
    } else {
        g.next() % 6
    };
    match kind {
        0 => {
            let s = STRINGS[g.next() as usize % STRINGS.len()];
            Item::string(s)
        }
        1 => Item::from(g.next() as i64),
        2 => dev::make_float(g.next() as f64),
        3 => dev::make_boolean(g.next() % 2 == 0),
        4 => gen_array_item(g, arena, depth),
        _ => gen_table_item(g, arena, depth),
    }
}

pub fn gen_table_item<'de>(g: &mut Gen<'_>, arena: &'de Arena, depth: u8) -> Item<'de> {
    let mut table = Table::default();
    let count = g.range(0, 4) as usize;
    let mut used = [false; N_KEYS];
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        let child = gen_item(g, arena, depth + 1);
        table.insert(Key::anon(KEYS[ki]), child, arena);
    }
    table.set_kind(random_table_kind(g));
    table.into_item()
}

pub fn gen_array_item<'de>(g: &mut Gen<'_>, arena: &'de Arena, depth: u8) -> Item<'de> {
    let count = g.range(0, 4);
    let mut arr = Array::default();
    for _ in 0..count {
        let elem = gen_item(g, arena, depth + 1);
        arr.push(elem, arena);
    }
    arr.set_kind(random_array_kind(g));
    arr.into_item()
}

pub fn gen_root_table<'de>(g: &mut Gen<'_>, arena: &'de Arena) -> Table<'de> {
    let mut root = Table::default();
    let count = g.range(1, 5) as usize;
    let mut used = [false; N_KEYS];
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        let item = gen_item(g, arena, 0);
        root.insert(Key::anon(KEYS[ki]), item, arena);
    }
    root
}

pub fn flag_name(flag: u32) -> &'static str {
    match flag {
        0 => "NONE",
        1 => "???1",
        2 => "ARRAY",
        3 => "AOT",
        4 => "IMPLICIT",
        5 => "DOTTED",
        6 => "HEADER",
        7 => "FROZEN",
        _ => "UNKNOWN",
    }
}

pub fn items_eq(a: &Item<'_>, b: &Item<'_>, path: &mut Vec<String>) -> Result<(), String> {
    let p = || {
        if path.is_empty() {
            "<root>".to_string()
        } else {
            path.join(".")
        }
    };

    if a.kind() as u8 != b.kind() as u8 {
        return Err(format!(
            "kind mismatch at {}: {:?} vs {:?}",
            p(),
            a.kind(),
            b.kind()
        ));
    }

    if a.flag() != b.flag() {
        return Err(format!(
            "flag mismatch at {}: {} vs {}",
            p(),
            flag_name(a.flag()),
            flag_name(b.flag())
        ));
    }

    match a.value() {
        Value::String(s) => {
            if b.as_str() != Some(*s) {
                return Err(format!(
                    "string mismatch at {}: {:?} vs {:?}",
                    p(),
                    s,
                    b.as_str()
                ));
            }
        }
        Value::Integer(i) => {
            if b.as_i64() != Some(*i) {
                return Err(format!(
                    "integer mismatch at {}: {} vs {:?}",
                    p(),
                    i,
                    b.as_i64()
                ));
            }
        }
        Value::Float(f) => {
            let bf = b.as_f64().unwrap();
            if f.to_bits() != bf.to_bits() {
                return Err(format!("float mismatch at {}: {} vs {}", p(), f, bf));
            }
        }
        Value::Boolean(v) => {
            if b.as_bool() != Some(*v) {
                return Err(format!(
                    "boolean mismatch at {}: {} vs {:?}",
                    p(),
                    v,
                    b.as_bool()
                ));
            }
        }
        Value::DateTime(dt_a) => {
            let dt_b = b.as_datetime().unwrap();
            if dt_a.date() != dt_b.date()
                || dt_a.time() != dt_b.time()
                || dt_a.offset() != dt_b.offset()
            {
                return Err(format!("datetime mismatch at {}", p()));
            }
        }
        Value::Array(arr_a) => {
            let arr_b = b.as_array().unwrap();
            if arr_a.len() != arr_b.len() {
                return Err(format!(
                    "array length mismatch at {}: {} vs {}",
                    p(),
                    arr_a.len(),
                    arr_b.len()
                ));
            }
            for i in 0..arr_a.len() {
                path.push(format!("[{i}]"));
                items_eq(&arr_a.as_slice()[i], &arr_b.as_slice()[i], path)?;
                path.pop();
            }
        }
        Value::Table(tab_a) => {
            let tab_b = b.as_table().unwrap();
            if tab_a.len() != tab_b.len() {
                return Err(format!(
                    "table length mismatch at {}: {} vs {}\n  keys_a: {:?}\n  keys_b: {:?}",
                    p(),
                    tab_a.len(),
                    tab_b.len(),
                    tab_a
                        .entries()
                        .iter()
                        .map(|(k, _)| k.name)
                        .collect::<Vec<_>>(),
                    tab_b
                        .entries()
                        .iter()
                        .map(|(k, _)| k.name)
                        .collect::<Vec<_>>(),
                ));
            }
            for (key, val_a) in tab_a {
                path.push(key.name.to_string());
                let Some(val_b) = tab_b.get(key.name) else {
                    return Err(format!("key {} missing in parsed output", path.join(".")));
                };
                items_eq(val_a, val_b, path)?;
                path.pop();
            }
        }
    }
    Ok(())
}

pub fn print_item(item: &Item<'_>, indent: usize, prefix: &str) {
    let pad = " ".repeat(indent);
    let kind = item.kind();
    let flag = flag_name(item.flag());

    match kind {
        toml_spanner::Kind::String => {
            println!(
                "{pad}{prefix}String({flag}) = {:?}",
                item.as_str().unwrap_or("???")
            );
        }
        toml_spanner::Kind::Integer => {
            println!(
                "{pad}{prefix}Integer({flag}) = {}",
                item.as_i64().unwrap_or(0)
            );
        }
        toml_spanner::Kind::Float => {
            println!(
                "{pad}{prefix}Float({flag}) = {}",
                item.as_f64().unwrap_or(0.0)
            );
        }
        toml_spanner::Kind::Boolean => {
            println!(
                "{pad}{prefix}Boolean({flag}) = {}",
                item.as_bool().unwrap_or(false)
            );
        }
        toml_spanner::Kind::DateTime => {
            println!("{pad}{prefix}DateTime({flag})");
        }
        toml_spanner::Kind::Array => {
            if let Some(arr) = item.as_array() {
                println!("{pad}{prefix}Array({flag}) [{} elements]", arr.len());
                for (i, elem) in arr.iter().enumerate() {
                    print_item(elem, indent + 2, &format!("[{i}] "));
                }
            } else {
                println!("{pad}{prefix}Array({flag}) [WRONG FLAG]");
            }
        }
        toml_spanner::Kind::Table => {
            if let Some(tab) = item.as_table() {
                println!("{pad}{prefix}Table({flag}) {{{} entries}}", tab.len());
                for (key, val) in tab {
                    print_item(val, indent + 2, &format!("{} = ", key.name));
                }
            } else {
                println!("{pad}{prefix}Table({flag}) [WRONG FLAG - children hidden]");
            }
        }
    }
}

pub fn print_table(table: &Table<'_>, label: &str) {
    println!("── {label} ──");
    for (key, val) in table {
        print_item(val, 0, &format!("{} = ", key.name));
    }
}

/// Reset all structural kinds to Implicit (tables) or Inline (arrays),
/// preserving Dotted and Inline table kinds which are content-level.
pub fn erase_kinds_table(table: &mut Table<'_>) {
    for (_, item) in table {
        erase_kinds_item(item);
    }
}

pub fn erase_kinds_item(item: &mut Item<'_>) {
    if let Some(t) = item.as_table_mut() {
        match t.kind() {
            TableKind::Dotted | TableKind::Inline => {}
            _ => t.set_kind(TableKind::Implicit),
        }
        erase_kinds_table(t);
    } else if let Some(a) = item.as_array_mut() {
        a.set_kind(ArrayKind::Inline);
        for elem in a.as_mut_slice() {
            erase_kinds_item(elem);
        }
    }
}
