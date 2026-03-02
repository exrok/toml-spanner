use std::{io::Read, mem::MaybeUninit};
use toml_spanner::{DateTime, Value};

#[rustfmt::skip]
fn datetime_to_toml_kind(dt: &DateTime) -> &'static str {
    match (dt.date()  , dt.time()  , dt.offset()  ) {
          (Some(_date), Some(_time), Some(_offset)) => "datetime",
          (Some(_date), Some(_time), None         ) => "datetime-local",
          (Some(_date), None       , None         ) => "date-local",
          (None       , Some(_time), None         ) => "time-local",
        _ => unreachable!("for a DateTime produced from the toml-spanner::parse"),
    }
}

fn serialize(output: jsony::json::ValueWriter<'_, '_>, item: &toml_spanner::Item<'_>) {
    match item.value() {
        Value::String(value) => {
            let mut obj = output.object();
            obj.key("type").value(&"string");
            obj.key("value").value(value);
        }
        Value::Integer(value) => {
            let mut obj = output.object();
            obj.key("type").value(&"integer");
            obj.key("value").value(&value.to_string());
        }
        Value::Float(value) => {
            let mut obj = output.object();
            obj.key("type").value(&"float");
            obj.key("value").value(&value.to_string());
        }
        Value::Boolean(value) => {
            let mut obj = output.object();
            obj.key("type").value(&"bool");
            obj.key("value").value(&value.to_string());
        }
        Value::Array(array) => {
            let mut arr = output.array();
            for item in array.iter() {
                serialize(arr.value_writer(), item);
            }
        }
        Value::Table(table) => {
            let mut obj = output.object();
            for (key, item) in table {
                serialize(obj.key(key.as_str()), item);
            }
        }
        Value::DateTime(time) => {
            let mut obj = output.object();
            obj.key("type").value(&datetime_to_toml_kind(time));
            obj.key("value")
                .value(time.format(&mut MaybeUninit::uninit()));
        }
    }
}

fn main() {
    let mut input = std::io::stdin();
    let _ = input.lock();
    let mut buffer = Vec::new();
    let _ = input.read_to_end(&mut buffer);
    let Ok(content) = std::str::from_utf8(&buffer) else {
        std::process::exit(1)
    };
    let arena = toml_spanner::Arena::new();
    match toml_spanner::parse(content, &arena) {
        Ok(table) => {
            let mut text = jsony::TextWriter::new();
            let output = jsony::json::ValueWriter::new(&mut text);
            serialize(output, &table.into_item());
            println!("{}", text.as_str());
        }
        Err(err) => {
            use codespan_reporting::files::SimpleFiles;
            use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

            let mut files = SimpleFiles::new();
            let file_id = files.add("input.toml", content.to_string());
            let diagonistic = err.to_diagnostic(file_id);
            let writer = StandardStream::stderr(ColorChoice::Always);
            let config = codespan_reporting::term::Config::default();

            codespan_reporting::term::emit_to_io_write(
                &mut writer.lock(),
                &config,
                &files,
                &diagonistic,
            )
            .unwrap();
            std::process::exit(1)
        }
    }
}
