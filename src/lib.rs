use ftp::{FtpStream};
use polars::prelude::*;

use std::collections::BTreeMap;

pub fn tickers() -> Vec<String> {
    // let files = ["nasdaqlisted.txt", "otherlisted.txt", "options.txt"];
    let files = ["nasdaqtraded.txt"];
    let tickers: Vec<String> = Vec::new();
    for file in files.iter() {
        let mut ftp_stream = FtpStream::connect("ftp.nasdaqtrader.com:21").unwrap_or_else(|err|
            panic!("{}", err)
        );
        
        // Download  the nasdaqlisted and otherlisted txt files from the directory...
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
                col_data.get_mut(header).unwrap().push(data.to_string());
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
        // write df to csv
        let mut ofile = std::fs::File::create(format!("{}.csv", file)).unwrap();
        CsvWriter::new(&mut ofile).finish(&mut df).unwrap();
        let symbols = df.column("Symbol").unwrap().clone();
        println!("Symbols: {:?}", symbols);
    }
    return tickers;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
