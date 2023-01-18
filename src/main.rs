// import library from lib.rs
extern crate nasdaq;
use polars::frame::DataFrame;

fn main() {
    // call function from lib.rs
    let nasdaq_tkrs: Vec<String> = nasdaq::nasdaq_tickers();
    let nyse_tkrs: Vec<String> = nasdaq::nyse_tickers();

    // combine nasaq and nyse tickers
    let combined_tickers: Vec<String> = nyse_tkrs.to_vec().into_iter().chain(nasdaq_tkrs.into_iter()).collect();
    println!("<Tickers, sep=\" \">");
    for ticker in combined_tickers {
      // no new line after each ticker
      print!("{} ", ticker);
    }
    println!("</Tickers>");
    println!("<Options>");
    let option_vec: DataFrame = nasdaq::all_options();
    println!("{:#?}", option_vec);
    println!("</Options>");
}