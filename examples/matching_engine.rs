//! Drive the matching engine directly: build a book, cross it, inspect fills.
//!
//! Run: `cargo run --example matching_engine`

use nasdaq::orderbook::{analytics, Orderbook, Price, Side};

fn main() -> Result<(), nasdaq::orderbook::BookError> {
    let mut ob = Orderbook::new();

    // Seed passive liquidity (prices in cents)
    ob.insert(1, Side::Bid, Price(9_975), 500)?;
    ob.insert(2, Side::Bid, Price(9_950), 800)?;
    ob.insert(3, Side::Ask, Price(10_000), 300)?;
    ob.insert(4, Side::Ask, Price(10_000), 200)?; // behind #3 in queue
    ob.insert(5, Side::Ask, Price(10_050), 1_000)?;

    let tob = analytics::top_of_book(&ob);
    println!("before: mid={:?} imbalance={:?}", tob.mid, tob.imbalance);

    // Aggressive buy crosses the spread
    let exec = ob.limit(Side::Bid, Price(10_050), 700)?;
    for f in &exec.fills {
        println!("fill: maker={} {}@{}", f.maker_id, f.qty, f.price);
    }
    println!("remaining rested: {} (id {:?})", exec.remaining, exec.resting_id);

    let tob = analytics::top_of_book(&ob);
    println!(
        "after: bid={:?} ask={:?} traded={}",
        tob.bid.map(|l| (l.price, l.qty)),
        tob.ask.map(|l| (l.price, l.qty)),
        ob.traded_volume
    );
    Ok(())
}
