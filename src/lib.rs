use ftp::{FtpStream};
use polars::prelude::*;

use std::collections::BTreeMap;
pub fn nasdaq_tickers() -> Vec<String> {
    return tickers(["nasdaqlisted.txt".to_string()].to_vec(), "Symbol".to_string());
}

pub fn nyse_tickers() -> Vec<String> {
    return tickers(["otherlisted.txt".to_string()].to_vec(), "ACT Symbol".to_string());
}

pub fn all_tickers() -> Vec<String> {
    let nt = nasdaq_tickers();
    let ny = nyse_tickers();
    let tickers = ny.into_iter().chain(nt.into_iter()).collect();
    return tickers;
}

pub fn nasdaqtraded() -> Vec<String> {
    return tickers(["nasdaqtraded.txt".to_string()].to_vec(), "Symbol".to_string());
}

pub fn bonds() -> Vec<String> {
    return tickers(["bondslist.txt".to_string()].to_vec(), "Symbol".to_string());
}

pub fn tickers(files: Vec<String>, col: String) -> Vec<String> {
    let tickers: Vec<String> = Vec::new();
    for file in files.iter() {
        let mut ftp_stream = FtpStream::connect("ftp.nasdaqtrader.com:21").unwrap_or_else(|err|
            panic!("{}", err)
        );
        
        // Download the nasdaqlisted and otherlisted txt files from the directory...
        let _ = ftp_stream.login("anonymous", "anonymous").unwrap_or_else(|err|
            panic!("{}", err)
        );
        
        let _ = ftp_stream.cwd("symboldirectory").unwrap_or_else(|err|
            panic!("{}", err)
        );
        
        let in_memory_file = ftp_stream.simple_retr(file).unwrap_or_else(|err|
            panic!("{}", err)
        );

        let lines = in_memory_file.into_inner();

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
                col_data.get_mut(header).unwrap().push(data.replace("\\", "").replace("\"", "").to_string());
            });

            // let row = row.collect::<Vec<&str>>();
            // println!("{:?}", row);
        });

        let vecs = col_data.iter().map(|(key, value)| { 
            Series::new(key, value)
            // let x = 
            // println!("Series {} length is {}...", key, x.len());
            // println!("Series {} last element is {}", key, value.last().unwrap());
            // x
        }).collect();

        let mut df = DataFrame::new(vecs).unwrap();
        println!("DataFrame length is {}...", df.height());
        println!("DataFrame last row is {:?}", df.get_row(df.height() - 1).unwrap());
        // first 15 rows shown without head
        println!("DataFrame first 15 rows are {:?}", df);

        // write df to csv
        let mut ofile = std::fs::File::create(format!("{}.csv", file)).unwrap();
        CsvWriter::new(&mut ofile).finish(&mut df).unwrap();
        // let good_headers = col_headers.clone().filter(|header| {
        //     header.contains("Symbol")
        // });
        let symbols = df.column(&col).unwrap().clone().into_frame();
        // turn symbols DataFrame into Vec<String> without using into_iter()
        
        let symbols = symbols.column(&col).unwrap().clone().iter().map(|x| x.to_string()).collect::<Vec<String>>();

        println!("Symbols: {:?}", symbols);
    }
    return tickers;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonds_test() {
        let tickers: Vec<String> = bonds();
        assert!(tickers.contains(&"AAPL42".to_string()));
    }

    #[test]
    fn nasdaq_tickers_test() {
        let tickers = nasdaq_tickers();
        println!("{:?}", tickers);
        assert!(tickers.contains(&"AMZN".to_string()));
        assert!(tickers.contains(&"TSLA".to_string()));
        assert!(tickers.contains(&"AAPL".to_string()));
        assert!(tickers.contains(&"ABNB".to_string()));
    }

    #[test]
    fn nyse_tickers_test() {
        let tickers = nyse_tickers();
        assert!(tickers.contains(&"GS".to_string()));
        assert!(tickers.contains(&"BRK.A".to_string()));
        assert!(tickers.contains(&"C".to_string()));
        assert!(tickers.contains(&"WMT".to_string()));
    }

    #[test]
    fn all_tickers_test() {
        let tickers = all_tickers();
        assert!(tickers.contains(&"TSLA".to_string()));
        assert!(tickers.contains(&"ZYXI".to_string()));
        assert!(tickers.contains(&"GS".to_string()));
    }

    #[test]
    fn nasdaq_traded_test() {
        let tickers = nasdaqtraded();
        assert!(tickers.contains(&"TSLA".to_string()));
        assert!(tickers.contains(&"ZYXI".to_string()));
        assert!(tickers.contains(&"GS".to_string()));
    }
}
