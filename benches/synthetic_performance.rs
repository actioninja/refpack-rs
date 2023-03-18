////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::io::Cursor;
use std::{fs, iter};
use std::time::Duration;

use criterion_cycles_per_byte::CyclesPerByte;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkGroup, Criterion, Throughput, BenchmarkId, SamplingMode};
use rand::prelude::*;
use refpack::{compress, decompress, easy_compress, easy_decompress};
use refpack::format::Reference;

const CONST_BENCH_LENGTH: usize = 8096;

fn random_vec(len: usize) -> Vec<u8> {
    iter::repeat_with(random::<u8>).take(len).collect()
}

fn random_increasing_vecs(num: usize, increase_interval: usize) -> Vec<Vec<u8>> {
    let mut cur_size = increase_interval;
    iter::repeat_with(|| {
        let tmp = cur_size;
        cur_size += increase_interval;
        random_vec(tmp)
    })
        .take(num)
        .collect()
}

fn repeating_vec(num: usize) -> Vec<u8> {
    (0..=255).cycle().take(num).collect()
}

fn bench_set(group: &mut BenchmarkGroup<CyclesPerByte>, input_vec: &[u8]) {
    let size = input_vec.len();
    group.bench_with_input(BenchmarkId::new("easy_compress", size), &input_vec, |b, i| {
        b.iter(|| easy_compress::<Reference>(black_box(i)))
    });

    group.bench_with_input(BenchmarkId::new("compress", size), &input_vec, |b, i| {
        b.iter(|| {
            let mut in_buf = Cursor::new(i);
            let mut out_buf = Cursor::new(vec![]);
            compress::<Reference>(size, black_box(&mut in_buf), black_box(&mut out_buf))
        })
    });

    let compressed = easy_compress::<Reference>(input_vec).unwrap();

    group.bench_with_input(BenchmarkId::new("easy_decompress", size), &compressed, |b, i| {
        b.iter(|| black_box(easy_decompress::<Reference>(black_box(i))))
    });

    group.bench_with_input(BenchmarkId::new("decompress", size), &compressed, |b, i| {
        b.iter(|| {
            let mut in_buf = Cursor::new(i);
            let mut out_buf = Cursor::new(vec![]);
            decompress::<Reference>(black_box(&mut in_buf), black_box(&mut out_buf))
        })
    });

    group.bench_with_input(BenchmarkId::new("symmetrical easy", size), &input_vec, |b, i| {
        b.iter(|| {
            let compressed = easy_compress::<Reference>(i).unwrap();
            let decompressed = easy_decompress::<Reference>(black_box(&compressed)).unwrap();
            black_box(decompressed);
        })
    });

    group.bench_with_input(BenchmarkId::new("symmetrical", size), &input_vec, |b, i| {
        b.iter(|| {
            let mut in_buf = Cursor::new(i);
            let mut out_buf = Cursor::new(vec![]);
            let mut final_buf = Cursor::new(vec![]);

            let _ = compress::<Reference>(
                size,
                black_box(&mut in_buf),
                black_box(&mut out_buf),
            );
            out_buf.set_position(0);
            let _ = decompress::<Reference>(black_box(&mut out_buf), black_box(&mut final_buf));
        })
    });
}

fn random_data_bench(c: &mut Criterion<CyclesPerByte>) {
    let mut group = c.benchmark_group("Constant Length Random Input Data".to_string());

    group.throughput(Throughput::Bytes(CONST_BENCH_LENGTH as u64));

    let constant_input = random_vec(CONST_BENCH_LENGTH);

    bench_set(&mut group, &constant_input);

    group.finish();
}

fn random_increasing_data_sets_bench(c: &mut Criterion<CyclesPerByte>) {
    let mut group = c.benchmark_group("Random Input Data Increasing");

    for size in [
        CONST_BENCH_LENGTH,
        CONST_BENCH_LENGTH * 2,
        CONST_BENCH_LENGTH * 4,
        CONST_BENCH_LENGTH * 8,
        CONST_BENCH_LENGTH * 16,
        CONST_BENCH_LENGTH * 32,
    ] {
        group.throughput(Throughput::Bytes(size as u64));

        let random_input = random_vec(size);
        bench_set(&mut group, &random_input);
    }
    group.finish()
}

fn repeating_increasing_data_sets_bench(c: &mut Criterion<CyclesPerByte>) {
    let mut group = c.benchmark_group("Repeating Input Data Increasing");

    for size in [
        CONST_BENCH_LENGTH,
        CONST_BENCH_LENGTH * 2,
        CONST_BENCH_LENGTH * 4,
        CONST_BENCH_LENGTH * 8,
        CONST_BENCH_LENGTH * 16,
        CONST_BENCH_LENGTH * 32,
    ] {
        group.throughput(Throughput::Bytes(size as u64));

        let random_input = repeating_vec(size);
        bench_set(&mut group, &random_input);
    }
    group.finish()
}

fn files_bench(c: &mut Criterion<CyclesPerByte>) {
    let mut entries = fs::read_dir("benches/bench_files/").unwrap()
        .map(|res| res.unwrap().path())
        .collect::<Vec<_>>();

    entries.sort();

    for file in entries {
        let mut group = c.benchmark_group(format!("File {:?}", file.file_name().unwrap()));

        let input = fs::read(file).unwrap();

        group.throughput(Throughput::Bytes(input.len() as u64));
        group.sample_size(10);
        group.sampling_mode(SamplingMode::Flat);
        group.measurement_time(Duration::from_secs(10));

        bench_set(&mut group, &input);

        group.finish();
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default()
    .with_measurement(CyclesPerByte)
    .noise_threshold(0.02);
    targets = random_data_bench,
    random_increasing_data_sets_bench,
    repeating_increasing_data_sets_bench,
    files_bench
);
criterion_main!(benches);
