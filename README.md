# nasdaq

Nasdaq market-structure toolkit in Rust:

- **Symbol directory client** — ticker universes, bonds, options, mutual funds from
  [Nasdaq Trader](https://www.nasdaqtrader.com/trader.aspx?id=symboldirdefs) over
  anonymous FTP with automatic HTTPS fallback and optional TTL disk cache. No API key.
- **ITCH 5.0 parser** — Nasdaq's TotalView binary feed protocol: 15 message types,
  length-framed file replay, per-symbol book building and session stats.
- **Matching-engine orderbook** — full order tracking (insert/cancel/replace/execute
  by id), price-time priority limit/market matching, aggregated levels.
- **Book analytics** — spread (abs/bps), mid, microprice, imbalance, depth-within-bps.

## CLI

```sh
# All Nasdaq-traded symbols, one per line
nasdaq tickers

# Nasdaq-listed only / other-listed (NYSE, AMEX, ...) only
nasdaq tickers --file nasdaq-listed
nasdaq tickers --file other-listed

# Combined listed universe
nasdaq all

# Download any directory file as CSV
nasdaq fetch options -o options.csv
nasdaq fetch bonds

# Force a transport
nasdaq --transport https tickers

# Cache fetched files for an hour (repeat calls are ~50x faster)
nasdaq --cache ~/.cache/nasdaq tickers

# Replay an ITCH 5.0 session file: per-symbol stats + top-of-book
nasdaq itch 01302019.NASDAQ_ITCH50 --top 20
nasdaq itch 01302019.NASDAQ_ITCH50 --symbol AAPL --json
```

Sample full-session ITCH files are published by Nasdaq at
`emi.nasdaq.com/ITCH/Nasdaq ITCH/` (several GB each, gzipped).

## Library

```rust
use nasdaq::{Client, DirectoryFile};

fn main() -> nasdaq::Result<()> {
    let client = Client::new();

    // Symbol lists
    let tickers = client.symbols(DirectoryFile::NasdaqListed)?;
    assert!(tickers.contains(&"AAPL".to_string()));

    // Full tables with headers + typed access
    let options = client.fetch(DirectoryFile::Options)?;
    for rec in options.records().take(5) {
        println!("{} {}", rec["Underlying Symbol"], rec["Expiration Date"]);
    }
    options.write_csv("options.csv")?;
    Ok(())
}
```

### Orderbook + matching engine

```rust
use nasdaq::orderbook::{analytics, Orderbook, Price, Side};

let mut ob = Orderbook::new();
ob.insert(101, Side::Bid, Price(9_975), 50)?;   // feed-handler style, by order id
ob.insert(201, Side::Ask, Price(10_000), 75)?;

let exec = ob.limit(Side::Bid, Price(10_000), 100)?; // crosses: fills 75, rests 25
assert_eq!(exec.fills[0].qty, 75);

let tob = analytics::top_of_book(&ob);
println!("mid={:?} microprice={:?} imbalance={:?}", tob.mid, tob.microprice, tob.imbalance);
```

### ITCH 5.0 replay

```rust
use nasdaq::itch::BookBuilder;
use std::{fs::File, io::BufReader};

let mut bb = BookBuilder::for_symbol("AAPL");
bb.replay(BufReader::new(File::open("session.NASDAQ_ITCH50")?))?;
let book = bb.book("AAPL").unwrap();
let stats = &bb.stats["AAPL"];
```

## Performance

Synthetic 1M-message session, M2 Max (`cargo bench`):

| Path | Throughput |
|---|---|
| Parse (framed reader) | ~25M msg/s |
| Parse (unframed, direct) | ~41M msg/s |
| Full replay, all symbols (book build) | ~2.4M msg/s |
| Filtered replay, one symbol | ~12.6M msg/s |

A full trading day (~400M messages) replays in a few minutes; single-symbol
studies run in well under a minute.

## Tests

```sh
cargo test               # offline unit tests
cargo test -- --ignored  # live network tests against nasdaqtrader.com
```

## Files

| `DirectoryFile` | Remote file | Contents |
|---|---|---|
| `NasdaqListed` | `nasdaqlisted.txt` | Nasdaq-listed equities |
| `OtherListed` | `otherlisted.txt` | NYSE/AMEX/other-listed equities |
| `NasdaqTraded` | `nasdaqtraded.txt` | Everything traded on Nasdaq |
| `Bonds` | `bondslist.txt` | Listed bonds |
| `Options` | `options.txt` | Options directory |
| `MutualFunds` | `mfundslist.txt` | Mutual funds |
