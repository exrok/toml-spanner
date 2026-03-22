use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

struct Project {
    display_name: &'static str,
    dir_name: &'static str,
    source: &'static str,
    cargo_toml: String,
    version_package: Option<&'static str>,
}

struct Stats {
    min: u128,
    median: u128,
    mean: u128,
    max: u128,
}

impl Stats {
    fn compute(durations: &[Duration]) -> Stats {
        assert!(!durations.is_empty(), "no successful builds");
        let mut ms: Vec<u128> = durations.iter().map(|d| d.as_millis()).collect();
        ms.sort();
        Stats {
            min: ms[0],
            median: ms[ms.len() / 2],
            mean: ms.iter().sum::<u128>() / ms.len() as u128,
            max: ms[ms.len() - 1],
        }
    }
}

fn spanner_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn cargo_toml(name: &str, deps: &str) -> String {
    let mut s = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n");
    if !deps.is_empty() {
        s.push_str("\n[dependencies]\n");
        s.push_str(deps);
        s.push('\n');
    }
    s
}

fn make_projects() -> Vec<Project> {
    let spanner = spanner_path();
    let spanner_dep = format!("toml-spanner.path = \"{}\"", spanner.display());

    vec![
        Project {
            display_name: "null",
            dir_name: "toml_compile_bench_null",
            source: include_str!("compile_examples/bin_null.rs"),
            cargo_toml: cargo_toml("null", ""),
            version_package: None,
        },
        Project {
            display_name: "toml-spanner",
            dir_name: "toml_compile_bench_spanner",
            source: include_str!("compile_examples/bin_toml_spanner.rs"),
            cargo_toml: cargo_toml("toml-compile-bench-spanner", &spanner_dep),
            version_package: Some("toml-spanner"),
        },
        Project {
            display_name: "toml-span",
            dir_name: "toml_compile_bench_toml_span",
            source: include_str!("compile_examples/bin_toml_span.rs"),
            cargo_toml: cargo_toml("toml-compile-bench-toml-span", "toml-span = \"0.7\""),
            version_package: Some("toml-span"),
        },
        Project {
            display_name: "toml",
            dir_name: "toml_compile_bench_toml",
            source: include_str!("compile_examples/bin_toml.rs"),
            cargo_toml: cargo_toml("toml-compile-bench-toml", "toml = \"1\""),
            version_package: Some("toml"),
        },
        Project {
            display_name: "toml+serde",
            dir_name: "toml_compile_bench_toml_serde",
            source: include_str!("compile_examples/bin_toml_with_serde.rs"),
            cargo_toml: cargo_toml(
                "toml-compile-bench-toml-serde",
                "serde = { version = \"1\", features = [\"derive\"] }\ntoml = \"1\"",
            ),
            version_package: Some("toml"),
        },
    ]
}

const MAIN_SUFFIX: &str = r#"

fn main() {
    let input = std::io::read_to_string(std::io::stdin()).unwrap();
    let mut project = run(&input);
    std::hint::black_box(&mut project);
}
"#;

const ITERATIONS: usize = 5;

struct Result {
    name: &'static str,
    version: String,
    stats: Stats,
}

pub fn run_compile_bench(release: bool, all: bool, report: bool) {
    let mut projects = make_projects();
    if !all {
        projects.retain(|p| p.display_name == "null" || p.display_name == "toml-spanner");
    }
    let mode = if release { "release" } else { "debug" };
    println!("=== Compile-time benchmark ({mode} builds, {ITERATIONS} iterations) ===\n");

    let mut results: Vec<Result> = Vec::new();

    for project in &projects {
        let base = PathBuf::from(format!("/tmp/{}", project.dir_name));
        let src_dir = base.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        std::fs::write(base.join("Cargo.toml"), &project.cargo_toml).unwrap();

        let main_rs = format!("{}{MAIN_SUFFIX}", project.source);
        std::fs::write(src_dir.join("main.rs"), &main_rs).unwrap();

        let target_dir = base.join("target");

        println!("--- {} ---", project.display_name);
        print!("  warmup build... ");

        let warmup = Command::new("cargo")
            .arg("build")
            .args(if release { &["--release"][..] } else { &[][..] })
            .current_dir(&base)
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("failed to run cargo");

        if !warmup.status.success() {
            panic!(
                "warmup build failed for {}:\n{}",
                project.display_name,
                String::from_utf8_lossy(&warmup.stderr)
            );
        }
        println!("ok");

        let version = match project.version_package {
            Some(pkg) => {
                let v = super::lockfile_version(&base.join("Cargo.lock"), pkg);
                println!("  version: {v}");
                v
            }
            None => String::new(),
        };

        let mut durations = Vec::with_capacity(ITERATIONS);
        for i in 0..ITERATIONS {
            let clean = Command::new("cargo")
                .args(["clean"])
                .current_dir(&base)
                .env("CARGO_TARGET_DIR", &target_dir)
                .output()
                .expect("failed to run cargo clean");

            assert!(
                clean.status.success(),
                "clean failed: {}",
                String::from_utf8_lossy(&clean.stderr)
            );

            let start = Instant::now();
            let build = Command::new("cargo")
                .arg("build")
                // Limit jobs to simulate more realistic results when being built in real programs
                // with more dependencies or on more constrained systems like CI.
                .args(["--jobs", "2"])
                .args(if release { &["--release"][..] } else { &[][..] })
                .current_dir(&base)
                .env("CARGO_TARGET_DIR", &target_dir)
                .output()
                .expect("failed to run cargo build");
            let elapsed = start.elapsed();

            assert!(
                build.status.success(),
                "build {} failed: {}",
                i + 1,
                String::from_utf8_lossy(&build.stderr)
            );

            durations.push(elapsed);
            println!("  [{}/{}] {:.0?}", i + 1, ITERATIONS, elapsed);
        }

        results.push(Result {
            name: project.display_name,
            version,
            stats: Stats::compute(&durations),
        });
        println!();
    }

    if !report {
        return;
    }

    // Print summary table
    println!("=== Summary ({mode}) ===\n");
    println!(
        "{:<16} {:>8} {:>8} {:>8} {:>8}  (ms)",
        "project", "min", "median", "mean", "max"
    );
    println!("{}", "-".repeat(60));

    for r in &results {
        println!(
            "{:<16} {:>8} {:>8} {:>8} {:>8}",
            r.name, r.stats.min, r.stats.median, r.stats.mean, r.stats.max
        );
    }

    if !release {
        return;
    }

    let null_median = results
        .iter()
        .find(|r| r.name == "null")
        .expect("no null baseline result")
        .stats
        .median;

    // Generate gnuplot data — iterate results in order, skip null
    let mut data = String::new();
    for (i, r) in results.iter().filter(|r| r.name != "null").enumerate() {
        let delta = r.stats.median.saturating_sub(null_median);
        let label = if r.version.is_empty() {
            r.name.to_string()
        } else {
            format!("{{/:Bold {}}}{{/=11 -VER-{}}}", r.name, r.version)
        };
        writeln!(data, "\"{label}\" {delta} {i}").unwrap();
    }

    let max_delta = results
        .iter()
        .filter(|r| r.name != "null")
        .map(|r| r.stats.median.saturating_sub(null_median))
        .max()
        .unwrap_or(0);

    // Run gnuplot
    let gnuplot_template = include_str!("../plot_compile.gnuplot");
    let (s1, rest) = gnuplot_template
        .split_once("__INSERT_LABEL_HERE__")
        .expect("Missing __INSERT_LABEL_HERE__");
    let (s2, s3) = rest
        .split_once("__INSERT_DATA_HERE__")
        .expect("Missing __INSERT_DATA_HERE__");

    let mut script = String::new();
    script.push_str(s1);
    write!(script, "\"Additional compile time over baseline (ms)\"").unwrap();
    script.push_str(s2);
    script.push_str(&data);
    let s3 = s3.replacen(
        "plot ",
        &format!("set xrange [0:{}]\nplot ", max_delta + 500),
        1,
    );
    script.push_str(&s3);

    let mut gnuplot = Command::new("gnuplot")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn gnuplot — is gnuplot installed?");
    gnuplot
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = gnuplot.wait_with_output().unwrap();
    let svg = String::from_utf8(output.stdout).unwrap();
    let svg = svg.replace(">-VER-", " dx=\"1ch\">");

    let assets_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
    std::fs::create_dir_all(&assets_dir).ok();
    let svg_path = assets_dir.join("compile_bench.svg");
    std::fs::write(&svg_path, &svg).expect("Failed to write SVG");
    eprintln!("Wrote SVG to {}", svg_path.display());

    // Print markdown table for README
    eprintln!("\n--- README Markdown ---\n");
    let version_line: Vec<_> = results
        .iter()
        .filter(|r| !r.version.is_empty())
        .map(|r| format!("{} {}", r.name, r.version))
        .collect();
    eprintln!("Versions: {}\n", version_line.join(", "));
    eprintln!("![compile benchmark](assets/compile_bench.svg)\n");
    eprintln!("```");
    eprintln!("{:<16} {:>10} {:>12}", "", "median(ms)", "added(ms)");
    for r in &results {
        if r.name == "null" {
            eprintln!("{:<16} {:>10}", r.name, r.stats.median);
        } else {
            let delta = r.stats.median.saturating_sub(null_median);
            eprintln!("{:<16} {:>10} {:>+12}", r.name, r.stats.median, delta);
        }
    }
    eprintln!("```");
}
