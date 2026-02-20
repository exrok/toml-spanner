use jsony_bench::{Bencher, Stat};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::Stdio;

mod compile_bench;
mod compile_examples;
mod static_input;

pub fn try_lockfile_version(path: &Path, package: &str) -> Option<String> {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let arena = toml_spanner::Arena::new();
    let table = toml_spanner::parse(&content, &arena)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    let packages = table["package"].as_array()?;
    for pkg in packages {
        if pkg["name"].as_str() == Some(package) {
            return Some(pkg["version"].as_str()?.into());
        }
    }
    panic!("package {package} not found in {}", path.display());
}

pub fn lockfile_version(path: &Path, package: &str) -> String {
    try_lockfile_version(path, package)
        .unwrap_or_else(|| panic!("package {package} not found in {}", path.display()))
}

fn bench_end2end_config_toml_span<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let mut results = Vec::new();
    for (name, source) in configs {
        let stat = bench.func(|| {
            let mut result = toml_span::parse(source);
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }
    // Mixed version avoids overly optimization branch predictions per configs.
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || configs[rng.rand_range(0..configs.len() as u32) as usize].1,
        |source| {
            let mut result = toml_span::parse(source);
            std::hint::black_box(&mut result);
        },
    );
    println!("mixed: {stat}");
    results
}

fn bench_end2end_config_toml_parser<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let mut results = Vec::new();
    for (name, source) in configs {
        let stat = bench.func(|| {
            let arena = toml_spanner::Arena::new();
            let mut result = toml_spanner::parse(source, &arena);
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }
    // Mixed version avoids overly optimization branch predictions per configs.
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || configs[rng.rand_range(0..configs.len() as u32) as usize].1,
        |source| {
            let arena = toml_spanner::Arena::new();
            let mut result = toml_spanner::parse(source, &arena);
            std::hint::black_box(&mut result);
        },
    );
    println!("mixed: {stat}");
    results
}

fn bench_end2end_config_toml<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let mut results = Vec::new();
    for (name, source) in configs {
        let stat = bench.func(|| {
            let mut result = source.parse::<toml::Table>();
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }
    // Mixed version avoids overly optimization branch predictions per configs.
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || configs[rng.rand_range(0..configs.len() as u32) as usize].1,
        |source| {
            let mut result = source.parse::<toml::Table>();
            std::hint::black_box(&mut result);
        },
    );
    println!("mixed: {stat}");
    results
}

struct Plotter {
    s1: &'static str,
    s2: &'static str,
    s3: &'static str,
}

impl Plotter {
    fn new() -> Plotter {
        let fs = include_str!("../plot.gnuplot");
        let (s1, rest) = fs
            .split_once("__INSERT_LABEL_HERE__")
            .expect("Missing __INSERT_LABEL_HERE__");
        let (s2, s3) = rest
            .split_once("__INSERT_DATA_HERE__")
            .expect("Missing __INSERT_DATA_HERE__");
        Plotter { s1, s2, s3 }
    }

    fn plot_raw(&self, label: &str, data: &str, xrange_max: f64) -> String {
        let mut render = String::new();
        render.push_str(self.s1);
        write!(render, "{label:?}").unwrap();
        render.push_str(self.s2);
        render.push_str(data);
        let s3 = self.s3.replacen(
            "plot ",
            &format!("set xrange [0:{:.1}]\nplot ", xrange_max),
            1,
        );
        render.push_str(&s3);

        let mut gnuplot = std::process::Command::new("gnuplot")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to spawn gnuplot — is gnuplot installed?");
        gnuplot
            .stdin
            .take()
            .unwrap()
            .write_all(render.as_bytes())
            .unwrap();
        let output = gnuplot.wait_with_output().unwrap();
        String::from_utf8(output.stdout).unwrap()
    }
}

fn main_for_profile() {
    let inputs = &[
        static_input::ZED_CARGO_TOML,
        static_input::EXTASK_TOML,
        static_input::DEVSM_TOML,
    ];
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    for _ in 0..1000000 {
        let stat = inputs[rng.rand_range(0..inputs.len() as u32) as usize];
        let arena = toml_spanner::Arena::new();
        let mut result = toml_spanner::parse(stat, &arena);
        std::hint::black_box(&mut result);
    }
}

fn main() {
    if std::env::args().any(|a| a == "profile") {
        main_for_profile();
        return;
    }
    if std::env::args().any(|a| a == "compile") {
        let all = std::env::args().any(|a| a == "--all");
        let dev = std::env::args().any(|a| a == "--dev");
        let report = std::env::args().any(|a| a == "--report");
        compile_bench::run_compile_bench(!dev, all, report);
        return;
    }

    let lock_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let versions: Vec<(&str, String)> = ["toml-spanner", "toml", "toml-span"]
        .into_iter()
        .map(|name| (name, lockfile_version(&lock_path, name)))
        .collect();

    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();

    let configs: &[(&str, &str)] = &[
        ("zed/Cargo.lock", static_input::ZED_CARGO_LOCK),
        ("zed/Cargo.toml", static_input::ZED_CARGO_TOML),
        ("extask.toml", static_input::EXTASK_TOML),
        ("devsm.toml", static_input::DEVSM_TOML),
        ("random", static_input::RANDOM_TOML),
    ];

    println!("===== toml_spanner =======");
    let spanner_stats = bench_end2end_config_toml_parser(&mut bench, configs);

    println!("===== toml =======");
    let toml_stats = bench_end2end_config_toml(&mut bench, configs);

    println!("===== toml_span =======");
    let span_stats = bench_end2end_config_toml_span(&mut bench, configs);
    println!("=== Versions ===");
    for (name, version) in &versions {
        println!("  {name} {version}");
    }
    println!();

    // --- Graph generation ---
    let graph_benchmarks = [
        //        ("zed/Cargo.lock", "272KB"),
        ("zed/Cargo.toml", "18KB"),
        ("extask.toml", "4KB"),
        ("devsm.toml", "2KB"),
    ];

    let mut data = String::new();
    let mut rel_values = Vec::new();
    for (bench_name, size) in &graph_benchmarks {
        let spanner_nanos = f64::from(
            spanner_stats
                .iter()
                .find(|(n, _)| *n == *bench_name)
                .unwrap()
                .1
                .nanos,
        );
        let toml_nanos = f64::from(
            toml_stats
                .iter()
                .find(|(n, _)| *n == *bench_name)
                .unwrap()
                .1
                .nanos,
        );
        let span_nanos = f64::from(
            span_stats
                .iter()
                .find(|(n, _)| *n == *bench_name)
                .unwrap()
                .1
                .nanos,
        );

        let rel_toml = toml_nanos / spanner_nanos;
        let rel_span = span_nanos / spanner_nanos;

        // Header line:
        writeln!(data, "\"{{/:Bold {} ({})}}\" 0 0", bench_name, size).unwrap();

        rel_values.push(rel_toml);
        rel_values.push(rel_span);

        writeln!(data, "\"toml-spanner\" {:.2} 1", 1.0).unwrap();
        writeln!(data, "\"toml\" {:.2} 2", rel_toml).unwrap();
        writeln!(data, "\"toml-span\" {:.2} 3", rel_span).unwrap();
    }

    let graph_benchmarks = [
        ("zed/Cargo.lock", "272KB"),
        ("zed/Cargo.toml", "18KB"),
        ("extask.toml", "4KB"),
        ("devsm.toml", "2KB"),
    ];

    let max_rel = rel_values.iter().copied().fold(0.0_f64, f64::max);

    let plotter = Plotter::new();
    let svg = plotter.plot_raw(
        "x times longer parse time than toml-spanner (lower is better)",
        &data,
        max_rel + 1.5,
    );

    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
    std::fs::create_dir_all(&assets_dir).ok();
    let svg_path = assets_dir.join("bench.svg");
    std::fs::write(&svg_path, &svg).expect("Failed to write SVG");
    eprintln!("Wrote SVG to {}", svg_path.display());

    // Print markdown table for README
    eprintln!("\n--- README Markdown ---\n");
    let version_line: Vec<_> = versions
        .iter()
        .map(|(name, v)| format!("{name} {v}"))
        .collect();
    eprintln!("Versions: {}\n", version_line.join(", "));
    eprintln!("![benchmark](assets/bench.svg)\n");
    eprintln!("```");
    eprintln!(
        "{:<16} {:>9} {:>10} {:>10} {:>10}",
        "", "time(μs)", "cycles(K)", "instr(K)", "branch(K)"
    );
    let all_stats: &[(&str, &[(&str, Stat)])] = &[
        ("toml-spanner", &spanner_stats),
        ("toml", &toml_stats),
        ("toml-span", &span_stats),
    ];
    for (bench_name, _) in &graph_benchmarks {
        eprintln!("{bench_name}");
        for (lib_name, stats) in all_stats {
            let stat = &stats.iter().find(|(n, _)| n == bench_name).unwrap().1;
            let t = f64::from(stat.nanos) / 1000.0;
            let c = f64::from(stat.cycles) / 1000.0;
            let i = f64::from(stat.inst) / 1000.0;
            let b = f64::from(stat.branch) / 1000.0;
            eprintln!(
                "  {:<14} {:>9.1} {:>10.0} {:>10.0} {:>10.0}",
                lib_name, t, c, i, b
            );
        }
    }
    eprintln!("```");
}
