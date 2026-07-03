# nasdaq

Rust client for the [Nasdaq Trader symbol directory](https://www.nasdaqtrader.com/trader.aspx?id=symboldirdefs):
ticker universes, bonds, options, and mutual funds — plus a small price-level orderbook.

Fetches over anonymous FTP (`ftp.nasdaqtrader.com/symboldirectory`) with automatic
HTTPS fallback. No API key required; this is public data.

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
```

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

### Orderbook

```rust
use nasdaq::orderbook::{Orderbook, Price, Quantity, Side};

let mut ob = Orderbook::new();
ob.add(Side::Bid, Price(9_975), Quantity(50));   // prices in cents
ob.add(Side::Ask, Price(10_000), Quantity(75));
assert_eq!(ob.spread(), Some(25));
```

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
