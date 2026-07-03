//! A minimal price-level orderbook.
//!
//! Prices are fixed-point integers (e.g. cents) so levels are exact keys and
//! ordering is total — no floating-point comparison issues.

use std::collections::BTreeMap;
use std::fmt;

/// Price in minor units (e.g. cents).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price(pub i64);

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0 as f64 / 100.0)
    }
}

/// Quantity in whole units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(pub u64);

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bid,
    Ask,
}

/// Aggregated price-level book: price -> total resting quantity.
#[derive(Debug, Default)]
pub struct Orderbook {
    bids: BTreeMap<Price, u64>,
    asks: BTreeMap<Price, u64>,
}

impl Orderbook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add quantity at a price level.
    pub fn add(&mut self, side: Side, price: Price, qty: Quantity) {
        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        *book.entry(price).or_insert(0) += qty.0;
    }

    /// Remove up to `qty` from a price level; clears the level if it empties.
    /// Returns the quantity actually removed.
    pub fn remove(&mut self, side: Side, price: Price, qty: Quantity) -> u64 {
        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        match book.get_mut(&price) {
            Some(level) => {
                let removed = qty.0.min(*level);
                *level -= removed;
                if *level == 0 {
                    book.remove(&price);
                }
                removed
            }
            None => 0,
        }
    }

    /// Highest bid, if any.
    pub fn best_bid(&self) -> Option<(Price, Quantity)> {
        self.bids
            .last_key_value()
            .map(|(&p, &q)| (p, Quantity(q)))
    }

    /// Lowest ask, if any.
    pub fn best_ask(&self) -> Option<(Price, Quantity)> {
        self.asks
            .first_key_value()
            .map(|(&p, &q)| (p, Quantity(q)))
    }

    /// Best ask minus best bid, in minor units.
    pub fn spread(&self) -> Option<i64> {
        Some(self.best_ask()?.0 .0 - self.best_bid()?.0 .0)
    }

    /// Midpoint of best bid/ask, in minor units.
    pub fn mid(&self) -> Option<f64> {
        Some((self.best_ask()?.0 .0 + self.best_bid()?.0 .0) as f64 / 2.0)
    }

    /// Bid levels, best (highest) first.
    pub fn bid_levels(&self) -> impl Iterator<Item = (Price, Quantity)> + '_ {
        self.bids.iter().rev().map(|(&p, &q)| (p, Quantity(q)))
    }

    /// Ask levels, best (lowest) first.
    pub fn ask_levels(&self) -> impl Iterator<Item = (Price, Quantity)> + '_ {
        self.asks.iter().map(|(&p, &q)| (p, Quantity(q)))
    }

    pub fn depth(&self) -> (usize, usize) {
        (self.bids.len(), self.asks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_and_spread() {
        let mut ob = Orderbook::new();
        ob.add(Side::Bid, Price(9_950), Quantity(100));
        ob.add(Side::Bid, Price(9_975), Quantity(50));
        ob.add(Side::Ask, Price(10_000), Quantity(75));
        ob.add(Side::Ask, Price(10_025), Quantity(200));

        assert_eq!(ob.best_bid(), Some((Price(9_975), Quantity(50))));
        assert_eq!(ob.best_ask(), Some((Price(10_000), Quantity(75))));
        assert_eq!(ob.spread(), Some(25));
        assert_eq!(ob.mid(), Some(9_987.5));
    }

    #[test]
    fn level_aggregation() {
        let mut ob = Orderbook::new();
        ob.add(Side::Bid, Price(100), Quantity(10));
        ob.add(Side::Bid, Price(100), Quantity(15));
        assert_eq!(ob.best_bid(), Some((Price(100), Quantity(25))));
    }

    #[test]
    fn remove_clears_empty_levels() {
        let mut ob = Orderbook::new();
        ob.add(Side::Ask, Price(100), Quantity(10));
        assert_eq!(ob.remove(Side::Ask, Price(100), Quantity(4)), 4);
        assert_eq!(ob.best_ask(), Some((Price(100), Quantity(6))));
        assert_eq!(ob.remove(Side::Ask, Price(100), Quantity(999)), 6);
        assert_eq!(ob.best_ask(), None);
        assert_eq!(ob.depth(), (0, 0));
    }

    #[test]
    fn empty_book() {
        let ob = Orderbook::new();
        assert_eq!(ob.best_bid(), None);
        assert_eq!(ob.best_ask(), None);
        assert_eq!(ob.spread(), None);
    }

    #[test]
    fn display_formats() {
        assert_eq!(Price(12_345).to_string(), "123.45");
        assert_eq!(Quantity(7).to_string(), "7x");
    }
}
