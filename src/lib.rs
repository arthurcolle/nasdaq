use ftp::{FtpStream};
use polars::prelude::*;
use std::collections::HashSet;

pub enum Exchange {
    Nasdaq,
    Nyse,
    Amex,
    Cme,
}

enum SymbolType {
    Generic,
    Act,
    Cqs,
    Nasdaq
}

struct Ticker {
    symbol: String,
    description: Option<String>,
    exchange: Option<Exchange>,
    etf: Option<bool>,
    active: Option<bool>,
    test: Option<bool>,
}

impl Ticker {
    fn new(symbol: String, description: Option<String>, exchange: Option<Exchange>, etf: Option<bool>, active: Option<bool>, test: Option<bool>) -> Ticker {
        Ticker {
            symbol,
            description,
            exchange,
            etf,
            active,
            test,
        }
    }
}

use std::collections::BTreeMap;
pub fn nasdaq_tickers() -> Vec<String> {
    return tickers("nasdaqlisted.txt".to_string(), "Symbol".to_string());
}

pub fn nyse_tickers() -> Vec<String> {
    return tickers("otherlisted.txt".to_string(), "ACT Symbol".to_string());
}

pub fn all_tickers() -> Vec<String> {
    let nt = nasdaq_tickers();
    let ny = nyse_tickers();
    let tickers = ny.to_vec().into_iter().chain(nt.into_iter()).collect();
    return tickers;
}

pub fn nasdaqtraded() -> Vec<String> {
    return tickers("nasdaqtraded.txt".to_string(), "Symbol".to_string());
}

pub fn bonds() -> Vec<String> {
    return tickers("bondslist.txt".to_string(), "Symbol".to_string());
}

pub fn all_options() -> DataFrame {
    let mut options = tickers_df("options.txt".to_string());

    let mut ofile = std::fs::File::create("options.csv").unwrap();
    CsvWriter::new(&mut ofile).finish(&mut options.clone()).unwrap();
    println!("Wrote options_oids.csv");
    return options;
}

pub fn tickers_df(file: String) -> DataFrame {
    let response: Vec<String> = Vec::new();
    let mut connection = FtpStream::connect("ftp.nasdaqtrader.com:21").unwrap_or_else(|err| panic!("{}", err) );

    // Download the nasdaqlisted and otherlisted txt files from the directory...
    connection.login("anonymous", "anonymous").unwrap_or_else(|err| panic!("{}", err) );

    // Navigate to symboldirectory, full of useful datasets
    connection.cwd("symboldirectory").unwrap_or_else(|err| panic!("{}", err) );

    let lines = connection.simple_retr(&file).unwrap_or_else(|err| 
        panic!("{}", err)
    ).into_inner();

    let utf8_content = String::from_utf8(lines)
        .map_err(|non_utf8| String::from_utf8_lossy(non_utf8.as_bytes()).into_owned())
        .unwrap();

    let col_headers = utf8_content.lines().next().unwrap().split("|");
    let mut col_data: BTreeMap<String, Vec<String>> = col_headers.clone().map(|header| {
        (header.to_string(), Vec::new())
    }).collect();

    let data = utf8_content.lines().skip(1).map(|line| line.split("|"));
    // skip the last line for each file
    let data = data.clone().take(data.count() - 1);

    data.into_iter().for_each(|row| {
        col_headers.clone().zip(row).for_each(|(header, data)| {
            col_data.get_mut(header).unwrap().push(data.to_string());
        });
    });

    let vecs = col_data.iter().map(|(key, value)| { 
        Series::new(key, value)
    }).collect();

    let mut df = DataFrame::new(vecs).unwrap();
    // write df to csv
    let mut ofile = std::fs::File::create(format!("{}.csv", file)).unwrap();
    CsvWriter::new(&mut ofile).finish(&mut df).unwrap();
    
    df.as_single_chunk_par();
    println!("{:#?}", df);
    return df;
}


pub fn tickers(file: String, col: String) -> Vec<String> {
    let response: Vec<String> = Vec::new();
    let mut connection = FtpStream::connect("ftp.nasdaqtrader.com:21").unwrap_or_else(|err| panic!("{}", err) );

    // Download the nasdaqlisted and otherlisted txt files from the directory...
    connection.login("anonymous", "anonymous").unwrap_or_else(|err| panic!("{}", err) );

    // Navigate to symboldirectory, full of useful datasets
    connection.cwd("symboldirectory").unwrap_or_else(|err| panic!("{}", err) );

    let lines = connection.simple_retr(&file).unwrap_or_else(|err| 
        panic!("{}", err)
    ).into_inner();

    let utf8_content = String::from_utf8(lines)
        .map_err(|non_utf8| String::from_utf8_lossy(non_utf8.as_bytes()).into_owned())
        .unwrap();

    let col_headers = utf8_content.lines().next().unwrap().split("|");
    let mut col_data: BTreeMap<String, Vec<String>> = col_headers.clone().map(|header| {
        (header.to_string(), Vec::new())
    }).collect();

    let data = utf8_content.lines().skip(1).map(|line| line.split("|"));
    // skip the last line for each file
    let data = data.clone().take(data.count() - 1);

    data.into_iter().for_each(|row| {
        col_headers.clone().zip(row).for_each(|(header, data)| {
            col_data.get_mut(header).unwrap().push(data.to_string());
        });
    });

    let vecs = col_data.iter().map(|(key, value)| { 
        Series::new(key, value)
    }).collect();

    let mut df = DataFrame::new(vecs).unwrap();
    // write df to csv
    let mut ofile = std::fs::File::create(format!("{}.csv", file)).unwrap();
    
    CsvWriter::new(&mut ofile).finish(&mut df).unwrap();
    println!("File created: {:?}", ofile.metadata().unwrap());
    let mut symbols = df.column(&col).unwrap().clone();
    return symbols.iter().map(|x| x.to_string().replace("\"", "")).collect();
}


// // use tickers to get ticker data and then return a DataFrame
// fn tickers_df() -> DataFrame {
//     let nt = nasdaq_tickers();
//     let ny = nyse_tickers();
//     let tickers: Vec<String> = ny.to_vec().into_iter().chain(nt.into_iter()).collect();
//     let mut df = DataFrame::new(vec![Series::new("tickers", tickers)]).unwrap();
//     return df;
// }


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonds_test() {
        let bonds: Vec<String> = bonds();
        println!("{:#?}", bonds);
    }

    #[test]
    fn nasdaq_tickers_test() {
        let tickers: Vec<String> = nasdaq_tickers();
        println!("{:#?}", tickers);

    }

    #[test]
    fn nyse_tickers_test() {
        let tickers = nyse_tickers();
        println!("{:#?}", tickers);
    }

    #[test]
    fn all_tickers_test() {
        let tickers = all_tickers();
        println!("{:#?}", tickers);
    }

    #[test]
    fn nasdaq_traded_test() {
        let tickers = nasdaqtraded();
        println!("{:#?}", tickers);
    }

    #[test]
    fn options_test() {
        let options = all_options();
        println!("{:#?}", options);
    }


}
