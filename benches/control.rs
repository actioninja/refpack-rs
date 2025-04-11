use std::io::Cursor;

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use refpack::data::control::{Command, Control};
use refpack::easy_decompress;
use refpack::format::{Format, Reference};
use refpack::header::mode::Mode as HeaderMode;
use refpack::header::Header;

const CONST_BENCH_LENGTH: usize = 8096;

fn repeating_short_control_vec(repeats: usize) -> Vec<Control> {
    let mut ret = vec![Control::new_literal_block(&[0; 4])];
    ret.append(&mut vec![
        Control::new(Command::new(1, 3, 0), vec![]);
        repeats
    ]);
    ret.push(Control::new_stop(&[]));
    ret
}

fn repeating_short_control_data<F: Format>(repeats: usize) -> Vec<u8> {
    let mut writer = Cursor::new(vec![]);

    let controls = repeating_short_control_vec(repeats);

    let header_length = F::HeaderMode::length(repeats + 1);

    writer.set_position(header_length as u64);

    for control in controls {
        control.write(&mut writer).unwrap();
    }

    let data_end_pos = writer.position();

    let compression_length = data_end_pos;

    let header = Header {
        compressed_length: Some(compression_length as u32),
        decompressed_length: (repeats + 1) as u32,
    };

    writer.set_position(0);

    header.write::<F::HeaderMode>(&mut writer).unwrap();

    writer.into_inner()
}

fn repeating_short_control_bench(c: &mut Criterion<WallTime>) {
    let mut group = c.benchmark_group("Repeating short control copy 1 byte".to_string());

    group.throughput(Throughput::Bytes(CONST_BENCH_LENGTH as u64));

    let input = repeating_short_control_data::<Reference>(CONST_BENCH_LENGTH);

    group.bench_with_input("easy_decompress", &input, |b, i| {
        b.iter(|| easy_decompress::<Reference>(i))
    });

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = repeating_short_control_bench
);
criterion_main!(benches);
