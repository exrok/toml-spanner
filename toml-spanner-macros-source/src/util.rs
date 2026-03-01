use std::alloc::Layout;
use std::io::Write;
use std::mem::MaybeUninit;

const FREAKY_BUCKET_SIZE: usize = 32;

struct Bucket<T> {
    data: [MaybeUninit<T>; FREAKY_BUCKET_SIZE],
    next: *mut Bucket<T>,
}

pub struct MemoryPool<T> {
    current_bucket: *mut Bucket<T>,
    current_size: usize,
}

impl<T> MemoryPool<T> {
    pub fn new() -> Self {
        Self {
            current_size: 0,
            current_bucket: std::ptr::null_mut(),
        }
    }
    pub fn allocator<'a>(&'a mut self) -> Allocator<'a, T> {
        Allocator { inner: self }
    }
}

impl<T> Drop for MemoryPool<T> {
    fn drop(&mut self) {
        let mut current_bucket = self.current_bucket;
        let mut current_size = self.current_size;
        while current_bucket != std::ptr::null_mut() {
            unsafe {
                let bucket = &mut *current_bucket;
                std::ptr::drop_in_place(std::slice::from_raw_parts_mut(
                    &mut bucket.data as *mut _ as *mut T,
                    current_size,
                ) as *mut [T]);
                current_bucket = bucket.next;
                current_size = FREAKY_BUCKET_SIZE;
                std::alloc::dealloc(current_bucket as *mut u8, Layout::new::<Bucket<T>>());
            }
        }
    }
}

pub struct Allocator<'a, T> {
    inner: &'a mut MemoryPool<T>,
}
impl<'a, T: Default> Allocator<'a, T> {
    pub fn alloc_default(&mut self) -> &'a mut T {
        if self.inner.current_bucket == std::ptr::null_mut()
            || self.inner.current_size >= FREAKY_BUCKET_SIZE
        {
            unsafe {
                let ptr = std::alloc::alloc(Layout::new::<Bucket<T>>()) as *mut Bucket<T>;
                if ptr.is_null() {
                    panic!();
                }
                ptr.byte_add(std::mem::offset_of!(Bucket<T>, next))
                    .cast::<*mut Bucket<T>>()
                    .write(self.inner.current_bucket);

                self.inner.current_bucket = ptr;
                self.inner.current_size = 0;
            }
        }
        unsafe {
            let entries = self.inner.current_bucket as *mut T;
            let tail = entries.add(self.inner.current_size);
            tail.write(<T as Default>::default());
            self.inner.current_size += 1;
            return unsafe { &mut *tail };
        }
    }
}

use proc_macro2::token_stream::IntoIter as Tokens;
use proc_macro2::{self as proc_macro, Spacing, TokenStream};
struct Formatter {
    output: Vec<u8>,
    line_index: usize,
    last_indent: usize,
    line_pre_index: usize,
    colors: bool,
}

fn is_rust_builtin_type(ident: &str) -> bool {
    matches!(
        ident,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "str"
            | "Self"
    )
}
fn is_rust_keyword(ident: &str) -> bool {
    matches!(
        ident,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
            | "union"
    )
}
const RED: &str = "\x1b[31m";
const BLUE: &str = "\x1b[34m";
const ORANGE: &str = "\x1b[33m";

impl Formatter {
    fn next_line(&mut self, indent: usize) {
        if self.output.len() == self.line_pre_index {
            while self.line_length() > indent * 4 {
                self.output.pop();
            }
            self.line_pre_index = self.output.len();
            return;
        }
        self.output.push(b'\n');
        self.line_index = self.output.len();
        for _ in 0..indent {
            self.output.extend_from_slice(b"    ");
        }
        self.line_pre_index = self.output.len();
    }
    fn force_space(&mut self) {
        self.output.push(b' ');
    }
    fn space(&mut self) {
        if let Some(ch) = self.output.last() {
            match ch {
                b' ' | b':' | b'(' | b'<' | b'>' | b'.' | b'&' => {}
                _ => {
                    self.output.push(b' ');
                }
            }
        }
    }
    fn line_length(&self) -> usize {
        self.output.len() - self.line_index
    }
    fn cls(&mut self) {
        if self.colors {
            self.output.extend_from_slice(b"\x1b[0m")
        }
    }
    fn clsx(&mut self) {
        if self.colors {
            self.output.extend_from_slice(b"\x1b[39m")
        }
    }
    fn green(&mut self) {
        if self.colors {
            self.output.extend_from_slice(b"\x1b[32m")
        }
    }
    fn yellow(&mut self) {
        if self.colors {
            self.output.extend_from_slice(b"\x1b[93m")
        }
    }
    fn red(&mut self) {
        if self.colors {
            self.output.extend_from_slice(b"\x1b[31m")
        }
    }
    fn purple(&mut self) {
        if self.colors {
            self.output.extend_from_slice(b"\x1b[35m")
        }
    }
    fn color_last_ident(&mut self, color: &str) {
        if self.colors {
            let Some(range) = self.output.get_mut(self.last_indent..self.last_indent + 5) else {
                return;
            };
            if range == b"\x1b[39m" {
                range.copy_from_slice(color.as_bytes())
            }
        }
    }
    fn rec(&mut self, indent: usize, tokens: Tokens, colon_break: bool) {
        let mut last_was_ident_xx = false;
        let mut matching = false;
        let mut joint_tick_xx = false;
        for token in tokens {
            let last_was_ident = last_was_ident_xx;
            let joint_tick = joint_tick_xx;
            last_was_ident_xx = false;
            joint_tick_xx = false;
            match token {
                proc_macro::TokenTree::Group(group) => {
                    let mut gindent = indent;
                    let mut enable_comma_break = matching;
                    let close = match group.delimiter() {
                        proc_macro2::Delimiter::Parenthesis => {
                            gindent += 1;
                            if last_was_ident {
                                self.color_last_ident(BLUE)
                            }
                            self.output.push(b'(');
                            ")"
                        }
                        proc_macro2::Delimiter::Brace => {
                            gindent += 1;
                            self.space();
                            enable_comma_break = true;
                            self.output.push(b'{');
                            self.next_line(gindent);
                            "}"
                        }
                        proc_macro2::Delimiter::Bracket => {
                            self.output.push(b'[');
                            "]"
                        }
                        proc_macro2::Delimiter::None => "",
                    };
                    self.rec(gindent, group.stream().into_iter(), enable_comma_break);
                    matching = false;
                    if close == "}" {
                        self.next_line(indent);
                    }
                    self.output.extend_from_slice(close.as_bytes());
                }
                proc_macro::TokenTree::Ident(ident) => {
                    last_was_ident_xx = true;
                    match self.output.last() {
                        Some(b'}') => {
                            self.next_line(indent);
                        }
                        _ => {
                            if !joint_tick {
                                self.space();
                            }
                        }
                    }
                    let fmt = ident.to_string();
                    if fmt == "match" {
                        matching = true;
                    }
                    self.last_indent = self.output.len();
                    if joint_tick {
                        self.red();
                    } else if is_rust_builtin_type(&fmt) {
                        self.yellow();
                    } else if is_rust_keyword(&fmt) {
                        self.purple();
                    } else if let Some(ch) = fmt.as_bytes().first() {
                        if ch.is_ascii_uppercase() {
                            self.yellow();
                        } else {
                            self.clsx();
                        }
                    } else {
                        self.clsx();
                    }
                    self.output.extend_from_slice(fmt.as_bytes());
                    self.cls();
                }

                proc_macro::TokenTree::Punct(punct) => match punct.as_char() {
                    ',' => {
                        if colon_break || self.output.len() - self.line_index > 120 {
                            self.output.push(b',');
                            self.next_line(indent);
                        } else {
                            self.output.extend_from_slice(", ".as_bytes());
                        }
                    }
                    ':' => {
                        if punct.spacing() == Spacing::Alone {
                            let x = if self.output.last() != Some(&b':') {
                                self.color_last_ident(RED);
                                true
                            } else {
                                false
                            };
                            self.output.push(b':');
                            if x {
                                self.force_space();
                            }
                        } else {
                            if last_was_ident {
                                self.color_last_ident(ORANGE)
                            }
                            self.output.push(b':');
                        }
                    }
                    '>' => {
                        if self.output.last() == Some(&b'=') {
                            self.output.push(b'>');
                            self.space();
                        } else {
                            self.output.push(b'>');
                        }
                    }
                    '=' => {
                        if punct.spacing() == Spacing::Alone {
                            if self.output.last() != Some(&b'=') {
                                self.space();
                                self.color_last_ident(RED)
                            }
                        } else {
                            self.space();
                        }
                        self.output.push(b'=');
                    }
                    '-' => {
                        if self.output.last() == Some(&b')') {
                            self.output.extend_from_slice(" -".as_bytes());
                        }
                    }
                    ';' => {
                        self.output.extend_from_slice(b";");
                        self.next_line(indent);
                    }
                    '\'' => {
                        if punct.spacing() == Spacing::Joint {
                            joint_tick_xx = true;
                        }
                        self.output.push(b'\'');
                    }
                    ch => {
                        self.output.push(ch as u8);
                    }
                },
                proc_macro::TokenTree::Literal(literal) => {
                    self.space();
                    let fmt = literal.to_string();
                    if fmt.starts_with(['\'', '"']) {
                        self.green();
                    }
                    self.output.extend_from_slice(fmt.as_bytes());
                    self.cls();
                }
            }
        }
    }
}

pub fn print_pretty_and_copy(tokens: TokenStream) {
    let mut buffer = Formatter {
        output: Vec::with_capacity(1024),
        line_index: 0,
        last_indent: 0,
        line_pre_index: 0,
        colors: true,
    };
    buffer.rec(0, tokens.clone().into_iter(), false);
    let _ = std::io::stdout().write_all(&buffer.output);
    let mut buffer = Formatter {
        output: Vec::with_capacity(1024),
        line_index: 0,
        last_indent: 0,
        line_pre_index: 0,
        colors: false,
    };
    buffer.rec(0, tokens.into_iter(), false);
    xsel_clipboard(&buffer.output);
}

fn xsel_clipboard(text: &[u8]) {
    let mut child = std::process::Command::new("xsel")
        .arg("-ib")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(text).unwrap();
}

pub fn print_pretty(tokens: TokenStream) {
    let mut buffer = Formatter {
        output: Vec::with_capacity(1024),
        line_index: 0,
        last_indent: 0,
        line_pre_index: 0,
        colors: true,
    };
    buffer.rec(0, tokens.into_iter(), false);
    let _ = std::io::stdout().write_all(&buffer.output);
}
