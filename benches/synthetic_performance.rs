////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::hint::black_box;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::time::Duration;
use std::{fs, io, iter};

use criterion::measurement::WallTime;
use criterion::{
    criterion_group,
    criterion_main,
    BenchmarkGroup,
    BenchmarkId,
    Criterion,
    SamplingMode,
    Throughput,
};
use rand::prelude::*;
use refpack::data::compression::CompressionOptions;
use refpack::format::Reference;
use refpack::{compress, decompress, easy_compress, easy_decompress};

const CONST_BENCH_LENGTHS: [usize; 7] =
    [1 << 6, 1 << 8, 1 << 10, 1 << 12, 1 << 14, 1 << 16, 1 << 18];

fn random_vec(len: usize) -> Vec<u8> {
    iter::repeat_with(random::<u8>).take(len).collect()
}

fn random_bool_vec(len: usize) -> Vec<u8> {
    iter::repeat_with(random::<bool>)
        .map(|b| b.into())
        .take(len)
        .collect()
}

fn repeating_vec(num: usize) -> Vec<u8> {
    (0..=255).cycle().take(num).collect()
}

fn zeros_vec(num: usize) -> Vec<u8> {
    vec![0; num]
}

fn bench_set(group: &mut BenchmarkGroup<WallTime>, input_vec: &[u8]) {
    let size = input_vec.len();

    for compression_options in [
        CompressionOptions::Fastest,
        CompressionOptions::Fast,
        CompressionOptions::Optimal,
    ] {
        group.bench_with_input(
            BenchmarkId::new(format!("easy_compress {:?}", compression_options), size),
            &input_vec,
            |b, i| b.iter(|| easy_compress::<Reference>(i, compression_options)),
        );

        group.bench_with_input(
            BenchmarkId::new(format!("compress {:?}", compression_options), size),
            &input_vec,
            |b, i| {
                b.iter(|| {
                    let mut in_buf = Cursor::new(i);
                    let mut out_buf = Cursor::new(vec![]);
                    compress::<Reference>(
                        size,
                        black_box(&mut in_buf),
                        black_box(&mut out_buf),
                        compression_options,
                    )
                })
            },
        );

        let compressed = easy_compress::<Reference>(input_vec, compression_options).unwrap();
        assert_eq!(
            easy_decompress::<Reference>(&compressed).unwrap(),
            input_vec
        );

        println!(
            "Compressed size: {} -> {}",
            input_vec.len(),
            compressed.len()
        );
        println!(
            "Compression ratio: {}",
            compressed.len() as f64 / input_vec.len() as f64
        );

        group.bench_with_input(
            BenchmarkId::new(
                format!("easy_decompress ({:?} compress)", compression_options),
                size,
            ),
            &compressed,
            |b, i| b.iter(|| easy_decompress::<Reference>(i)),
        );

        group.bench_with_input(
            BenchmarkId::new(
                format!("decompress ({:?} compress)", compression_options),
                size,
            ),
            &compressed,
            |b, i| {
                b.iter(|| {
                    let mut in_buf = Cursor::new(i);
                    let mut out_buf = Cursor::new(vec![]);
                    decompress::<Reference>(black_box(&mut in_buf), black_box(&mut out_buf))
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new(format!("easy symmetrical {:?}", compression_options), size),
            &input_vec,
            |b, i| {
                b.iter(|| {
                    let compressed = easy_compress::<Reference>(i, compression_options).unwrap();
                    easy_decompress::<Reference>(black_box(&compressed)).unwrap()
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new(format!("symmetrical {:?}", compression_options), size),
            &input_vec,
            |b, i| {
                b.iter(|| {
                    let mut in_buf = Cursor::new(i);
                    let mut out_buf = Cursor::new(vec![]);
                    let mut final_buf = Cursor::new(vec![]);

                    let _ = compress::<Reference>(
                        size,
                        black_box(&mut in_buf),
                        black_box(&mut out_buf),
                        compression_options,
                    );
                    out_buf.set_position(0);
                    let _ =
                        decompress::<Reference>(black_box(&mut out_buf), black_box(&mut final_buf));
                })
            },
        );
    }
}

fn increasing_data_sets_bench<S: Into<String>, F: FnMut(usize) -> Vec<u8>>(
    c: &mut Criterion<WallTime>,
    group_name: S,
    mut make_vec: F,
) {
    let mut group = c.benchmark_group(group_name);

    for size in CONST_BENCH_LENGTHS {
        group.throughput(Throughput::Bytes(size as u64));

        let random_input = make_vec(size);
        bench_set(&mut group, &random_input);
    }
    group.finish()
}

fn random_increasing_data_sets_bench(c: &mut Criterion<WallTime>) {
    increasing_data_sets_bench(c, "Random Input Data Increasing", random_vec);
}

fn random_bool_increasing_data_sets_bench(c: &mut Criterion<WallTime>) {
    increasing_data_sets_bench(c, "Random Boolean Input Data Increasing", random_bool_vec);
}

fn repeating_increasing_data_sets_bench(c: &mut Criterion<WallTime>) {
    increasing_data_sets_bench(c, "Repeating Input Data Increasing", repeating_vec);
}

fn zeros_increasing_data_sets_bench(c: &mut Criterion<WallTime>) {
    increasing_data_sets_bench(c, "All Zero Input Data Increasing", zeros_vec);
}

const BENCH_FILE_DIR: &str = "benches/bench_files/";
const BENCH_FILE_URL: &str = "https://sun.aei.polsl.pl//~sdeor/corpus/silesia.zip";

fn files_bench(c: &mut Criterion<WallTime>) {
    let num_files = fs::read_dir(BENCH_FILE_DIR).map(|x| x.count()).unwrap_or(0);
    if num_files == 0 {
        println!("Input bench files not found, downloading...");
        // create dir
        let _ = fs::create_dir(Path::new(BENCH_FILE_DIR));
        // download files with reqwest
        let mut buf = vec![];
        ureq::get(BENCH_FILE_URL)
            .call()
            .unwrap()
            .into_reader()
            .read_to_end(&mut buf)
            .unwrap();
        let mut out = fs::File::create("benches/silesia.zip").unwrap();
        out.write_all(&buf).unwrap();
        println!("Downloaded files");
        // unzip files
        let mut archive =
            zip::ZipArchive::new(fs::File::open("benches/silesia.zip").unwrap()).unwrap();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let outpath = Path::new(BENCH_FILE_DIR).join(file.name());
            let mut outfile = fs::File::create(&outpath).unwrap();
            io::copy(&mut file, &mut outfile).unwrap();
        }
        println!("Unzipped files");
        println!("Cleaning up...");
        fs::remove_file("benches/silesia.zip").unwrap();
    }

    let mut entries = fs::read_dir(BENCH_FILE_DIR)
        .unwrap()
        .map(|res| res.unwrap().path())
        .collect::<Vec<_>>();

    println!("Found {} files", entries.len());

    entries.sort();

    for file in entries {
        println!("File: {:?}", file.file_name().unwrap());

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
    .noise_threshold(0.02);
    targets = random_increasing_data_sets_bench,
    random_bool_increasing_data_sets_bench,
    repeating_increasing_data_sets_bench,
    zeros_increasing_data_sets_bench,
    files_bench
);
criterion_main!(benches);
