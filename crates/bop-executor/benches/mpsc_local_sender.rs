use bop_executor::mpsc;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_mpsc_local_sender(c: &mut Criterion) {
    c.bench_function("mpsc_local_sender_roundtrip", |b| {
        let (mut sender, mut receiver) = mpsc::new::<u64>(7, 9);
        let mut buffer = [0u64; 128];
        b.iter(|| {
            for i in 0..buffer.len() {
                sender.try_push(i as u64).unwrap();
            }
            let drained = receiver.try_pop_n(&mut buffer);
            black_box(drained);
        });
    });
}

criterion_group!(benches, bench_mpsc_local_sender);
criterion_main!(benches);
