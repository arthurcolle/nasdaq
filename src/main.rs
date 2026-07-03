use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use nasdaq::itch::BookBuilder;
use nasdaq::orderbook::analytics;
use nasdaq::{Client, DirectoryFile, Transport};

#[derive(Parser)]
#[command(name = "nasdaq", version, about = "Nasdaq market-structure toolkit")]
struct Cli {
    /// Transport: auto (FTP with HTTPS fallback), ftp, or https
    #[arg(long, value_enum, default_value_t = TransportArg::Auto)]
    transport: TransportArg,

    /// Cache directory for fetched files (TTL 1h); disables refetch churn
    #[arg(long)]
    cache: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum TransportArg {
    Auto,
    Ftp,
    Https,
}

impl From<TransportArg> for Transport {
    fn from(t: TransportArg) -> Self {
        match t {
            TransportArg::Auto => Transport::Auto,
            TransportArg::Ftp => Transport::Ftp,
            TransportArg::Https => Transport::Https,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum FileArg {
    NasdaqListed,
    OtherListed,
    NasdaqTraded,
    Bonds,
    Options,
    MutualFunds,
}

impl From<FileArg> for DirectoryFile {
    fn from(f: FileArg) -> Self {
        match f {
            FileArg::NasdaqListed => DirectoryFile::NasdaqListed,
            FileArg::OtherListed => DirectoryFile::OtherListed,
            FileArg::NasdaqTraded => DirectoryFile::NasdaqTraded,
            FileArg::Bonds => DirectoryFile::Bonds,
            FileArg::Options => DirectoryFile::Options,
            FileArg::MutualFunds => DirectoryFile::MutualFunds,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Print symbols, one per line
    Tickers {
        #[arg(long, value_enum, default_value_t = FileArg::NasdaqTraded)]
        file: FileArg,
    },
    /// Print combined Nasdaq-listed + other-listed symbols
    All,
    /// Fetch a directory file and write CSV (or JSON with --json)
    Fetch {
        #[arg(value_enum)]
        file: FileArg,
        /// Output path (defaults to <file>.csv / <file>.json)
        #[arg(long, short)]
        out: Option<String>,
        /// Emit JSON records instead of CSV
        #[arg(long)]
        json: bool,
    },
    /// Replay an ITCH 5.0 file and print per-symbol stats + top-of-book
    Itch {
        /// Path to a length-framed ITCH 5.0 file (*.NASDAQ_ITCH50[.gz])
        path: String,
        /// Restrict to one symbol (much lower memory on full sessions)
        #[arg(long, short)]
        symbol: Option<String>,
        /// Show top N symbols by volume (ignored with --symbol)
        #[arg(long, default_value_t = 10)]
        top: usize,
        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let mut client = Client::with_transport(cli.transport.into());
    if let Some(dir) = &cli.cache {
        client = client.with_cache(dir, Duration::from_secs(3600));
    }

    match cli.command {
        Command::Tickers { file } => {
            let file: DirectoryFile = file.into();
            for s in client.symbols(file)? {
                println!("{s}");
            }
        }
        Command::All => {
            for s in client.symbols(DirectoryFile::OtherListed)? {
                println!("{s}");
            }
            for s in client.symbols(DirectoryFile::NasdaqListed)? {
                println!("{s}");
            }
        }
        Command::Fetch { file, out, json } => {
            let file: DirectoryFile = file.into();
            let table = client.fetch(file)?;
            let stem = file.file_name();
            if json {
                let path = out.unwrap_or_else(|| format!("{stem}.json"));
                let records: Vec<_> = table.records().collect();
                std::fs::write(&path, serde_json::to_string_pretty(&records)?)?;
                eprintln!("wrote {path}: {} records", records.len());
            } else {
                let path = out.unwrap_or_else(|| format!("{stem}.csv"));
                table.write_csv(&path)?;
                eprintln!(
                    "wrote {path}: {} rows x {} cols",
                    table.rows.len(),
                    table.headers.len()
                );
            }
        }
        Command::Itch { path, symbol, top, json } => {
            let mut bb = match &symbol {
                Some(s) => BookBuilder::for_symbol(s.to_uppercase()),
                None => BookBuilder::new(),
            };
            let n = bb.replay(nasdaq::itch::open_session(&path)?)?;
            eprintln!("replayed {n} messages from {path}");

            let mut ranked: Vec<_> = bb.stats.iter().collect();
            ranked.sort_by_key(|(_, st)| std::cmp::Reverse(st.volume));
            let show: Vec<_> = match &symbol {
                Some(s) => ranked.into_iter().filter(|(sym, _)| *sym == s).collect(),
                None => ranked.into_iter().take(top).collect(),
            };

            if json {
                let out: Vec<_> = show
                    .iter()
                    .map(|(sym, st)| {
                        let tob = bb.book(sym).map(analytics::top_of_book);
                        serde_json::json!({ "symbol": sym, "stats": st, "top_of_book": tob })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "{:<10} {:>10} {:>8} {:>8} {:>8} {:>12} {:>12} {:>8}",
                    "symbol", "volume", "adds", "execs", "dels", "best_bid", "best_ask", "imbal"
                );
                for (sym, st) in show {
                    let (bid, ask, imb) = bb
                        .book(sym)
                        .map(|b| {
                            let t = analytics::top_of_book(b);
                            (
                                t.bid.map(|l| format!("{:.4}", l.price.0 as f64 / 10_000.0)),
                                t.ask.map(|l| format!("{:.4}", l.price.0 as f64 / 10_000.0)),
                                t.imbalance.map(|i| format!("{i:+.2}")),
                            )
                        })
                        .unwrap_or((None, None, None));
                    println!(
                        "{:<10} {:>10} {:>8} {:>8} {:>8} {:>12} {:>12} {:>8}",
                        sym,
                        st.volume,
                        st.adds,
                        st.executes,
                        st.deletes,
                        bid.unwrap_or_else(|| "-".into()),
                        ask.unwrap_or_else(|| "-".into()),
                        imb.unwrap_or_else(|| "-".into()),
                    );
                }
            }
        }
    }
    Ok(())
}
