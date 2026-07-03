//! Benchmarks: ITCH parse throughput and book-building replay throughput.
//!
//! Run: `cargo bench`

use std::hint::black_box;
use std::time::Instant;

use nasdaq::itch::{parse, BookBuilder, Reader};

// Build a synthetic framed session in memory: `n` messages across `syms` symbols.
fn synth_session(n: usize) -> Vec<u8> {
    let syms = ["AAPL", "MSFT", "NVDA", "AMD", "TSLA", "AMZN", "GOOG", "META"];
    let mut out = Vec::with_capacity(n * 44);
    let mut oid: u64 = 0;
    let mut live: Vec<u64> = Vec::new();
    let mut state = 0x9E3779B97F4A7C15u64;
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut ns: u64 = 34_200_000_000_000;

    let mut push = |out: &mut Vec<u8>, body: &[u8]| {
        out.extend((body.len() as u16).to_be_bytes());
        out.extend(body);
    };

    for _ in 0..n {
        ns += 1 + rng() % 50_000;
        let r = rng() % 100;
        if r < 55 || live.is_empty() {
            oid += 1;
            let sym = syms[(rng() % syms.len() as u64) as usize];
            let side = if rng() % 2 == 0 { b'B' } else { b'S' };
            let px: u32 = 1_000_000 + (rng() % 100_000) as u32;
            let sh: u32 = 100 * (1 + (rng() % 5) as u32);
            let mut b = Vec::with_capacity(36);
            b.push(b'A');
            b.extend([0u8, 1, 0, 0]);
            b.extend(&ns.to_be_bytes()[2..]);
            b.extend(oid.to_be_bytes());
            b.push(side);
            b.extend(sh.to_be_bytes());
            let mut s8 = [b' '; 8];
            s8[..sym.len()].copy_from_slice(sym.as_bytes());
            b.extend(s8);
            b.extend(px.to_be_bytes());
            push(&mut out, &b);
            live.push(oid);
        } else if r < 80 {
            let id = live[(rng() % live.len() as u64) as usize];
            let mut b = Vec::with_capacity(31);
            b.push(b'E');
            b.extend([0u8, 1, 0, 0]);
            b.extend(&ns.to_be_bytes()[2..]);
            b.extend(id.to_be_bytes());
            b.extend(100u32.to_be_bytes());
            b.extend(7u64.to_be_bytes());
            push(&mut out, &b);
            if rng() % 2 == 0 {
                live.retain(|&x| x != id);
            }
        } else {
            let idx = (rng() % live.len() as u64) as usize;
            let id = live.swap_remove(idx);
            let mut b = Vec::with_capacity(19);
            b.push(b'D');
            b.extend([0u8, 1, 0, 0]);
            b.extend(&ns.to_be_bytes()[2..]);
            b.extend(id.to_be_bytes());
            push(&mut out, &b);
        }
    }
    out
}

fn bench(name: &str, msgs: usize, mut f: impl FnMut()) {
    // warmup
    f();
    let runs = 5;
    let start = Instant::now();
    for _ in 0..runs {
        f();
    }
    let per_run = start.elapsed() / runs;
    let rate = msgs as f64 / per_run.as_secs_f64();
    println!("{name:<28} {per_run:>10.2?}/run   {:.2}M msg/s", rate / 1e6);
}

fn main() {
    const N: usize = 1_000_000;
    let session = synth_session(N);
    println!("synthetic session: {N} messages, {} MB", session.len() / 1_048_576);

    bench("parse only", N, || {
        let mut count = 0u64;
        for msg in Reader::new(session.as_slice()) {
            black_box(msg.unwrap());
            count += 1;
        }
        assert_eq!(count as usize, N);
    });

    bench("parse (unframed, direct)", N, || {
        let mut pos = 0usize;
        while pos + 2 <= session.len() {
            let len = u16::from_be_bytes([session[pos], session[pos + 1]]) as usize;
            pos += 2;
            black_box(parse(&session[pos..pos + len]).unwrap());
            pos += len;
        }
    });

    bench("full replay (book build)", N, || {
        let mut bb = BookBuilder::new();
        bb.replay(session.as_slice()).unwrap();
        black_box(&bb);
    });

    bench("filtered replay (1 symbol)", N, || {
        let mut bb = BookBuilder::for_symbol("AAPL");
        bb.replay(session.as_slice()).unwrap();
        black_box(&bb);
    });
}
