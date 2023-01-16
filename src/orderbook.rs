use std::collections::BTreeMap;

use chrono::{NaiveDateTime, Utc};

struct Order {
    creation_date: NaiveDateTime
}

struct Price {
    value: f64
}

struct Quantity {
    value: f64
}

struct Orderbook {
    bids: BTreeMap<Order, BTreeMap<Price, Quantity>>,
    asks: BTreeMap<Order, BTreeMap<Price, Quantity>>
}

// Implement the orderbook
impl Orderbook {
    fn new() -> Orderbook {
        Orderbook {
            bids: BTreeMap::new(),
            asks: BTreeMap::new()
        }
    }

    fn add_bid(&mut self, price: Price, quantity: Quantity) {
        let order = Order {
            creation_date: Utc::now().naive_utc()
        };
        self.bids.entry(order).or_insert(BTreeMap::new()).insert(price, quantity);
    }

    fn add_ask(&mut self, price: Price, quantity: Quantity) {
        let order = Order {
            creation_date: Utc::now().naive_utc()
        };
        self.asks.entry(order).or_insert(BTreeMap::new()).insert(price, quantity);
    }
}

// Implement derive for Ord for Order
impl Ord for Order {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.creation_date.cmp(&other.creation_date)
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value as f64 / 100.0)
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x", self.value)
    }
}