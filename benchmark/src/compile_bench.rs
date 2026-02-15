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
}

struct Stats {
    min: u128,
    median: u128,
    mean: u128,
    max: u128,
}

impl Stats {
    fn compute(durations: &[Duration]) -> Option<Self> {
        if durations.is_empty() {
            return None;
        }
        let mut ms: Vec<u128> = durations.iter().map(|d| d.as_millis()).collect();
        ms.sort();
        Some(Stats {
            min: ms[0],
            median: ms[ms.len() / 2],
            mean: ms.iter().sum::<u128>() / ms.len() as u128,
            max: ms[ms.len() - 1],
        })
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
    let spanner_dep = format!(
        "toml-spanner = {{ path = {:?} }}",
        spanner.to_str().unwrap()
    );

    vec![
        Project {
            display_name: "null",
            dir_name: "toml_compile_bench_null",
            source: include_str!("compile_examples/bin_null.rs"),
            cargo_toml: cargo_toml("null", ""),
        },
        Project {
            display_name: "toml-spanner",
            dir_name: "toml_compile_bench_spanner",
            source: include_str!("compile_examples/bin_toml_spanner.rs"),
            cargo_toml: cargo_toml("toml-compile-bench-spanner", &spanner_dep),
        },
        Project {
            display_name: "toml-span",
            dir_name: "toml_compile_bench_toml_span",
            source: include_str!("compile_examples/bin_toml_span.rs"),
            cargo_toml: cargo_toml("toml-compile-bench-toml-span", "toml-span = \"0.7\""),
        },
        Project {
            display_name: "toml",
            dir_name: "toml_compile_bench_toml",
            source: include_str!("compile_examples/bin_toml.rs"),
            cargo_toml: cargo_toml("toml-compile-bench-toml", "toml = \"1\""),
        },
        Project {
            display_name: "toml+serde",
            dir_name: "toml_compile_bench_toml_serde",
            source: include_str!("compile_examples/bin_toml_with_serde.rs"),
            cargo_toml: cargo_toml(
                "toml-compile-bench-toml-serde",
                "serde = { version = \"1\", features = [\"derive\"] }\ntoml = \"1\"",
            ),
        },
    ]
}

const MAIN_SUFFIX: &str = r#"

fn main() {
    let input = std::io::read_to_string(std::io::stdin()).unwrap();
    run(&input);
}
"#;

const ITERATIONS: usize = 5;

pub fn run_compile_bench(release: bool) {
    let projects = make_projects();
    let mode = if release { "release" } else { "debug" };
    println!("=== Compile-time benchmark ({mode} builds, {ITERATIONS} iterations) ===\n");

    let mut results: Vec<(&str, Option<Stats>)> = Vec::new();

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
            eprintln!("FAILED");
            eprintln!("{}", String::from_utf8_lossy(&warmup.stderr));
            results.push((project.display_name, None));
            continue;
        }
        println!("ok");

        let mut durations = Vec::with_capacity(ITERATIONS);
        for i in 0..ITERATIONS {
            let clean = Command::new("cargo")
                .args(["clean"])
                .current_dir(&base)
                .env("CARGO_TARGET_DIR", &target_dir)
                .output()
                .expect("failed to run cargo clean");

            if !clean.status.success() {
                eprintln!("  clean failed: {}", String::from_utf8_lossy(&clean.stderr));
                continue;
            }

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

            if !build.status.success() {
                eprintln!(
                    "  build {} failed: {}",
                    i + 1,
                    String::from_utf8_lossy(&build.stderr)
                );
                continue;
            }

            durations.push(elapsed);
            println!("  [{}/{}] {:.0?}", i + 1, ITERATIONS, elapsed);
        }

        results.push((project.display_name, Stats::compute(&durations)));
        println!();
    }

    // Print summary table
    println!("=== Summary ({mode}) ===\n");
    println!(
        "{:<16} {:>8} {:>8} {:>8} {:>8}  (ms)",
        "project", "min", "median", "mean", "max"
    );
    println!("{}", "-".repeat(60));

    for &(name, ref stats) in &results {
        let Some(stats) = stats else {
            println!("{:<16} FAILED", name);
            continue;
        };
        println!(
            "{:<16} {:>8} {:>8} {:>8} {:>8}",
            name, stats.min, stats.median, stats.mean, stats.max
        );
    }

    if !release {
        return;
    }

    // The first project is always the null baseline
    let [("null", Some(null_stats)), rest @ ..] = &results[..] else {
        panic!("Expected first null result with stats")
    };

    // Generate gnuplot data — iterate results in order, skip null
    let mut data = String::new();
    for (i, (name, stats)) in rest.iter().enumerate() {
        let Some(stats) = stats else {
            continue;
        };
        let delta = stats.median.saturating_sub(null_stats.median);
        writeln!(data, "\"{name}\" {delta} {i}").unwrap();
    }

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
    script.push_str(s3);

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

    let assets_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
    std::fs::create_dir_all(&assets_dir).ok();
    let svg_path = assets_dir.join("compile_bench.svg");
    std::fs::write(&svg_path, &svg).expect("Failed to write SVG");
    eprintln!("Wrote SVG to {}", svg_path.display());

    // Print markdown table for README
    eprintln!("\n--- README Markdown ---\n");
    eprintln!("![compile benchmark](assets/compile_bench.svg)\n");
    eprintln!("```");
    eprintln!("{:<16} {:>10} {:>12}", "", "median(ms)", "Δ null(ms)");
    for &(name, ref stats) in &results {
        let Some(stats) = stats else {
            eprintln!("{:<16} FAILED", name);
            continue;
        };
        if name == "null" {
            eprintln!("{:<16} {:>10}", name, stats.median);
        } else {
            let delta = stats.median.saturating_sub(null_stats.median);
            eprintln!("{:<16} {:>10} {:>+12}", name, stats.median, delta);
        }
    }
    eprintln!("```");
}
