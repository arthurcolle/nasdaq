//! Order-tracking limit orderbook with price-time priority matching.
//!
//! Two layers:
//! - [`Orderbook`]: full order tracking (add/cancel/replace/execute by order id),
//!   aggregated price levels, and a matching engine for incoming limit/market
//!   orders with price-time priority.
//! - [`analytics`]: microstructure measures over the book (spread, mid,
//!   microprice, imbalance, depth within N bps).
//!
//! Prices are fixed-point integers (e.g. cents, or ITCH 1/10000 dollars) so
//! levels are exact keys with total ordering.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt;

use serde::Serialize;

/// Price in minor units (cents by default; 1/10000 USD when fed from ITCH).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Price(pub i64);

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0 as f64 / 100.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }
}

/// A resting order.
#[derive(Debug, Clone, Serialize)]
pub struct Order {
    pub id: u64,
    pub side: Side,
    pub price: Price,
    pub qty: u64,
}

/// A fill produced by the matching engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fill {
    /// Resting order that was hit.
    pub maker_id: u64,
    /// Incoming order id (0 for anonymous market orders).
    pub taker_id: u64,
    pub price: Price,
    pub qty: u64,
}

/// Outcome of submitting an order to the matching engine.
#[derive(Debug, Clone, Serialize)]
pub struct Execution {
    pub fills: Vec<Fill>,
    /// Quantity that did not match. For limit orders this rests on the book;
    /// for market orders it is discarded.
    pub remaining: u64,
    /// Order id assigned to the resting remainder, if any.
    pub resting_id: Option<u64>,
}

#[derive(Debug, Error)]
pub enum BookError {
    #[error("order {0} not found")]
    UnknownOrder(u64),
    #[error("order id {0} already exists")]
    DuplicateOrder(u64),
    #[error("zero-quantity order rejected")]
    ZeroQuantity,
}

use thiserror::Error;

/// Aggregated view of one price level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Level {
    pub price: Price,
    pub qty: u64,
    pub orders: usize,
}

/// FIFO queue of order ids at one price.
#[derive(Debug, Default, Clone)]
struct LevelQueue {
    ids: VecDeque<u64>,
    total_qty: u64,
}

/// Order-tracking limit orderbook with price-time priority.
#[derive(Debug, Default)]
pub struct Orderbook {
    bids: BTreeMap<Price, LevelQueue>,
    asks: BTreeMap<Price, LevelQueue>,
    orders: HashMap<u64, Order>,
    next_id: u64,
    /// Cumulative traded volume through the matching engine.
    pub traded_volume: u64,
    /// Last trade price, if any.
    pub last_trade: Option<Price>,
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    fn book_mut(&mut self, side: Side) -> &mut BTreeMap<Price, LevelQueue> {
        match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        }
    }

    fn alloc_id(&mut self) -> u64 {
        loop {
            let id = self.next_id;
            self.next_id += 1;
            if !self.orders.contains_key(&id) {
                return id;
            }
        }
    }

    /// Number of live orders.
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    /// Look up a live order.
    pub fn order(&self, id: u64) -> Option<&Order> {
        self.orders.get(&id)
    }

    // ---------------------------------------------------------------
    // Passive book maintenance (feed-handler style, e.g. from ITCH)
    // ---------------------------------------------------------------

    /// Insert a resting order with a caller-assigned id (no matching).
    ///
    /// Caller-assigned ids share one id space with engine-allocated ids from
    /// [`Orderbook::limit`]. Inserting id N advances the internal allocator
    /// past N, but if you mix both styles, keep caller ids in a disjoint
    /// (e.g. high) range to avoid collisions with previously allocated ids.
    pub fn insert(&mut self, id: u64, side: Side, price: Price, qty: u64) -> Result<(), BookError> {
        if qty == 0 {
            return Err(BookError::ZeroQuantity);
        }
        if self.orders.contains_key(&id) {
            return Err(BookError::DuplicateOrder(id));
        }
        self.next_id = self.next_id.max(id + 1);
        self.orders.insert(id, Order { id, side, price, qty });
        let level = self.book_mut(side).entry(price).or_default();
        level.ids.push_back(id);
        level.total_qty += qty;
        Ok(())
    }

    /// Reduce an order's quantity (partial cancel / execution). Removes the
    /// order when it reaches zero. Returns the quantity actually removed.
    pub fn reduce(&mut self, id: u64, qty: u64) -> Result<u64, BookError> {
        let order = self.orders.get_mut(&id).ok_or(BookError::UnknownOrder(id))?;
        let removed = qty.min(order.qty);
        order.qty -= removed;
        let (side, price, empty) = (order.side, order.price, order.qty == 0);
        if let Some(level) = self.book_mut(side).get_mut(&price) {
            level.total_qty -= removed;
        }
        if empty {
            self.remove_from_level(id, side, price);
            self.orders.remove(&id);
        }
        Ok(removed)
    }

    /// Delete an order entirely. Returns the canceled quantity.
    pub fn cancel(&mut self, id: u64) -> Result<u64, BookError> {
        let order = self.orders.remove(&id).ok_or(BookError::UnknownOrder(id))?;
        if let Some(level) = self.book_mut(order.side).get_mut(&order.price) {
            level.total_qty -= order.qty;
        }
        self.remove_from_level(id, order.side, order.price);
        Ok(order.qty)
    }

    /// Replace an order: cancel `old_id`, insert `new_id` at a new price/qty.
    /// Time priority is lost, matching exchange semantics.
    pub fn replace(
        &mut self,
        old_id: u64,
        new_id: u64,
        price: Price,
        qty: u64,
    ) -> Result<(), BookError> {
        let side = self
            .orders
            .get(&old_id)
            .ok_or(BookError::UnknownOrder(old_id))?
            .side;
        self.cancel(old_id)?;
        self.insert(new_id, side, price, qty)
    }

    /// Execute quantity against a resting order (trade report), updating stats.
    pub fn execute(&mut self, id: u64, qty: u64) -> Result<u64, BookError> {
        let price = self
            .orders
            .get(&id)
            .ok_or(BookError::UnknownOrder(id))?
            .price;
        let done = self.reduce(id, qty)?;
        self.traded_volume += done;
        self.last_trade = Some(price);
        Ok(done)
    }

    fn remove_from_level(&mut self, id: u64, side: Side, price: Price) {
        let book = self.book_mut(side);
        if let Some(level) = book.get_mut(&price) {
            level.ids.retain(|&x| x != id);
            if level.ids.is_empty() {
                book.remove(&price);
            }
        }
    }

    // ---------------------------------------------------------------
    // Matching engine
    // ---------------------------------------------------------------

    /// Submit a limit order. Matches against the opposite side at prices that
    /// cross, price-time priority; any remainder rests on the book.
    pub fn limit(&mut self, side: Side, price: Price, qty: u64) -> Result<Execution, BookError> {
        if qty == 0 {
            return Err(BookError::ZeroQuantity);
        }
        let taker_id = self.alloc_id();
        let mut remaining = qty;
        let mut fills = Vec::new();

        while remaining > 0 {
            let best = match side {
                Side::Bid => self.asks.first_key_value().map(|(&p, _)| p),
                Side::Ask => self.bids.last_key_value().map(|(&p, _)| p),
            };
            let Some(best_price) = best else { break };
            let crosses = match side {
                Side::Bid => price.0 >= best_price.0,
                Side::Ask => price.0 <= best_price.0,
            };
            if !crosses {
                break;
            }
            remaining = self.consume_level(side.opposite(), best_price, remaining, taker_id, &mut fills);
        }

        let resting_id = if remaining > 0 {
            self.orders.insert(
                taker_id,
                Order { id: taker_id, side, price, qty: remaining },
            );
            let level = self.book_mut(side).entry(price).or_default();
            level.ids.push_back(taker_id);
            level.total_qty += remaining;
            Some(taker_id)
        } else {
            None
        };

        Ok(Execution { fills, remaining, resting_id })
    }

    /// Submit a market order: consumes liquidity until filled or book empty.
    pub fn market(&mut self, side: Side, qty: u64) -> Result<Execution, BookError> {
        if qty == 0 {
            return Err(BookError::ZeroQuantity);
        }
        let taker_id = self.alloc_id();
        let mut remaining = qty;
        let mut fills = Vec::new();
        while remaining > 0 {
            let best = match side {
                Side::Bid => self.asks.first_key_value().map(|(&p, _)| p),
                Side::Ask => self.bids.last_key_value().map(|(&p, _)| p),
            };
            let Some(best_price) = best else { break };
            remaining = self.consume_level(side.opposite(), best_price, remaining, taker_id, &mut fills);
        }
        Ok(Execution { fills, remaining, resting_id: None })
    }

    /// Consume up to `remaining` from the front of a level, FIFO.
    fn consume_level(
        &mut self,
        maker_side: Side,
        price: Price,
        mut remaining: u64,
        taker_id: u64,
        fills: &mut Vec<Fill>,
    ) -> u64 {
        loop {
            if remaining == 0 {
                return 0;
            }
            let front = {
                let book = self.book_mut(maker_side);
                let Some(level) = book.get_mut(&price) else { return remaining };
                let Some(&front) = level.ids.front() else {
                    book.remove(&price);
                    return remaining;
                };
                front
            };
            let maker_qty = self.orders[&front].qty;
            let traded = maker_qty.min(remaining);
            remaining -= traded;
            self.traded_volume += traded;
            self.last_trade = Some(price);
            fills.push(Fill { maker_id: front, taker_id, price, qty: traded });

            // reduce maker
            let order = self.orders.get_mut(&front).expect("maker exists");
            order.qty -= traded;
            let emptied = order.qty == 0;
            if emptied {
                self.orders.remove(&front);
            }
            let book = self.book_mut(maker_side);
            let level = book.get_mut(&price).expect("level exists");
            level.total_qty -= traded;
            if emptied {
                level.ids.pop_front();
                if level.ids.is_empty() {
                    book.remove(&price);
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // Views
    // ---------------------------------------------------------------

    pub fn best_bid(&self) -> Option<Level> {
        self.bids.last_key_value().map(|(&p, q)| Level {
            price: p,
            qty: q.total_qty,
            orders: q.ids.len(),
        })
    }

    pub fn best_ask(&self) -> Option<Level> {
        self.asks.first_key_value().map(|(&p, q)| Level {
            price: p,
            qty: q.total_qty,
            orders: q.ids.len(),
        })
    }

    /// Bid levels, best first, up to `depth`.
    pub fn bid_levels(&self, depth: usize) -> Vec<Level> {
        self.bids
            .iter()
            .rev()
            .take(depth)
            .map(|(&p, q)| Level { price: p, qty: q.total_qty, orders: q.ids.len() })
            .collect()
    }

    /// Ask levels, best first, up to `depth`.
    pub fn ask_levels(&self, depth: usize) -> Vec<Level> {
        self.asks
            .iter()
            .take(depth)
            .map(|(&p, q)| Level { price: p, qty: q.total_qty, orders: q.ids.len() })
            .collect()
    }

    pub fn depth(&self) -> (usize, usize) {
        (self.bids.len(), self.asks.len())
    }

    /// Verify internal consistency: every level's total equals the sum of its
    /// orders' quantities, every queued id exists, and the passive book is not
    /// crossed. Cheap enough for debug assertions in replay loops.
    pub fn check_invariants(&self) -> Result<(), String> {
        for (side_name, book) in [("bid", &self.bids), ("ask", &self.asks)] {
            for (price, level) in book {
                let mut sum = 0u64;
                for id in &level.ids {
                    let order = self
                        .orders
                        .get(id)
                        .ok_or_else(|| format!("{side_name} level {price:?} references unknown order {id}"))?;
                    if order.price != *price {
                        return Err(format!("order {id} price {:?} != level {price:?}", order.price));
                    }
                    sum += order.qty;
                }
                if sum != level.total_qty {
                    return Err(format!(
                        "{side_name} level {price:?} total {} != sum of orders {sum}",
                        level.total_qty
                    ));
                }
                if level.ids.is_empty() {
                    return Err(format!("{side_name} level {price:?} is empty but present"));
                }
            }
        }
        if let (Some(b), Some(a)) = (self.best_bid(), self.best_ask())
            && b.price.0 >= a.price.0
        {
            return Err(format!("book crossed: bid {:?} >= ask {:?}", b.price, a.price));
        }
        Ok(())
    }
}

/// Microstructure analytics over an [`Orderbook`].
pub mod analytics {
    use super::*;

    /// Snapshot of top-of-book measures.
    #[derive(Debug, Clone, Serialize)]
    pub struct TopOfBook {
        pub bid: Option<Level>,
        pub ask: Option<Level>,
        /// Ask - bid, minor units.
        pub spread: Option<i64>,
        /// Spread in basis points of the mid.
        pub spread_bps: Option<f64>,
        pub mid: Option<f64>,
        /// Size-weighted mid: (bid_px*ask_sz + ask_px*bid_sz)/(bid_sz+ask_sz).
        pub microprice: Option<f64>,
        /// (bid_sz - ask_sz)/(bid_sz + ask_sz) in [-1, 1].
        pub imbalance: Option<f64>,
    }

    pub fn top_of_book(ob: &Orderbook) -> TopOfBook {
        let bid = ob.best_bid();
        let ask = ob.best_ask();
        let (spread, spread_bps, mid, microprice, imbalance) = match (&bid, &ask) {
            (Some(b), Some(a)) => {
                let spread = a.price.0 - b.price.0;
                let mid = (a.price.0 + b.price.0) as f64 / 2.0;
                let denom = (b.qty + a.qty) as f64;
                let micro = (b.price.0 as f64 * a.qty as f64 + a.price.0 as f64 * b.qty as f64) / denom;
                let imb = (b.qty as f64 - a.qty as f64) / denom;
                let bps = if mid > 0.0 { spread as f64 / mid * 10_000.0 } else { 0.0 };
                (Some(spread), Some(bps), Some(mid), Some(micro), Some(imb))
            }
            _ => (None, None, None, None, None),
        };
        TopOfBook { bid, ask, spread, spread_bps, mid, microprice, imbalance }
    }

    /// Total resting quantity within `bps` basis points of the mid, per side.
    pub fn depth_within_bps(ob: &Orderbook, bps: f64) -> Option<(u64, u64)> {
        let tob = top_of_book(ob);
        let mid = tob.mid?;
        let band = mid * bps / 10_000.0;
        let bid_qty = ob
            .bid_levels(usize::MAX)
            .into_iter()
            .take_while(|l| (mid - l.price.0 as f64) <= band)
            .map(|l| l.qty)
            .sum();
        let ask_qty = ob
            .ask_levels(usize::MAX)
            .into_iter()
            .take_while(|l| (l.price.0 as f64 - mid) <= band)
            .map(|l| l.qty)
            .sum();
        Some((bid_qty, ask_qty))
    }
}

#[cfg(test)]
mod tests {
    use super::analytics::*;
    use super::*;

    fn seeded() -> Orderbook {
        let mut ob = Orderbook::new();
        ob.insert(101, Side::Bid, Price(9_975), 50).unwrap();
        ob.insert(102, Side::Bid, Price(9_950), 100).unwrap();
        ob.insert(201, Side::Ask, Price(10_000), 75).unwrap();
        ob.insert(202, Side::Ask, Price(10_025), 200).unwrap();
        ob
    }

    #[test]
    fn tob_and_analytics() {
        let ob = seeded();
        let t = top_of_book(&ob);
        assert_eq!(t.spread, Some(25));
        assert_eq!(t.mid, Some(9_987.5));
        // microprice = (9975*75 + 10000*50)/125 = 9985
        assert_eq!(t.microprice, Some(9_985.0));
        // imbalance = (50-75)/125 = -0.2
        assert_eq!(t.imbalance, Some(-0.2));
    }

    #[test]
    fn limit_crosses_with_price_time_priority() {
        let mut ob = seeded();
        // second ask at same price as best, added later -> behind 201
        ob.insert(203, Side::Ask, Price(10_000), 30).unwrap();
        let exec = ob.limit(Side::Bid, Price(10_000), 90).unwrap();
        assert_eq!(exec.remaining, 0);
        assert_eq!(exec.fills.len(), 2);
        assert_eq!(exec.fills[0], Fill { maker_id: 201, taker_id: exec.fills[0].taker_id, price: Price(10_000), qty: 75 });
        assert_eq!(exec.fills[1].maker_id, 203);
        assert_eq!(exec.fills[1].qty, 15);
        // 203 has 15 left
        assert_eq!(ob.order(203).unwrap().qty, 15);
        assert_eq!(ob.traded_volume, 90);
    }

    #[test]
    fn limit_rests_when_not_crossing() {
        let mut ob = seeded();
        let exec = ob.limit(Side::Bid, Price(9_990), 40).unwrap();
        assert!(exec.fills.is_empty());
        assert_eq!(exec.remaining, 40);
        let id = exec.resting_id.unwrap();
        assert_eq!(ob.best_bid().unwrap().price, Price(9_990));
        assert_eq!(ob.order(id).unwrap().qty, 40);
    }

    #[test]
    fn limit_partial_fill_rests_remainder() {
        let mut ob = seeded();
        let exec = ob.limit(Side::Bid, Price(10_000), 100).unwrap();
        assert_eq!(exec.fills.iter().map(|f| f.qty).sum::<u64>(), 75);
        assert_eq!(exec.remaining, 25);
        // remainder now best bid at 10000
        assert_eq!(ob.best_bid().unwrap().price, Price(10_000));
        assert_eq!(ob.best_ask().unwrap().price, Price(10_025));
    }

    #[test]
    fn market_walks_the_book() {
        let mut ob = seeded();
        let exec = ob.market(Side::Bid, 150).unwrap();
        assert_eq!(exec.fills.iter().map(|f| f.qty).sum::<u64>(), 150);
        assert_eq!(exec.fills[0].price, Price(10_000));
        assert_eq!(exec.fills[1].price, Price(10_025));
        assert_eq!(ob.best_ask().unwrap().qty, 125);
    }

    #[test]
    fn market_on_empty_book_returns_remainder() {
        let mut ob = Orderbook::new();
        let exec = ob.market(Side::Ask, 10).unwrap();
        assert!(exec.fills.is_empty());
        assert_eq!(exec.remaining, 10);
    }

    #[test]
    fn cancel_replace_execute() {
        let mut ob = seeded();
        assert_eq!(ob.cancel(102).unwrap(), 100);
        assert!(ob.order(102).is_none());
        ob.replace(101, 301, Price(9_960), 60).unwrap();
        assert_eq!(ob.best_bid().unwrap().price, Price(9_960));
        assert_eq!(ob.execute(301, 25).unwrap(), 25);
        assert_eq!(ob.order(301).unwrap().qty, 35);
        assert_eq!(ob.traded_volume, 25);
        assert_eq!(ob.last_trade, Some(Price(9_960)));
    }

    #[test]
    fn errors() {
        let mut ob = Orderbook::new();
        assert!(matches!(ob.cancel(9), Err(BookError::UnknownOrder(9))));
        ob.insert(1, Side::Bid, Price(1), 1).unwrap();
        assert!(matches!(
            ob.insert(1, Side::Bid, Price(1), 1),
            Err(BookError::DuplicateOrder(1))
        ));
        assert!(matches!(
            ob.limit(Side::Bid, Price(1), 0),
            Err(BookError::ZeroQuantity)
        ));
    }

    #[test]
    fn invariants_hold_under_random_ops() {
        // Deterministic xorshift so the test is reproducible without rand dep.
        let mut state = 0x2545F4914F6CDD1Du64;
        let mut rng = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut ob = Orderbook::new();
        let mut live: Vec<u64> = Vec::new();
        // Caller-assigned ids in a high range so they never collide with
        // engine-allocated resting-order ids (see insert() docs).
        let mut next_id = 1_000_000_000u64;
        for i in 0..20_000 {
            match rng() % 100 {
                0..=44 => {
                    next_id += 1;
                    while ob.order(next_id).is_some() {
                        next_id += 1;
                    }
                    let side = if rng() % 2 == 0 { Side::Bid } else { Side::Ask };
                    // keep sides in non-crossing bands so passive inserts stay sane
                    let price = match side {
                        Side::Bid => Price(9_000 + (rng() % 900) as i64),
                        Side::Ask => Price(10_000 + (rng() % 900) as i64),
                    };
                    let qty = 1 + rng() % 500;
                    ob.insert(next_id, side, price, qty).unwrap();
                    live.push(next_id);
                }
                45..=64 => {
                    if !live.is_empty() {
                        let id = live[(rng() % live.len() as u64) as usize];
                        let _ = ob.reduce(id, 1 + rng() % 200);
                        if ob.order(id).is_none() {
                            live.retain(|&x| x != id);
                        }
                    }
                }
                65..=79 => {
                    if !live.is_empty() {
                        let idx = (rng() % live.len() as u64) as usize;
                        let id = live.swap_remove(idx);
                        let _ = ob.cancel(id);
                    }
                }
                80..=89 => {
                    let side = if rng() % 2 == 0 { Side::Bid } else { Side::Ask };
                    let price = match side {
                        Side::Bid => Price(9_500 + (rng() % 1_000) as i64),
                        Side::Ask => Price(9_400 + (rng() % 1_000) as i64),
                    };
                    if let Ok(exec) = ob.limit(side, price, 1 + rng() % 300) {
                        for f in &exec.fills {
                            if ob.order(f.maker_id).is_none() {
                                live.retain(|&x| x != f.maker_id);
                            }
                        }
                        if let Some(id) = exec.resting_id {
                            live.push(id);
                        }
                    }
                }
                _ => {
                    let side = if rng() % 2 == 0 { Side::Bid } else { Side::Ask };
                    if let Ok(exec) = ob.market(side, 1 + rng() % 300) {
                        for f in &exec.fills {
                            if ob.order(f.maker_id).is_none() {
                                live.retain(|&x| x != f.maker_id);
                            }
                        }
                    }
                }
            }
            if i % 1_000 == 0 {
                ob.check_invariants().unwrap_or_else(|e| panic!("iter {i}: {e}"));
            }
        }
        ob.check_invariants().unwrap();
        assert_eq!(ob.order_count(), live.len());
    }

    #[test]
    fn depth_within_band() {
        let ob = seeded();
        // mid 9987.5; 20bps band ~= 19.975 -> includes 9975 bid (12.5 away)
        // and 10000 ask (12.5 away), excludes 9950 and 10025 (37.5 away)
        let (b, a) = depth_within_bps(&ob, 20.0).unwrap();
        assert_eq!(b, 50);
        assert_eq!(a, 75);
    }
}
