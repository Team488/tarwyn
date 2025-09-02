use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use tarwyn::{
    utils::ring_buffer::RingBuffer,
    tarwyn_client::TarwynClient,
    tarwyn_server::{self, TarwynServer},
};

fn bench_format(c: &mut Criterion) {
    let mut counter = 0u64;
    c.bench_function("format_concat", |b| {
        b.iter(|| {
            let s = format!("hello{}", counter);
            black_box(&s);
            counter += 1;
        })
    });
}

fn bench_counter_machine(c: &mut Criterion) {
    let machine_id: u32 = 42;
    let mut local_counter = 0u64;
    c.bench_function("counter_machine_id", |b| {
        b.iter(|| {
            let id = ((machine_id as u128) << 64) | (local_counter as u128);
            black_box(id);
            local_counter += 1;
        })
    });
}

fn bench_ring_buffers(c: &mut Criterion) {
    c.bench_function("ring_buffer_placeholder", |b| {
        b.iter(|| {
            let mut ring_buffer = RingBuffer::new(10);
            ring_buffer.push("hello");
            black_box(ring_buffer);
        })
    });
}

criterion_group!(
    benches,
    bench_format,
    bench_counter_machine,
    bench_ring_buffers,
);

criterion_main!(benches);
