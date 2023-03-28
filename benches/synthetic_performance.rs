////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::hint::black_box;
use std::io::Cursor;
use std::iter;

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion, Throughput};
use rand::prelude::*;
use refpack::format::TheSims12;
use refpack::{compress, decompress, easy_compress, easy_decompress};

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

fn bench_set(group: &mut BenchmarkGroup<WallTime>, input_vec: &[u8]) {
    let size = input_vec.len();
    group.bench_with_input(format!("easy_compress ({size})"), &input_vec, |b, i| {
        b.iter(|| easy_compress::<TheSims12>(black_box(i)))
    });

    group.bench_with_input(format!("compress ({size})"), &input_vec, |b, i| {
        b.iter(|| {
            let mut in_buf = Cursor::new(i);
            let mut out_buf = Cursor::new(vec![]);
            compress::<TheSims12>(size, black_box(&mut in_buf), black_box(&mut out_buf))
        })
    });

    group.bench_with_input(format!("symmetrical easy ({size})"), &input_vec, |b, i| {
        b.iter(|| {
            let compressed = easy_compress::<TheSims12>(i).unwrap();
            let decompressed = easy_decompress::<TheSims12>(black_box(&compressed)).unwrap();
            black_box(decompressed);
        })
    });

    group.bench_with_input(format!("symmetrical ({size})"), &input_vec, |b, i| {
        b.iter(|| {
            let mut in_buf = Cursor::new(i);
            let mut out_buf = Cursor::new(vec![]);
            let mut final_buf = Cursor::new(vec![]);

            let _ = compress::<TheSims12>(
                CONST_BENCH_LENGTH,
                black_box(&mut in_buf),
                black_box(&mut out_buf),
            );
            let _ = decompress::<TheSims12>(black_box(&mut out_buf), black_box(&mut final_buf));
        })
    });
}

fn random_data_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Constant Length Random Input Data".to_string());

    let constant_input = random_vec(CONST_BENCH_LENGTH);

    bench_set(&mut group, &constant_input);

    group.finish();
}

fn random_increasing_data_sets_bench(c: &mut Criterion) {
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

criterion_group!(
    benches,
    random_data_bench,
    random_increasing_data_sets_bench
);
criterion_main!(benches);
