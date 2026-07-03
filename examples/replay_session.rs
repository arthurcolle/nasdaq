//! Replay an ITCH 5.0 session file and print a per-symbol summary.
//!
//! Run: `cargo run --release --example replay_session -- <file.NASDAQ_ITCH50> [SYMBOL]`

use std::fs::File;
use std::io::BufReader;

use nasdaq::itch::BookBuilder;
use nasdaq::orderbook::analytics;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: replay_session <file> [SYMBOL]");
    let symbol = args.next();

    let mut bb = match &symbol {
        Some(s) => BookBuilder::for_symbol(s.to_uppercase()),
        None => BookBuilder::new(),
    };

    let start = std::time::Instant::now();
    let n = bb.replay(BufReader::new(File::open(&path)?))?;
    let secs = start.elapsed().as_secs_f64();
    eprintln!("{n} messages in {secs:.1}s ({:.0} msg/s)", n as f64 / secs);

    let mut ranked: Vec<_> = bb.stats.iter().collect();
    ranked.sort_by_key(|(_, st)| std::cmp::Reverse(st.volume));
    for (sym, st) in ranked.iter().take(10) {
        let tob = bb.book(sym).map(analytics::top_of_book);
        let spread = tob
            .as_ref()
            .and_then(|t| t.spread_bps)
            .map(|b| format!("{b:.1}bps"))
            .unwrap_or_else(|| "-".into());
        println!(
            "{sym:<8} vol={:<12} adds={:<9} execs={:<8} spread={spread}",
            st.volume, st.adds, st.executes
        );
    }
    Ok(())
}
