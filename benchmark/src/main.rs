use jsony_bench::Bencher;

mod static_input;

fn bench_end2end_config_toml_span(bench: &mut Bencher, configs: &[(&str, &str)]) {
    for (name, source) in configs {
        let stat = bench.func(|| {
            let mut result = toml_span::parse(source);
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
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
}

fn bench_end2end_config_toml_parser(bench: &mut Bencher, configs: &[(&str, &str)]) {
    for (name, source) in configs {
        let stat = bench.func(|| {
            let arena = toml_spanner::Arena::new();
            let mut result = toml_spanner::parse(source, &arena);
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
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
}

fn bench_end2end_config_toml(bench: &mut Bencher, configs: &[(&str, &str)]) {
    for (name, source) in configs {
        let stat = bench.func(|| {
            let mut result = source.parse::<toml::Table>();
            std::hint::black_box(&mut result);
        });
        println!("{name}: {stat}");
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
}

// fn main_for_profile() {
//     let inputs = &[
//         static_input::ZED_CARGO_TOML,
//         static_input::EXTASK_TOML,
//         static_input::DEVSM_TOML,
//     ];
//     let mut rng = oorandom::Rand32::new(0xdeadbeaf);
//     for _ in 0..1000000 {
//         let stat = inputs[rng.rand_range(0..inputs.len() as u32) as usize];
//         let mut result = toml_spanner::parse(stat);
//         std::hint::black_box(&mut result);
//     }
// }
fn main() {
    let mut bench = jsony_bench::Bencher::new();
    bench.calibrate();
    println!("===== toml_spanner =======");
    bench_end2end_config_toml_parser(
        &mut bench,
        &[
            ("zed", static_input::ZED_CARGO_TOML),
            ("extask", static_input::EXTASK_TOML),
            ("devsm", static_input::DEVSM_TOML),
            ("random", static_input::RANDOM_TOML),
        ],
    );
    println!("===== toml =======");
    bench_end2end_config_toml(
        &mut bench,
        &[
            ("zed", static_input::ZED_CARGO_TOML),
            ("extask", static_input::EXTASK_TOML),
            ("devsm", static_input::DEVSM_TOML),
            ("random", static_input::RANDOM_TOML),
        ],
    );
    println!("===== toml_span =======");
    bench_end2end_config_toml_span(
        &mut bench,
        &[
            ("zed", static_input::ZED_CARGO_TOML),
            ("extask", static_input::EXTASK_TOML),
            ("devsm", static_input::DEVSM_TOML),
            ("random", static_input::RANDOM_TOML),
        ],
    );
}
