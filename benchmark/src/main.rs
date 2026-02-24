use jsony_bench::{Bencher, Stat};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::Stdio;

mod cargo;
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

fn bench_end2end_config_toml_edit<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let mut results = Vec::new();
    for (name, source) in configs {
        let stat = bench.func(|| {
            let mut result = source.parse::<toml_edit::DocumentMut>();
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || configs[rng.rand_range(0..configs.len() as u32) as usize].1,
        |source| {
            let mut result = source.parse::<toml_edit::DocumentMut>();
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

    fn plot_raw(&self, label: &str, data: &str, xrange_max: f64, height: u32) -> String {
        let mut render = String::new();
        let s1 = self
            .s1
            .replacen("size 780,400", &format!("size 780,{height}"), 1);
        render.push_str(&s1);
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

fn main_for_cargo() {
    let lock_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let versions: Vec<(&str, String)> = ["toml-spanner", "toml", "toml-span"]
        .into_iter()
        .map(|name| (name, lockfile_version(&lock_path, name)))
        .collect();

    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();

    println!("===== Cargo.lock: parse + deserialize =====");

    println!("toml-spanner:");
    let lock_spanner = bench.func(|| {
        let mut result = cargo::parse_lock_toml_spanner(static_input::ZED_CARGO_LOCK);
        std::hint::black_box(&mut result);
    });
    println!("  {lock_spanner}");

    println!("toml:");
    let lock_toml = bench.func(|| {
        let mut result = cargo::parse_lock_serde_toml(static_input::ZED_CARGO_LOCK);
        std::hint::black_box(&mut result);
    });
    println!("  {lock_toml}");

    println!("toml-span:");
    let lock_span = bench.func(|| {
        let mut result = cargo::parse_lock_toml_span(static_input::ZED_CARGO_LOCK);
        std::hint::black_box(&mut result);
    });
    println!("  {lock_span}");

    println!("\n===== Cargo.toml: parse + deserialize =====");

    println!("toml-spanner:");
    let manifest_spanner = bench.func(|| {
        let mut result = cargo::parse_manifest_toml_spanner(static_input::ZED_CARGO_TOML);
        std::hint::black_box(&mut result);
    });
    println!("  {manifest_spanner}");

    println!("toml:");
    let manifest_toml = bench.func(|| {
        let mut result = cargo::parse_manifest_serde_toml(static_input::ZED_CARGO_TOML);
        std::hint::black_box(&mut result);
    });
    println!("  {manifest_toml}");

    println!("\n=== Versions ===");
    for (name, version) in &versions {
        println!("  {name} {version}");
    }

    let lock_spanner_nanos = f64::from(lock_spanner.nanos);
    let lock_toml_nanos = f64::from(lock_toml.nanos);
    let lock_span_nanos = f64::from(lock_span.nanos);
    let manifest_spanner_nanos = f64::from(manifest_spanner.nanos);
    let manifest_toml_nanos = f64::from(manifest_toml.nanos);

    let rel_lock_toml = lock_toml_nanos / lock_spanner_nanos;
    let rel_lock_span = lock_span_nanos / lock_spanner_nanos;
    let rel_manifest_toml = manifest_toml_nanos / manifest_spanner_nanos;

    let mut data = String::new();
    writeln!(data, "\"{{/:Bold zed/Cargo.lock (272KB)}}\" 0 0").unwrap();
    writeln!(data, "\"toml-spanner\" {:.2} 1", 1.0).unwrap();
    writeln!(data, "\"toml\" {:.2} 2", rel_lock_toml).unwrap();
    writeln!(data, "\"toml-span\" {:.2} 3", rel_lock_span).unwrap();
    writeln!(data, "\"{{/:Bold zed/Cargo.toml (18KB)}}\" 0 0").unwrap();
    writeln!(data, "\"toml-spanner\" {:.2} 1", 1.0).unwrap();
    writeln!(data, "\"toml\" {:.2} 2", rel_manifest_toml).unwrap();

    let max_rel = [rel_lock_toml, rel_lock_span, rel_manifest_toml]
        .into_iter()
        .fold(0.0_f64, f64::max);

    let plotter = Plotter::new();
    let svg = plotter.plot_raw(
        "x times longer parse + deserialize time than toml-spanner (lower is better)",
        &data,
        max_rel + 1.5,
        280,
    );

    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
    std::fs::create_dir_all(&assets_dir).ok();
    let svg_path = assets_dir.join("bench_cargo.svg");
    std::fs::write(&svg_path, &svg).expect("Failed to write SVG");
    eprintln!("Wrote SVG to {}", svg_path.display());

    eprintln!("\n--- README Markdown ---\n");
    let version_line: Vec<_> = versions
        .iter()
        .map(|(name, v)| format!("{name} {v}"))
        .collect();
    eprintln!("Versions: {}\n", version_line.join(", "));
    eprintln!("![benchmark](assets/bench_cargo.svg)\n");
    eprintln!("```");
    eprintln!(
        "{:<16} {:>9} {:>10} {:>10} {:>10}",
        "", "time(μs)", "cycles(K)", "instr(K)", "branch(K)"
    );

    let sections: &[(&str, &[(&str, &Stat)])] = &[
        (
            "zed/Cargo.lock (parse + deserialize)",
            &[
                ("toml-spanner", &lock_spanner),
                ("toml", &lock_toml),
                ("toml-span", &lock_span),
            ],
        ),
        (
            "zed/Cargo.toml (parse + deserialize)",
            &[
                ("toml-spanner", &manifest_spanner),
                ("toml", &manifest_toml),
            ],
        ),
    ];
    for (section_name, stats) in sections {
        eprintln!("{section_name}");
        for (lib_name, stat) in *stats {
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

fn bench_clone_in<'a>(bench: &mut Bencher, configs: &[(&'a str, &str)]) -> Vec<(&'a str, Stat)> {
    // Pre-parse all configs into their own arenas.
    let arenas: Vec<_> = configs.iter().map(|_| toml_spanner::Arena::new()).collect();
    let parsed: Vec<_> = configs
        .iter()
        .zip(arenas.iter())
        .map(|((_, source), arena)| toml_spanner::parse(source, arena).unwrap())
        .collect();

    let mut results = Vec::new();
    for (i, (name, _)) in configs.iter().enumerate() {
        let table = parsed[i].table();
        let stat = bench.func(|| {
            let arena = toml_spanner::Arena::new();
            let mut cloned = table.clone_in(&arena);
            std::hint::black_box(&mut cloned);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }

    let tables: Vec<_> = parsed.iter().map(|r| r.table()).collect();
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || tables[rng.rand_range(0..tables.len() as u32) as usize],
        |table| {
            let arena = toml_spanner::Arena::new();
            let mut cloned = table.clone_in(&arena);
            std::hint::black_box(&mut cloned);
        },
    );
    println!("mixed: {stat}");
    results
}

fn main_for_clone() {
    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();

    let configs: &[(&str, &str)] = &[
        ("zed/Cargo.lock", static_input::ZED_CARGO_LOCK),
        ("zed/Cargo.toml", static_input::ZED_CARGO_TOML),
        ("extask.toml", static_input::EXTASK_TOML),
        ("devsm.toml", static_input::DEVSM_TOML),
        ("random", static_input::RANDOM_TOML),
    ];

    println!("===== parse (baseline) =====");
    let parse_stats = bench_end2end_config_toml_parser(&mut bench, configs);

    println!("\n===== clone_in =====");
    let clone_stats = bench_clone_in(&mut bench, configs);

    println!("\n=== clone_in vs parse ===");
    println!(
        "{:<20} {:>9} {:>9} {:>9} {:>9} {:>6}",
        "", "parse(μs)", "cycles(K)", "clone(μs)", "cycles(K)", "ratio"
    );
    for ((name, parse_stat), (_, clone_stat)) in parse_stats.iter().zip(clone_stats.iter()) {
        let pt = f64::from(parse_stat.nanos) / 1000.0;
        let pc = f64::from(parse_stat.cycles) / 1000.0;
        let ct = f64::from(clone_stat.nanos) / 1000.0;
        let cc = f64::from(clone_stat.cycles) / 1000.0;
        let ratio = ct / pt;
        println!(
            "{:<20} {:>9.1} {:>9.0} {:>9.1} {:>9.0} {:>5.1}%",
            name,
            pt,
            pc,
            ct,
            cc,
            ratio * 100.0
        );
    }
}

fn bench_emit_toml_spanner<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let arenas: Vec<_> = configs.iter().map(|_| toml_spanner::Arena::new()).collect();
    let mut tables: Vec<_> = configs
        .iter()
        .zip(arenas.iter())
        .map(|((_, source), arena)| toml_spanner::parse(source, arena).unwrap().into_table())
        .collect();
    for table in &mut tables {
        table.normalize();
    }

    let mut results = Vec::new();
    for (i, (name, _)) in configs.iter().enumerate() {
        let normalized = tables[i].try_as_normalized().unwrap();
        let stat = bench.func(|| {
            let mut buf = Vec::new();
            toml_spanner::emit(normalized, &mut buf);
            std::hint::black_box(&mut buf);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }

    let normalized: Vec<_> = tables
        .iter()
        .map(|r| r.try_as_normalized().unwrap())
        .collect();
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || normalized[rng.rand_range(0..normalized.len() as u32) as usize],
        |normalized| {
            let mut buf = Vec::new();
            toml_spanner::emit(normalized, &mut buf);
            std::hint::black_box(&mut buf);
        },
    );
    println!("mixed: {stat}");
    results
}

fn bench_emit_reprojected<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    // For each config: parse source, parse dest (identity), reproject, normalize.
    // Then benchmark just the emit_with_config call.
    struct Prepared<'de> {
        source: &'de str,
        items: Vec<&'de toml_spanner::Item<'de>>,
        normalized: &'de toml_spanner::NormalizedTable<'de>,
    }

    let src_arenas: Vec<_> = configs.iter().map(|_| toml_spanner::Arena::new()).collect();
    let src_roots: Vec<_> = configs
        .iter()
        .zip(src_arenas.iter())
        .map(|((_, source), arena)| toml_spanner::parse(source, arena).unwrap())
        .collect();

    let dest_arenas: Vec<_> = configs.iter().map(|_| toml_spanner::Arena::new()).collect();
    let mut dest_tables: Vec<_> = configs
        .iter()
        .zip(dest_arenas.iter())
        .map(|((_, source), arena)| toml_spanner::parse(source, arena).unwrap().into_table())
        .collect();

    let mut prepared: Vec<Prepared<'_>> = Vec::new();
    for (i, (_, source)) in configs.iter().enumerate() {
        let mut items = Vec::new();
        toml_spanner::reproject(&src_roots[i], &mut dest_tables[i], &mut items);
        let normalized = dest_tables[i].normalize();
        // SAFETY: normalized borrows dest_tables[i] which lives as long as dest_arenas.
        // We never move or mutate dest_tables again after this point.
        let normalized: &'static toml_spanner::NormalizedTable<'static> =
            unsafe { std::mem::transmute(normalized) };
        let items: Vec<&'static toml_spanner::Item<'static>> =
            unsafe { std::mem::transmute(items) };
        prepared.push(Prepared {
            source,
            items,
            normalized,
        });
    }

    let mut results = Vec::new();
    for (i, (name, _)) in configs.iter().enumerate() {
        let p = &prepared[i];
        let config = toml_spanner::EmitConfig {
            projected_source_text: p.source,
            projected_source_items: &p.items,
            reprojected_order: true,
        };
        let stat = bench.func(|| {
            let mut buf = Vec::new();
            toml_spanner::emit_with_config(p.normalized, &config, &mut buf);
            std::hint::black_box(&mut buf);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }

    let configs_for_gen: Vec<_> = prepared
        .iter()
        .map(|p| {
            (
                p.normalized,
                toml_spanner::EmitConfig {
                    projected_source_text: p.source,
                    projected_source_items: &p.items,
                    reprojected_order: true,
                },
            )
        })
        .collect();
    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || &configs_for_gen[rng.rand_range(0..configs_for_gen.len() as u32) as usize],
        |(normalized, config)| {
            let mut buf = Vec::new();
            toml_spanner::emit_with_config(normalized, config, &mut buf);
            std::hint::black_box(&mut buf);
        },
    );
    println!("mixed: {stat}");
    results
}

fn bench_emit_toml<'a>(bench: &mut Bencher, configs: &[(&'a str, &str)]) -> Vec<(&'a str, Stat)> {
    let parsed: Vec<_> = configs
        .iter()
        .map(|(_, source)| source.parse::<toml::Table>().unwrap())
        .collect();

    let mut results = Vec::new();
    for (i, (name, _)) in configs.iter().enumerate() {
        let table = &parsed[i];
        let stat = bench.func(|| {
            let mut result = toml::to_string(table);
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }

    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || &parsed[rng.rand_range(0..parsed.len() as u32) as usize],
        |table| {
            let mut result = toml::to_string(table);
            std::hint::black_box(&mut result);
        },
    );
    println!("mixed: {stat}");
    results
}

fn bench_emit_toml_edit<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let parsed: Vec<_> = configs
        .iter()
        .map(|(_, source)| source.parse::<toml_edit::DocumentMut>().unwrap())
        .collect();

    let mut results = Vec::new();
    for (i, (name, _)) in configs.iter().enumerate() {
        let doc = &parsed[i];
        let stat = bench.func(|| {
            let mut result = doc.to_string();
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }

    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || &parsed[rng.rand_range(0..parsed.len() as u32) as usize],
        |doc| {
            let mut result = doc.to_string();
            std::hint::black_box(&mut result);
        },
    );
    println!("mixed: {stat}");
    results
}

fn main_for_emit() {
    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();

    let configs: &[(&str, &str)] = &[
        ("zed/Cargo.lock", static_input::ZED_CARGO_LOCK),
        ("zed/Cargo.toml", static_input::ZED_CARGO_TOML),
        ("extask.toml", static_input::EXTASK_TOML),
        ("devsm.toml", static_input::DEVSM_TOML),
        ("random", static_input::RANDOM_TOML),
    ];

    println!("===== toml-spanner emit =====");
    let spanner_stats = bench_emit_toml_spanner(&mut bench, configs);

    println!("\n===== toml-spanner emit (reprojected) =====");
    let reproj_stats = bench_emit_reprojected(&mut bench, configs);

    println!("\n===== toml to_string =====");
    let toml_stats = bench_emit_toml(&mut bench, configs);

    println!("\n===== toml_edit to_string =====");
    let toml_edit_stats = bench_emit_toml_edit(&mut bench, configs);

    println!("\n=== emit comparison ===");
    println!(
        "{:<20} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>11} {:>9}",
        "",
        "emit(μs)",
        "cyc(K)",
        "reproj(μs)",
        "cyc(K)",
        "toml(μs)",
        "cyc(K)",
        "toml_ed(μs)",
        "cyc(K)"
    );
    for i in 0..spanner_stats.len() {
        let (name, ref ss) = spanner_stats[i];
        let (_, ref rs) = reproj_stats[i];
        let (_, ref ts) = toml_stats[i];
        let (_, ref es) = toml_edit_stats[i];
        let st = f64::from(ss.nanos) / 1000.0;
        let sc = f64::from(ss.cycles) / 1000.0;
        let rt = f64::from(rs.nanos) / 1000.0;
        let rc = f64::from(rs.cycles) / 1000.0;
        let tt = f64::from(ts.nanos) / 1000.0;
        let tc = f64::from(ts.cycles) / 1000.0;
        let et = f64::from(es.nanos) / 1000.0;
        let ec = f64::from(es.cycles) / 1000.0;
        println!(
            "{:<20} {:>9.1} {:>9.0} {:>9.1} {:>9.0} {:>9.1} {:>9.0} {:>11.1} {:>9.0}",
            name, st, sc, rt, rc, tt, tc, et, ec
        );
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
    if std::env::args().any(|a| a == "cargo") {
        main_for_cargo();
        return;
    }
    if std::env::args().any(|a| a == "clone") {
        main_for_clone();
        return;
    }
    if std::env::args().any(|a| a == "emit") {
        main_for_emit();
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
    let versions: Vec<(&str, String)> = ["toml-spanner", "toml", "toml_edit", "toml-span"]
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

    println!("===== toml_edit =======");
    let toml_edit_stats = bench_end2end_config_toml_edit(&mut bench, configs);

    println!("===== toml_span =======");
    let span_stats = bench_end2end_config_toml_span(&mut bench, configs);
    println!("=== Versions ===");
    for (name, version) in &versions {
        println!("  {name} {version}");
    }
    println!();

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
        let toml_edit_nanos = f64::from(
            toml_edit_stats
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
        let rel_toml_edit = toml_edit_nanos / spanner_nanos;
        let rel_span = span_nanos / spanner_nanos;

        // Header line:
        writeln!(data, "\"{{/:Bold {} ({})}}\" 0 0", bench_name, size).unwrap();

        rel_values.push(rel_toml);
        rel_values.push(rel_toml_edit);
        rel_values.push(rel_span);

        writeln!(data, "\"toml-spanner\" {:.2} 1", 1.0).unwrap();
        writeln!(data, "\"toml\" {:.2} 2", rel_toml).unwrap();
        writeln!(data, "\"toml_edit\" {:.2} 3", rel_toml_edit).unwrap();
        writeln!(data, "\"toml-span\" {:.2} 4", rel_span).unwrap();
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
        400,
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
        ("toml_edit", &toml_edit_stats),
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
