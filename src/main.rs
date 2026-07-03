use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use nasdaq::{Client, DirectoryFile, Transport};

#[derive(Parser)]
#[command(name = "nasdaq", version, about = "Nasdaq Trader symbol directory client")]
struct Cli {
    /// Transport: auto (FTP with HTTPS fallback), ftp, or https
    #[arg(long, value_enum, default_value_t = TransportArg::Auto)]
    transport: TransportArg,

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
        /// Which directory file to read symbols from
        #[arg(long, value_enum, default_value_t = FileArg::NasdaqTraded)]
        file: FileArg,
    },
    /// Print combined Nasdaq-listed + other-listed symbols
    All,
    /// Fetch a directory file and write it as CSV
    Fetch {
        #[arg(value_enum)]
        file: FileArg,
        /// Output CSV path (defaults to <file>.csv)
        #[arg(long, short)]
        out: Option<String>,
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

fn run(cli: Cli) -> nasdaq::Result<()> {
    let client = Client::with_transport(cli.transport.into());
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
        Command::Fetch { file, out } => {
            let file: DirectoryFile = file.into();
            let table = client.fetch(file)?;
            let path = out.unwrap_or_else(|| format!("{}.csv", file.file_name()));
            table.write_csv(&path)?;
            eprintln!(
                "wrote {path}: {} rows x {} cols",
                table.rows.len(),
                table.headers.len()
            );
        }
    }
    Ok(())
}
