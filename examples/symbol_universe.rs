//! Fetch the listed-symbol universe and print summary counts.
//!
//! Run: `cargo run --example symbol_universe`

use nasdaq::{Client, DirectoryFile};

fn main() -> anyhow::Result<()> {
    let client = Client::new().with_cache("/tmp/nasdaq-cache", std::time::Duration::from_secs(3600));

    let nasdaq_listed = client.symbols(DirectoryFile::NasdaqListed)?;
    let other_listed = client.symbols(DirectoryFile::OtherListed)?;
    let bonds = client.symbols(DirectoryFile::Bonds)?;

    println!("Nasdaq-listed: {}", nasdaq_listed.len());
    println!("Other-listed:  {}", other_listed.len());
    println!("Bonds:         {}", bonds.len());

    // Full table access: ETFs among Nasdaq-traded symbols
    let traded = client.fetch(DirectoryFile::NasdaqTraded)?;
    let etfs = traded
        .records()
        .filter(|r| r.get("ETF").copied() == Some("Y"))
        .count();
    println!("ETFs traded:   {etfs}");
    Ok(())
}
