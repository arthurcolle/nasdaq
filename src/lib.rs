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

// struct Ticker {
//     symbols: BTreeMap<SymbolType, String>,
//     description: String,
//     exchange: Exchange,
//     etf: bool,
//     active: bool,
//     test: bool,
// }

struct Ticker {
    symbol: String,
    description: String,
    exchange: Exchange,
    etf: bool,
    active: bool,
    test: bool,
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

// pub fn bonds() -> Vec<String> {
//   return tickers("bondslist.txt".to_string(), "Symbol".to_string());
//}


pub fn all_options() -> DataFrame {
    let mut options = tickers_df("options.txt".to_string());
    options.add_column("oid", &[1]);

    // let n: u32 = 0b11110000;
    // 0 indicates pad with zeros
    // 8 is the target width
    // b indicates to format as binary
    // let formatted = format!("{:08b}", n);
    // options.add_column("Formatted Strike Price", &[
    //     options.column("Explicit Strike Price").unwrap().to_vec().map(|x: &f64| format!("{:08b}", x)).to_series()
    // ]).unwrap();
    // println!("{:#?}", options);

    // options.add_column("oid", &[
    //     options.column("Root Symbol").unwrap(), 
    //     options.column("Expiration Date").unwrap().map(|xd: &Timestamp| {
    //         let xd = xd.date();
    //         format!("{:02}{:02}{:02}", xd.year() % 100, xd.month(), xd.day())
    //     }), 
    //     options.column("Options Type").unwrap(),
    //     options.column("Formatted Strike Price").unwrap().map(|x: &f64| x.trunc().to_string())
    // ]).unwrap();

    // let options_final = options.add_column("oid", &[
    //     options.column("Root Symbol"), 
    //     options.column("Expiration Date").map(|xd: &Timestamp| {
    //         let xd = xd.date();
    //         format!("{:02}{}{:02}", xd.year() % 100, xd.month(), xd.day())
    //     }), 
    //     options.column("Options Type"), 
    //     options.column("Formatted Strike Price").map(|x: &f64| x.trunc().to_string())
    // ]);

    // write new options file to a new file called options_oids.csv

    // add a new column called abc where every row is 1
    options.add_column("help", &[1]).unwrap();


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

    //#[test]
    //fn bonds_test() {
    //    let tickers: Vec<String> = bonds();
    //    println!("{:#?}", tickers);
    //}

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
