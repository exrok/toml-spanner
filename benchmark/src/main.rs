use jsony_bench::{Bencher, Stat};
use std::fmt::Write as _;
use std::io::Write as _;
use std::process::Stdio;

mod compile_bench;
mod compile_examples;
mod static_input;

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

    fn plot_raw(&self, label: &str, data: &str) -> String {
        let mut render = String::new();
        render.push_str(self.s1);
        write!(render, "{label:?}").unwrap();
        render.push_str(self.s2);
        render.push_str(data);
        render.push_str(self.s3);

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
        let release = std::env::args().any(|a| a == "--release");
        compile_bench::run_compile_bench(release);
        return;
    }

    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();

    let configs: &[(&str, &str)] = &[
        ("zed", static_input::ZED_CARGO_TOML),
        ("extask", static_input::EXTASK_TOML),
        ("devsm", static_input::DEVSM_TOML),
        ("random", static_input::RANDOM_TOML),
    ];

    println!("===== toml_spanner =======");
    let spanner_stats = bench_end2end_config_toml_parser(&mut bench, configs);

    println!("===== toml =======");
    let toml_stats = bench_end2end_config_toml(&mut bench, configs);

    println!("===== toml_span =======");
    let span_stats = bench_end2end_config_toml_span(&mut bench, configs);

    // --- Graph generation ---
    let graph_benchmarks = [("zed", "18KB"), ("extask", "4KB"), ("devsm", "2KB")];

    let mut data = String::new();
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

        writeln!(data, "\"── {} ({}) ──\" 0 0", bench_name, size).unwrap();
        writeln!(data, "\"toml-spanner\" {:.2} 1", 1.0).unwrap();
        writeln!(data, "\"toml\" {:.2} 2", rel_toml).unwrap();
        writeln!(data, "\"toml-span\" {:.2} 3", rel_span).unwrap();
    }

    let plotter = Plotter::new();
    let svg = plotter.plot_raw("Relative Parse Time (lower is better)", &data);

    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
    std::fs::create_dir_all(&assets_dir).ok();
    let svg_path = assets_dir.join("bench.svg");
    std::fs::write(&svg_path, &svg).expect("Failed to write SVG");
    eprintln!("Wrote SVG to {}", svg_path.display());

    // Print markdown table for README
    eprintln!("\n--- README Markdown ---\n");
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
