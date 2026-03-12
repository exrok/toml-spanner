#[allow(dead_code)]
pub struct TestCase<'a> {
    pub name: &'a str,
    pub source: Option<&'a str>,
    pub dest: Option<&'a str>,
    pub modified: Option<&'a str>,
    pub expected: Option<&'a str>,
}

#[allow(dead_code)]
impl<'a> TestCase<'a> {
    pub fn source(&self) -> &'a str {
        self.source
            .unwrap_or_else(|| panic!("test case {:?}: missing SOURCE field", self.name))
    }

    pub fn dest(&self) -> &'a str {
        self.dest
            .unwrap_or_else(|| panic!("test case {:?}: missing DEST field", self.name))
    }

    pub fn modified(&self) -> &'a str {
        self.modified
            .unwrap_or_else(|| panic!("test case {:?}: missing MODIFIED field", self.name))
    }

    pub fn expected(&self) -> &'a str {
        self.expected
            .unwrap_or_else(|| panic!("test case {:?}: missing EXPECTED field", self.name))
    }
}

pub fn parse_test_cases(data: &str) -> Vec<TestCase<'_>> {
    let mut cases = Vec::new();

    let mut case_starts: Vec<(usize, &str)> = Vec::new();
    for line in data.lines() {
        if let Some(name) = line.strip_prefix("#--- ") {
            let byte_offset = line.as_ptr() as usize - data.as_ptr() as usize;
            case_starts.push((byte_offset, name.trim()));
        }
    }

    for (idx, &(start, name)) in case_starts.iter().enumerate() {
        let line_end = start + data[start..].find('\n').unwrap_or(data.len() - start);
        let raw_start = (line_end + 1).min(data.len());
        let raw_end = if idx + 1 < case_starts.len() {
            case_starts[idx + 1].0
        } else {
            data.len()
        };
        let raw = &data[raw_start..raw_end];

        let mut source = None;
        let mut dest = None;
        let mut modified = None;
        let mut expected = None;

        if raw.contains("\n#-- ") || raw.starts_with("#-- ") {
            let mut field_starts: Vec<(usize, &str)> = Vec::new();
            for line in raw.lines() {
                if let Some(field_name) = line.strip_prefix("#-- ") {
                    let offset = line.as_ptr() as usize - raw.as_ptr() as usize;
                    field_starts.push((offset, field_name.trim()));
                }
            }

            for (fi, &(fstart, fname)) in field_starts.iter().enumerate() {
                let line_end = fstart + raw[fstart..].find('\n').unwrap_or(raw.len() - fstart);
                let content_start = (line_end + 1).min(raw.len());
                let content_end = if fi + 1 < field_starts.len() {
                    let next = field_starts[fi + 1].0;
                    if next > 0 && raw.as_bytes()[next - 1] == b'\n' {
                        next - 1
                    } else {
                        next
                    }
                } else {
                    let end = raw.len();
                    if end > 0 && raw.as_bytes()[end - 1] == b'\n' {
                        end - 1
                    } else {
                        end
                    }
                };
                let content = if content_start <= content_end {
                    &raw[content_start..content_end]
                } else {
                    ""
                };
                match fname {
                    "SOURCE" => source = Some(content),
                    "DEST" => dest = Some(content),
                    "MODIFIED" => modified = Some(content),
                    "EXPECTED" => expected = Some(content),
                    _ => panic!(
                        "test case {name:?}: unknown field {fname:?} (expected SOURCE, DEST, MODIFIED, or EXPECTED)"
                    ),
                }
            }
        } else {
            let trimmed = raw.strip_prefix('\n').unwrap_or(raw);
            let trimmed = trimmed.strip_suffix('\n').unwrap_or(trimmed);
            source = Some(trimmed);
        }

        cases.push(TestCase {
            name,
            source,
            dest,
            modified,
            expected,
        });
    }

    cases
}

pub fn run_cases(cases: &[TestCase<'_>], f: impl Fn(&TestCase<'_>)) {
    for case in cases {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(case)));
        if let Err(e) = result {
            eprintln!("FAILED test case: {:?}", case.name);
            std::panic::resume_unwind(e);
        }
    }
}
