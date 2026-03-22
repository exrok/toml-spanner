use jsony_bench::{Bencher, Stat};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::Stdio;

mod cargo;
mod compile_bench;
mod compile_examples;
mod static_input;

const DEFAULT_CONFIGS: &[(&str, &str)] = &[
    ("zed/Cargo.lock", static_input::ZED_CARGO_LOCK),
    ("zed/Cargo.toml", static_input::ZED_CARGO_TOML),
    ("extask.toml", static_input::EXTASK_TOML),
    ("devsm.toml", static_input::DEVSM_TOML),
    ("random", static_input::RANDOM_TOML),
];

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
    // Mixed version avoids overly optimized branch predictions per config.
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

fn print_stat_table(sections: &[(&str, &[(&str, &Stat)])]) {
    eprintln!(
        "{:<16} {:>9} {:>10} {:>10} {:>10}",
        "", "time(μs)", "cycles(K)", "instr(K)", "branch(K)"
    );
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

    /// Compute ratios relative to the first library in each group, generate
    /// gnuplot data, render SVG, and write it to `svg_path`.
    fn plot_relative(
        &self,
        label: &str,
        height: u32,
        svg_path: &Path,
        // Each group: (header_label, &[(lib_name, &Stat)])
        // First entry per group is the baseline (ratio = 1.0).
        groups: &[(&str, &[(&str, &Stat)])],
    ) {
        let mut data = String::new();
        let mut max_rel: f64 = 0.0;
        for (header, stats) in groups {
            writeln!(data, "\"{{/:Bold {header}}}\" 0 0").unwrap();
            let baseline = f64::from(stats[0].1.nanos);
            for (idx, (lib_name, stat)) in stats.iter().enumerate() {
                let rel = f64::from(stat.nanos) / baseline;
                if rel > max_rel {
                    max_rel = rel;
                }
                let escaped = lib_name.replace('_', "\\\\_");
                writeln!(data, "\"{escaped}\" {rel:.2} {}", idx + 1).unwrap();
            }
        }

        let svg = self.plot_raw(label, &data, max_rel + 1.5, height);
        if let Some(parent) = svg_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(svg_path, &svg).expect("Failed to write SVG");
        eprintln!("Wrote SVG to {}", svg_path.display());
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

    let lock_libs: &[(&str, fn(&str))] = &[
        ("toml-spanner", |s| {
            let _ = std::hint::black_box(cargo::parse_lock_toml_spanner(s));
        }),
        ("toml", |s| {
            let _ = std::hint::black_box(cargo::parse_lock_serde_toml(s));
        }),
        ("toml-span", |s| {
            let _ = std::hint::black_box(cargo::parse_lock_toml_span(s));
        }),
    ];
    let mut lock_stats = Vec::new();
    for (name, f) in lock_libs {
        println!("{name}:");
        let stat = bench.func(|| f(static_input::ZED_CARGO_LOCK));
        println!("  {stat}");
        lock_stats.push((*name, stat));
    }

    println!("\n===== Cargo.toml: parse + deserialize =====");

    let manifest_libs: &[(&str, fn(&str))] = &[
        ("toml-spanner", |s| {
            let _ = std::hint::black_box(cargo::parse_manifest_toml_spanner(s));
        }),
        ("toml", |s| {
            let _ = std::hint::black_box(cargo::parse_manifest_serde_toml(s));
        }),
    ];
    let mut manifest_stats = Vec::new();
    for (name, f) in manifest_libs {
        println!("{name}:");
        let stat = bench.func(|| f(static_input::ZED_CARGO_TOML));
        println!("  {stat}");
        manifest_stats.push((*name, stat));
    }

    println!("\n=== Versions ===");
    for (name, version) in &versions {
        println!("  {name} {version}");
    }

    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
    let plotter = Plotter::new();

    let lock_group: Vec<(&str, &Stat)> = lock_stats.iter().map(|(n, s)| (*n, s)).collect();
    let manifest_group: Vec<(&str, &Stat)> = manifest_stats.iter().map(|(n, s)| (*n, s)).collect();

    plotter.plot_relative(
        "x times longer parse + deserialize time than toml-spanner (lower is better)",
        280,
        &assets_dir.join("bench_cargo.svg"),
        &[
            ("zed/Cargo.lock (272KB)", &lock_group),
            ("zed/Cargo.toml (18KB)", &manifest_group),
        ],
    );

    eprintln!("\n--- README Markdown ---\n");
    let version_line: Vec<_> = versions
        .iter()
        .map(|(name, v)| format!("{name} {v}"))
        .collect();
    eprintln!("Versions: {}\n", version_line.join(", "));
    eprintln!("![benchmark](assets/bench_cargo.svg)\n");
    eprintln!("```");
    print_stat_table(&[
        ("zed/Cargo.lock (parse + deserialize)", &lock_group),
        ("zed/Cargo.toml (parse + deserialize)", &manifest_group),
    ]);
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

    println!("===== parse (baseline) =====");
    let parse_stats = bench_end2end_config_toml_parser(&mut bench, DEFAULT_CONFIGS);

    println!("\n===== clone_in =====");
    let clone_stats = bench_clone_in(&mut bench, DEFAULT_CONFIGS);

    println!("\n=== clone_in vs parse ===");
    println!(
        "{:<20} {:>9} {:>9} {:>9} {:>9} {:>6}",
        "",
        "parse(μs)",
        "cycles(K)",
        "clone(μs)",
        "cycles(K)",
        "ratio",
    );
    for i in 0..parse_stats.len() {
        let (name, ref ps) = parse_stats[i];
        let (_, ref cs) = clone_stats[i];
        let pt = f64::from(ps.nanos) / 1000.0;
        let pc = f64::from(ps.cycles) / 1000.0;
        let ct = f64::from(cs.nanos) / 1000.0;
        let cc = f64::from(cs.cycles) / 1000.0;
        let clone_ratio = ct / pt;
        println!(
            "{:<20} {:>9.1} {:>9.0} {:>9.1} {:>9.0} {:>5.1}%",
            name,
            pt,
            pc,
            ct,
            cc,
            clone_ratio * 100.0,
        );
    }
}

fn bench_emit_toml_spanner<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let arenas: Vec<_> = configs.iter().map(|_| toml_spanner::Arena::new()).collect();
    let tables: Vec<_> = configs
        .iter()
        .zip(arenas.iter())
        .map(|((_, source), arena)| toml_spanner::parse(source, arena).unwrap().into_table())
        .collect();

    let formatting = toml_spanner::Formatting::default();
    let mut results = Vec::new();
    for (i, (name, _)) in configs.iter().enumerate() {
        let table = &tables[i];
        let arena = &arenas[i];
        let stat = bench.func(|| {
            let scratch = toml_spanner::Arena::new();
            let buf = formatting.format_table_to_bytes(table.clone_in(arena), &scratch);
            std::hint::black_box(buf);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }

    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || {
            let idx = rng.rand_range(0..tables.len() as u32) as usize;
            (&tables[idx], &arenas[idx])
        },
        |(table, arena)| {
            let scratch = toml_spanner::Arena::new();
            let buf = formatting.format_table_to_bytes(table.clone_in(arena), &scratch);
            std::hint::black_box(buf);
        },
    );
    println!("mixed: {stat}");
    results
}

fn bench_emit_reprojected<'a>(
    bench: &mut Bencher,
    configs: &[(&'a str, &str)],
) -> Vec<(&'a str, Stat)> {
    let src_arenas: Vec<_> = configs.iter().map(|_| toml_spanner::Arena::new()).collect();
    let src_roots: Vec<_> = configs
        .iter()
        .zip(src_arenas.iter())
        .map(|((_, source), arena)| toml_spanner::parse(source, arena).unwrap())
        .collect();

    let mut results = Vec::new();
    for (i, (name, _)) in configs.iter().enumerate() {
        let src_root = &src_roots[i];
        let formatting = toml_spanner::Formatting::preserved_from(src_root);
        let stat = bench.func(|| {
            let arena = toml_spanner::Arena::new();
            let dest_table = src_root.table().clone_in(&arena);
            let buf = formatting.format_table_to_bytes(dest_table, &arena);
            std::hint::black_box(buf);
        });
        println!("{name}: {stat}");
        results.push((*name, stat));
    }

    let mut rng = oorandom::Rand32::new(0xdeadbeaf);
    let stat = bench.bench_with_generator(
        || {
            let idx = rng.rand_range(0..src_roots.len() as u32) as usize;
            &src_roots[idx]
        },
        |src_root| {
            let formatting = toml_spanner::Formatting::preserved_from(src_root);
            let arena = toml_spanner::Arena::new();
            let dest_table = src_root.table().clone_in(&arena);
            let buf = formatting.format_table_to_bytes(dest_table, &arena);
            std::hint::black_box(buf);
        },
    );
    println!("mixed: {stat}");
    results
}

fn main_for_emit() {
    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();

    println!("===== toml-spanner emit =====");
    let spanner_stats = bench_emit_toml_spanner(&mut bench, DEFAULT_CONFIGS);

    println!("\n===== toml-spanner emit (reprojected) =====");
    let reproj_stats = bench_emit_reprojected(&mut bench, DEFAULT_CONFIGS);

    println!("\n===== toml to_string =====");
    let toml_stats = bench_emit_toml(&mut bench, DEFAULT_CONFIGS);

    println!("\n===== toml_edit to_string =====");
    let toml_edit_stats = bench_emit_toml_edit(&mut bench, DEFAULT_CONFIGS);

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);


    if has("profile") {
        main_for_profile();
        return;
    }
    if has("cargo") {
        main_for_cargo();
        return;
    }
    if has("clone") {
        main_for_clone();
        return;
    }
    if has("emit") {
        main_for_emit();
        return;
    }
    if has("compile") {
        compile_bench::run_compile_bench(!has("--dev"), has("--all"), has("--report"));
        return;
    }

    let lock_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let versions: Vec<(&str, String)> = ["toml-spanner", "toml", "toml_edit", "toml-span"]
        .into_iter()
        .map(|name| (name, lockfile_version(&lock_path, name)))
        .collect();

    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();

    println!("===== toml_spanner =======");
    let spanner_stats = bench_end2end_config_toml_parser(&mut bench, DEFAULT_CONFIGS);

    println!("===== toml =======");
    let toml_stats = bench_end2end_config_toml(&mut bench, DEFAULT_CONFIGS);

    println!("===== toml_edit =======");
    let toml_edit_stats = bench_end2end_config_toml_edit(&mut bench, DEFAULT_CONFIGS);

    println!("===== toml_span =======");
    let span_stats = bench_end2end_config_toml_span(&mut bench, DEFAULT_CONFIGS);

    println!("=== Versions ===");
    for (name, version) in &versions {
        println!("  {name} {version}");
    }
    println!();

    let graph_benchmarks: &[(&str, &str)] = &[
        //        ("zed/Cargo.lock", "272KB"),
        ("zed/Cargo.toml", "18KB"),
        ("extask.toml", "4KB"),
        ("devsm.toml", "2KB"),
    ];

    let all_lib_stats: &[(&str, &[(&str, Stat)])] = &[
        ("toml-spanner", &spanner_stats),
        ("toml", &toml_stats),
        ("toml_edit", &toml_edit_stats),
        ("toml-span", &span_stats),
    ];

    // Build plot groups: one group per benchmark input, entries are libraries.
    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
    let plotter = Plotter::new();

    let mut groups: Vec<(&str, Vec<(&str, &Stat)>)> = Vec::new();
    for (bench_name, size) in graph_benchmarks {
        let header = format!("{bench_name} ({size})");
        // Leak the header string so it lives long enough — this is a one-shot CLI.
        let header: &str = Box::leak(header.into_boxed_str());
        let mut entries = Vec::new();
        for (lib_name, stats) in all_lib_stats {
            let stat = &stats.iter().find(|(n, _)| n == bench_name).unwrap().1;
            entries.push((*lib_name, stat));
        }
        groups.push((header, entries));
    }
    let group_refs: Vec<(&str, &[(&str, &Stat)])> =
        groups.iter().map(|(h, e)| (*h, e.as_slice())).collect();
    plotter.plot_relative(
        "x times longer parse time than toml-spanner (lower is better)",
        400,
        &assets_dir.join("bench.svg"),
        &group_refs,
    );

    // Print markdown table for README
    eprintln!("\n--- README Markdown ---\n");
    let version_line: Vec<_> = versions
        .iter()
        .map(|(name, v)| format!("{name} {v}"))
        .collect();
    eprintln!("Versions: {}\n", version_line.join(", "));
    eprintln!("![benchmark](assets/bench.svg)\n");
    eprintln!("```");

    let graph_benchmarks_full: &[(&str, &str)] = &[
        ("zed/Cargo.lock", "272KB"),
        ("zed/Cargo.toml", "18KB"),
        ("extask.toml", "4KB"),
        ("devsm.toml", "2KB"),
    ];
    let mut table_sections: Vec<(&str, Vec<(&str, &Stat)>)> = Vec::new();
    for (bench_name, _) in graph_benchmarks_full {
        let mut entries = Vec::new();
        for (lib_name, stats) in all_lib_stats {
            let stat = &stats.iter().find(|(n, _)| n == bench_name).unwrap().1;
            entries.push((*lib_name, stat));
        }
        table_sections.push((bench_name, entries));
    }
    let section_refs: Vec<(&str, &[(&str, &Stat)])> = table_sections
        .iter()
        .map(|(h, e)| (*h, e.as_slice()))
        .collect();
    print_stat_table(&section_refs);
    eprintln!("```");
}
