//! Client for the Nasdaq Trader symbol directory.
//!
//! Fetches the public symbol-directory files (tickers, bonds, options) from
//! `ftp.nasdaqtrader.com`, with an HTTPS fallback to `nasdaqtrader.com`.
//! Files are pipe-delimited with a `File Creation Time` trailer row.

pub mod orderbook;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use suppaftp::FtpStream;
use thiserror::Error;

const FTP_HOST: &str = "ftp.nasdaqtrader.com:21";
const FTP_DIR: &str = "symboldirectory";
const HTTPS_BASE: &str = "https://www.nasdaqtrader.com/dynamic/SymDir";

/// Errors returned by this crate.
#[derive(Debug, Error)]
pub enum Error {
    #[error("ftp error: {0}")]
    Ftp(#[from] suppaftp::FtpError),
    #[error("http error: {0}")]
    Http(Box<ureq::Error>),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("file {file:?} is empty or malformed")]
    Malformed { file: String },
    #[error("column {column:?} not found in {file:?} (available: {available:?})")]
    MissingColumn {
        file: String,
        column: String,
        available: Vec<String>,
    },
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Http(Box::new(e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Known symbol-directory files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryFile {
    /// Nasdaq-listed equities (`nasdaqlisted.txt`).
    NasdaqListed,
    /// NYSE/AMEX/other-listed equities (`otherlisted.txt`).
    OtherListed,
    /// All Nasdaq-traded symbols (`nasdaqtraded.txt`).
    NasdaqTraded,
    /// Listed bonds (`bondslist.txt`).
    Bonds,
    /// Options directory (`options.txt`).
    Options,
    /// Mutual funds (`mfundslist.txt`).
    MutualFunds,
}

impl DirectoryFile {
    pub fn file_name(self) -> &'static str {
        match self {
            Self::NasdaqListed => "nasdaqlisted.txt",
            Self::OtherListed => "otherlisted.txt",
            Self::NasdaqTraded => "nasdaqtraded.txt",
            Self::Bonds => "bondslist.txt",
            Self::Options => "options.txt",
            Self::MutualFunds => "mfundslist.txt",
        }
    }

    /// Symbol column header for this file.
    pub fn symbol_column(self) -> &'static str {
        match self {
            Self::OtherListed => "ACT Symbol",
            _ => "Symbol",
        }
    }
}

/// A parsed pipe-delimited directory table: ordered headers + rows.
#[derive(Debug, Clone, Default)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    fn parse(file: &str, content: &str) -> Result<Table> {
        let mut lines = content.lines();
        let headers: Vec<String> = lines
            .next()
            .ok_or_else(|| Error::Malformed { file: file.into() })?
            .split('|')
            .map(str::to_owned)
            .collect();
        if headers.is_empty() {
            return Err(Error::Malformed { file: file.into() });
        }
        let mut rows: Vec<Vec<String>> = Vec::new();
        for line in lines {
            // Trailer looks like: "File Creation Time: 0702202522:01|||||"
            if line.starts_with("File Creation Time") || line.trim().is_empty() {
                continue;
            }
            let mut row: Vec<String> = line.split('|').map(str::to_owned).collect();
            row.resize(headers.len(), String::new());
            rows.push(row);
        }
        Ok(Table { headers, rows })
    }

    /// Index of a column by header name.
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == name)
    }

    /// All values in a named column.
    pub fn column(&self, file: &str, name: &str) -> Result<Vec<String>> {
        let idx = self
            .column_index(name)
            .ok_or_else(|| Error::MissingColumn {
                file: file.into(),
                column: name.into(),
                available: self.headers.clone(),
            })?;
        Ok(self.rows.iter().map(|r| r[idx].clone()).collect())
    }

    /// Rows as header->value maps (BTreeMap for stable ordering).
    pub fn records(&self) -> impl Iterator<Item = BTreeMap<&str, &str>> {
        self.rows.iter().map(|row| {
            self.headers
                .iter()
                .map(String::as_str)
                .zip(row.iter().map(String::as_str))
                .collect()
        })
    }

    /// Write the table as CSV to `path`.
    pub fn write_csv(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut w = csv::Writer::from_path(path)?;
        w.write_record(&self.headers)?;
        for row in &self.rows {
            w.write_record(row)?;
        }
        w.flush()?;
        Ok(())
    }
}

/// Transport used to reach the symbol directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    /// Try FTP first, fall back to HTTPS.
    #[default]
    Auto,
    Ftp,
    Https,
}

/// Client for the Nasdaq Trader symbol directory.
#[derive(Debug, Clone, Default)]
pub struct Client {
    transport: Transport,
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_transport(transport: Transport) -> Self {
        Self { transport }
    }

    /// Fetch and parse a directory file.
    pub fn fetch(&self, file: DirectoryFile) -> Result<Table> {
        let name = file.file_name();
        let content = match self.transport {
            Transport::Ftp => self.fetch_ftp(name)?,
            Transport::Https => self.fetch_https(name)?,
            Transport::Auto => self
                .fetch_ftp(name)
                .or_else(|_| self.fetch_https(name))?,
        };
        Table::parse(name, &content)
    }

    /// Symbols from a directory file.
    pub fn symbols(&self, file: DirectoryFile) -> Result<Vec<String>> {
        let table = self.fetch(file)?;
        table.column(file.file_name(), file.symbol_column())
    }

    fn fetch_ftp(&self, name: &str) -> Result<String> {
        let mut ftp = FtpStream::connect(FTP_HOST)?;
        ftp.login("anonymous", "anonymous")?;
        ftp.cwd(FTP_DIR)?;
        let cursor: Cursor<Vec<u8>> = ftp.retr_as_buffer(name)?;
        let _ = ftp.quit();
        Ok(String::from_utf8_lossy(cursor.get_ref()).into_owned())
    }

    fn fetch_https(&self, name: &str) -> Result<String> {
        let url = format!("{HTTPS_BASE}/{name}");
        let mut resp = ureq::get(&url)
            .config()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .call()?;
        Ok(resp.body_mut().read_to_string()?)
    }
}

// ---------------------------------------------------------------------------
// Convenience functions (compatible with the 0.1 API, now returning Result)
// ---------------------------------------------------------------------------

/// Nasdaq-listed symbols.
pub fn nasdaq_tickers() -> Result<Vec<String>> {
    Client::new().symbols(DirectoryFile::NasdaqListed)
}

/// NYSE/AMEX/other-listed symbols (ACT symbols).
pub fn nyse_tickers() -> Result<Vec<String>> {
    Client::new().symbols(DirectoryFile::OtherListed)
}

/// Union of other-listed and Nasdaq-listed symbols.
pub fn all_tickers() -> Result<Vec<String>> {
    let mut out = nyse_tickers()?;
    out.extend(nasdaq_tickers()?);
    Ok(out)
}

/// All Nasdaq-traded symbols.
pub fn nasdaqtraded() -> Result<Vec<String>> {
    Client::new().symbols(DirectoryFile::NasdaqTraded)
}

/// Listed bonds.
pub fn bonds() -> Result<Vec<String>> {
    Client::new().symbols(DirectoryFile::Bonds)
}

/// The full options directory as a [`Table`].
pub fn all_options() -> Result<Table> {
    Client::new().fetch(DirectoryFile::Options)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Symbol|Security Name|Test Issue\n\
AAPL|Apple Inc. - Common Stock|N\n\
MSFT|Microsoft Corporation - Common Stock|N\n\
ZXYZ.A|Test Row|Y\n\
File Creation Time: 0702202522:01||";

    #[test]
    fn parse_table() {
        let t = Table::parse("sample.txt", SAMPLE).unwrap();
        assert_eq!(t.headers, vec!["Symbol", "Security Name", "Test Issue"]);
        assert_eq!(t.rows.len(), 3); // trailer dropped
        assert_eq!(t.rows[0][0], "AAPL");
    }

    #[test]
    fn column_extraction() {
        let t = Table::parse("sample.txt", SAMPLE).unwrap();
        let syms = t.column("sample.txt", "Symbol").unwrap();
        assert_eq!(syms, vec!["AAPL", "MSFT", "ZXYZ.A"]);
    }

    #[test]
    fn missing_column_error() {
        let t = Table::parse("sample.txt", SAMPLE).unwrap();
        let err = t.column("sample.txt", "Nope").unwrap_err();
        assert!(matches!(err, Error::MissingColumn { .. }));
    }

    #[test]
    fn records_map() {
        let t = Table::parse("sample.txt", SAMPLE).unwrap();
        let first = t.records().next().unwrap();
        assert_eq!(first["Symbol"], "AAPL");
        assert_eq!(first["Test Issue"], "N");
    }

    #[test]
    fn ragged_rows_are_padded() {
        let ragged = "A|B|C\n1|2\nFile Creation Time: x||";
        let t = Table::parse("r.txt", ragged).unwrap();
        assert_eq!(t.rows[0], vec!["1", "2", ""]);
    }

    // Network tests: `cargo test -- --ignored`
    #[test]
    #[ignore = "hits ftp.nasdaqtrader.com"]
    fn live_nasdaq_tickers() {
        let tickers = nasdaq_tickers().unwrap();
        assert!(tickers.len() > 1000);
        assert!(tickers.iter().any(|t| t == "AAPL"));
    }

    #[test]
    #[ignore = "hits nasdaqtrader.com over https"]
    fn live_https_fallback() {
        let client = Client::with_transport(Transport::Https);
        let t = client.fetch(DirectoryFile::NasdaqListed).unwrap();
        assert!(t.rows.len() > 1000);
    }
}
