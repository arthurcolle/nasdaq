//! Nasdaq TotalView-ITCH 5.0 message parser and book builder.
//!
//! Parses the binary ITCH 5.0 protocol (big-endian, fixed-layout messages) as
//! specified by Nasdaq. Supports the standard length-prefixed file framing
//! (2-byte big-endian length before each message) used by daily sample files
//! (e.g. `emi.nasdaq.com` `*.NASDAQ_ITCH50` dumps) so full sessions can be
//! replayed into [`BookBuilder`].
//!
//! Implemented messages: S, R, H, Y, L, A, F, E, C, X, D, U, P, Q, B.
//! Unknown types are surfaced as [`Message::Unsupported`] with their tag so
//! replay never desyncs (framing carries the length).

use std::collections::HashMap;
use std::io::{self, Read};

use serde::Serialize;
use thiserror::Error;

use crate::orderbook::{Orderbook, Price, Side};

#[derive(Debug, Error)]
pub enum ItchError {
    #[error("message truncated: wanted {wanted} bytes, had {have} (tag {tag:?})")]
    Truncated { tag: char, wanted: usize, have: usize },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

/// Nanoseconds since midnight (ITCH timestamps are 48-bit).
pub type Ns = u64;

/// Buy/Sell indicator from ITCH ('B'/'S').
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ItchSide {
    Buy,
    Sell,
}

impl ItchSide {
    pub fn to_book_side(self) -> Side {
        match self {
            ItchSide::Buy => Side::Bid,
            ItchSide::Sell => Side::Ask,
        }
    }
}

/// A parsed ITCH 5.0 message (fields per the Nasdaq spec).
#[derive(Debug, Clone, Serialize)]
pub enum Message {
    /// 'S' — System Event.
    SystemEvent { ns: Ns, event: char },
    /// 'R' — Stock Directory.
    StockDirectory {
        ns: Ns,
        stock: String,
        market_category: char,
        financial_status: char,
        round_lot_size: u32,
        etp_flag: char,
    },
    /// 'H' — Stock Trading Action.
    TradingAction { ns: Ns, stock: String, state: char },
    /// 'Y' — Reg SHO Restriction.
    RegSho { ns: Ns, stock: String, action: char },
    /// 'L' — Market Participant Position.
    ParticipantPosition { ns: Ns, mpid: String, stock: String, state: char },
    /// 'A' — Add Order (no MPID).
    AddOrder {
        ns: Ns,
        order_id: u64,
        side: ItchSide,
        shares: u32,
        stock: String,
        price: u32,
    },
    /// 'F' — Add Order with MPID attribution.
    AddOrderMpid {
        ns: Ns,
        order_id: u64,
        side: ItchSide,
        shares: u32,
        stock: String,
        price: u32,
        mpid: String,
    },
    /// 'E' — Order Executed.
    OrderExecuted { ns: Ns, order_id: u64, shares: u32, match_id: u64 },
    /// 'C' — Order Executed With Price.
    OrderExecutedPrice {
        ns: Ns,
        order_id: u64,
        shares: u32,
        match_id: u64,
        printable: bool,
        price: u32,
    },
    /// 'X' — Order Cancel (partial).
    OrderCancel { ns: Ns, order_id: u64, shares: u32 },
    /// 'D' — Order Delete.
    OrderDelete { ns: Ns, order_id: u64 },
    /// 'U' — Order Replace.
    OrderReplace {
        ns: Ns,
        old_order_id: u64,
        new_order_id: u64,
        shares: u32,
        price: u32,
    },
    /// 'P' — Trade (non-cross, hidden liquidity).
    Trade {
        ns: Ns,
        order_id: u64,
        side: ItchSide,
        shares: u32,
        stock: String,
        price: u32,
        match_id: u64,
    },
    /// 'Q' — Cross Trade.
    CrossTrade { ns: Ns, shares: u64, stock: String, price: u32, cross_type: char },
    /// 'B' — Broken Trade.
    BrokenTrade { ns: Ns, match_id: u64 },
    /// Any message type we don't decode; length-framed replay skips it safely.
    Unsupported { tag: char },
}

impl Message {
    /// Stock symbol this message concerns, if any.
    pub fn stock(&self) -> Option<&str> {
        match self {
            Message::StockDirectory { stock, .. }
            | Message::TradingAction { stock, .. }
            | Message::RegSho { stock, .. }
            | Message::ParticipantPosition { stock, .. }
            | Message::AddOrder { stock, .. }
            | Message::AddOrderMpid { stock, .. }
            | Message::Trade { stock, .. }
            | Message::CrossTrade { stock, .. } => Some(stock),
            _ => None,
        }
    }
}

// --- field readers -----------------------------------------------------

struct Cur<'a> {
    buf: &'a [u8],
    pos: usize,
    tag: char,
}

impl<'a> Cur<'a> {
    fn new(tag: char, buf: &'a [u8]) -> Self {
        Cur { buf, pos: 0, tag }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], ItchError> {
        if self.pos + n > self.buf.len() {
            return Err(ItchError::Truncated {
                tag: self.tag,
                wanted: self.pos + n,
                have: self.buf.len(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, ItchError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, ItchError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, ItchError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    /// 48-bit big-endian timestamp.
    fn ns48(&mut self) -> Result<Ns, ItchError> {
        let b = self.take(6)?;
        Ok(((b[0] as u64) << 40)
            | ((b[1] as u64) << 32)
            | ((b[2] as u64) << 24)
            | ((b[3] as u64) << 16)
            | ((b[4] as u64) << 8)
            | (b[5] as u64))
    }
    fn ch(&mut self) -> Result<char, ItchError> {
        Ok(self.take(1)?[0] as char)
    }
    fn alpha(&mut self, n: usize) -> Result<String, ItchError> {
        Ok(String::from_utf8_lossy(self.take(n)?).trim_end().to_string())
    }
    fn side(&mut self) -> Result<ItchSide, ItchError> {
        Ok(match self.take(1)?[0] {
            b'B' => ItchSide::Buy,
            _ => ItchSide::Sell,
        })
    }
}

/// Parse one ITCH message body (starting at the tag byte).
pub fn parse(body: &[u8]) -> Result<Message, ItchError> {
    let tag = body[0] as char;
    // All messages start: tag(1) locate(2) tracking(2) timestamp(6)
    let mut c = Cur::new(tag, &body[1..]);
    let msg = match tag {
        'S' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::SystemEvent { ns, event: c.ch()? }
        }
        'R' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            let stock = c.alpha(8)?;
            let market_category = c.ch()?;
            let financial_status = c.ch()?;
            let round_lot_size = c.u32()?;
            let _round_lots_only = c.ch()?;
            let _issue_class = c.ch()?;
            let _issue_subtype = c.alpha(2)?;
            let _authenticity = c.ch()?;
            let _short_sale_thresh = c.ch()?;
            let _ipo_flag = c.ch()?;
            let _luld_tier = c.ch()?;
            let etp_flag = c.ch()?;
            Message::StockDirectory {
                ns,
                stock,
                market_category,
                financial_status,
                round_lot_size,
                etp_flag,
            }
        }
        'H' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            let stock = c.alpha(8)?;
            let state = c.ch()?;
            Message::TradingAction { ns, stock, state }
        }
        'Y' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            let stock = c.alpha(8)?;
            let action = c.ch()?;
            Message::RegSho { ns, stock, action }
        }
        'L' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            let mpid = c.alpha(4)?;
            let stock = c.alpha(8)?;
            let _primary = c.ch()?;
            let _mode = c.ch()?;
            let state = c.ch()?;
            Message::ParticipantPosition { ns, mpid, stock, state }
        }
        'A' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::AddOrder {
                ns,
                order_id: c.u64()?,
                side: c.side()?,
                shares: c.u32()?,
                stock: c.alpha(8)?,
                price: c.u32()?,
            }
        }
        'F' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::AddOrderMpid {
                ns,
                order_id: c.u64()?,
                side: c.side()?,
                shares: c.u32()?,
                stock: c.alpha(8)?,
                price: c.u32()?,
                mpid: c.alpha(4)?,
            }
        }
        'E' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::OrderExecuted {
                ns,
                order_id: c.u64()?,
                shares: c.u32()?,
                match_id: c.u64()?,
            }
        }
        'C' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::OrderExecutedPrice {
                ns,
                order_id: c.u64()?,
                shares: c.u32()?,
                match_id: c.u64()?,
                printable: c.ch()? == 'Y',
                price: c.u32()?,
            }
        }
        'X' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::OrderCancel { ns, order_id: c.u64()?, shares: c.u32()? }
        }
        'D' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::OrderDelete { ns, order_id: c.u64()? }
        }
        'U' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::OrderReplace {
                ns,
                old_order_id: c.u64()?,
                new_order_id: c.u64()?,
                shares: c.u32()?,
                price: c.u32()?,
            }
        }
        'P' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::Trade {
                ns,
                order_id: c.u64()?,
                side: c.side()?,
                shares: c.u32()?,
                stock: c.alpha(8)?,
                price: c.u32()?,
                match_id: c.u64()?,
            }
        }
        'Q' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::CrossTrade {
                ns,
                shares: c.u64()?,
                stock: c.alpha(8)?,
                price: c.u32()?,
                cross_type: c.ch()?,
            }
        }
        'B' => {
            let (_l, _t, ns) = (c.u16()?, c.u16()?, c.ns48()?);
            Message::BrokenTrade { ns, match_id: c.u64()? }
        }
        other => Message::Unsupported { tag: other },
    };
    Ok(msg)
}

/// Iterator over a length-prefixed ITCH stream (2-byte BE length framing).
pub struct Reader<R: Read> {
    inner: R,
}

impl<R: Read> Reader<R> {
    pub fn new(inner: R) -> Self {
        Reader { inner }
    }
}

impl<R: Read> Iterator for Reader<R> {
    type Item = Result<Message, ItchError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut len_buf = [0u8; 2];
        // Clean EOF on the first length byte -> end of stream.
        match self.inner.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return None,
            Err(e) => return Some(Err(e.into())),
        }
        let len = u16::from_be_bytes(len_buf) as usize;
        if len == 0 {
            return None;
        }
        let mut body = vec![0u8; len];
        if let Err(e) = self.inner.read_exact(&mut body) {
            return Some(Err(e.into()));
        }
        Some(parse(&body))
    }
}

/// Per-symbol book statistics accumulated during replay.
#[derive(Debug, Default, Clone, Serialize)]
pub struct SymbolStats {
    pub adds: u64,
    pub executes: u64,
    pub cancels: u64,
    pub deletes: u64,
    pub replaces: u64,
    pub trades: u64,
    pub volume: u64,
}

/// Builds per-symbol orderbooks from an ITCH message stream.
///
/// ITCH order ids are unique per session across all symbols, so a global
/// order-id -> symbol map routes execute/cancel/delete/replace messages.
#[derive(Debug, Default)]
pub struct BookBuilder {
    books: HashMap<String, Orderbook>,
    order_symbol: HashMap<u64, String>,
    pub stats: HashMap<String, SymbolStats>,
    /// Restrict building to one symbol (saves memory on full-session files).
    filter: Option<String>,
}

impl BookBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Only build the book for `symbol`; other messages are counted but skipped.
    pub fn for_symbol(symbol: impl Into<String>) -> Self {
        BookBuilder { filter: Some(symbol.into()), ..Self::default() }
    }

    pub fn book(&self, symbol: &str) -> Option<&Orderbook> {
        self.books.get(symbol)
    }

    pub fn books(&self) -> impl Iterator<Item = (&String, &Orderbook)> {
        self.books.iter()
    }

    fn wanted(&self, stock: &str) -> bool {
        self.filter.as_deref().is_none_or(|f| f == stock)
    }

    /// Apply one message to the books.
    pub fn apply(&mut self, msg: &Message) {
        match msg {
            Message::AddOrder { order_id, side, shares, stock, price, .. }
            | Message::AddOrderMpid { order_id, side, shares, stock, price, .. } => {
                if !self.wanted(stock) {
                    return;
                }
                let st = self.stats.entry(stock.clone()).or_default();
                st.adds += 1;
                self.order_symbol.insert(*order_id, stock.clone());
                let book = self.books.entry(stock.clone()).or_default();
                let _ = book.insert(
                    *order_id,
                    side.to_book_side(),
                    Price(*price as i64),
                    *shares as u64,
                );
            }
            Message::OrderExecuted { order_id, shares, .. }
            | Message::OrderExecutedPrice { order_id, shares, .. } => {
                if let Some(stock) = self.order_symbol.get(order_id).cloned() {
                    let st = self.stats.entry(stock.clone()).or_default();
                    st.executes += 1;
                    st.volume += *shares as u64;
                    if let Some(book) = self.books.get_mut(&stock) {
                        let _ = book.execute(*order_id, *shares as u64);
                        if book.order(*order_id).is_none() {
                            self.order_symbol.remove(order_id);
                        }
                    }
                }
            }
            Message::OrderCancel { order_id, shares, .. } => {
                if let Some(stock) = self.order_symbol.get(order_id).cloned() {
                    self.stats.entry(stock.clone()).or_default().cancels += 1;
                    if let Some(book) = self.books.get_mut(&stock) {
                        let _ = book.reduce(*order_id, *shares as u64);
                        if book.order(*order_id).is_none() {
                            self.order_symbol.remove(order_id);
                        }
                    }
                }
            }
            Message::OrderDelete { order_id, .. } => {
                if let Some(stock) = self.order_symbol.remove(order_id) {
                    self.stats.entry(stock.clone()).or_default().deletes += 1;
                    if let Some(book) = self.books.get_mut(&stock) {
                        let _ = book.cancel(*order_id);
                    }
                }
            }
            Message::OrderReplace { old_order_id, new_order_id, shares, price, .. } => {
                if let Some(stock) = self.order_symbol.remove(old_order_id) {
                    self.stats.entry(stock.clone()).or_default().replaces += 1;
                    if let Some(book) = self.books.get_mut(&stock) {
                        let _ = book.replace(
                            *old_order_id,
                            *new_order_id,
                            Price(*price as i64),
                            *shares as u64,
                        );
                    }
                    self.order_symbol.insert(*new_order_id, stock);
                }
            }
            Message::Trade { stock, shares, .. } => {
                if self.wanted(stock) {
                    let st = self.stats.entry(stock.clone()).or_default();
                    st.trades += 1;
                    st.volume += *shares as u64;
                }
            }
            _ => {}
        }
    }

    /// Replay an entire length-framed stream. Returns messages applied.
    pub fn replay<R: Read>(&mut self, reader: R) -> Result<u64, ItchError> {
        let mut n = 0;
        for msg in Reader::new(reader) {
            self.apply(&msg?);
            n += 1;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::analytics;

    // --- test message encoders (spec layouts) ---

    fn header(buf: &mut Vec<u8>, tag: u8, ns: u64) {
        buf.push(tag);
        buf.extend([0, 1]); // stock locate
        buf.extend([0, 0]); // tracking
        buf.extend(&ns.to_be_bytes()[2..]); // 48-bit timestamp
    }

    fn stock8(s: &str) -> [u8; 8] {
        let mut b = [b' '; 8];
        b[..s.len()].copy_from_slice(s.as_bytes());
        b
    }

    fn add_order(id: u64, side: u8, shares: u32, stock: &str, price: u32) -> Vec<u8> {
        let mut b = Vec::new();
        header(&mut b, b'A', 1_000);
        b.extend(id.to_be_bytes());
        b.push(side);
        b.extend(shares.to_be_bytes());
        b.extend(stock8(stock));
        b.extend(price.to_be_bytes());
        b
    }

    fn executed(id: u64, shares: u32) -> Vec<u8> {
        let mut b = Vec::new();
        header(&mut b, b'E', 2_000);
        b.extend(id.to_be_bytes());
        b.extend(shares.to_be_bytes());
        b.extend(77u64.to_be_bytes());
        b
    }

    fn delete(id: u64) -> Vec<u8> {
        let mut b = Vec::new();
        header(&mut b, b'D', 3_000);
        b.extend(id.to_be_bytes());
        b
    }

    fn replace(old: u64, new: u64, shares: u32, price: u32) -> Vec<u8> {
        let mut b = Vec::new();
        header(&mut b, b'U', 4_000);
        b.extend(old.to_be_bytes());
        b.extend(new.to_be_bytes());
        b.extend(shares.to_be_bytes());
        b.extend(price.to_be_bytes());
        b
    }

    fn frame(msgs: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for m in msgs {
            out.extend((m.len() as u16).to_be_bytes());
            out.extend(m);
        }
        out
    }

    #[test]
    fn parse_add_order() {
        let raw = add_order(42, b'B', 100, "AAPL", 1_750_000);
        let msg = parse(&raw).unwrap();
        match msg {
            Message::AddOrder { order_id, side, shares, stock, price, ns } => {
                assert_eq!(order_id, 42);
                assert_eq!(side, ItchSide::Buy);
                assert_eq!(shares, 100);
                assert_eq!(stock, "AAPL");
                assert_eq!(price, 1_750_000);
                assert_eq!(ns, 1_000);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn truncated_is_error_not_panic() {
        let raw = add_order(42, b'B', 100, "AAPL", 1_750_000);
        let err = parse(&raw[..12]).unwrap_err();
        assert!(matches!(err, ItchError::Truncated { tag: 'A', .. }));
    }

    #[test]
    fn unknown_tag_is_unsupported() {
        let msg = parse(&[b'z', 0, 0]).unwrap();
        assert!(matches!(msg, Message::Unsupported { tag: 'z' }));
    }

    #[test]
    fn framed_replay_builds_book() {
        // AAPL: bid 100@175.00, ask 50@175.10; MSFT: bid 10@300.00
        // ITCH prices are 1/10000 USD.
        let stream = frame(&[
            add_order(1, b'B', 100, "AAPL", 1_750_000),
            add_order(2, b'S', 50, "AAPL", 1_751_000),
            add_order(3, b'B', 10, "MSFT", 3_000_000),
            executed(1, 40),  // AAPL bid partially executed
            delete(3),        // MSFT bid gone
            replace(2, 4, 60, 1_750_500), // ask replaced tighter
        ]);

        let mut bb = BookBuilder::new();
        let n = bb.replay(stream.as_slice()).unwrap();
        assert_eq!(n, 6);

        let aapl = bb.book("AAPL").unwrap();
        let tob = analytics::top_of_book(aapl);
        assert_eq!(tob.bid.unwrap().qty, 60); // 100 - 40
        assert_eq!(tob.ask.unwrap().price, Price(1_750_500));
        assert_eq!(tob.ask.unwrap().qty, 60);
        assert_eq!(aapl.traded_volume, 40);

        // MSFT book exists but is empty after delete
        let msft = bb.book("MSFT").unwrap();
        assert_eq!(msft.depth(), (0, 0));

        let st = &bb.stats["AAPL"];
        assert_eq!(st.adds, 2);
        assert_eq!(st.executes, 1);
        assert_eq!(st.replaces, 1);
        assert_eq!(st.volume, 40);
    }

    #[test]
    fn symbol_filter_skips_others() {
        let stream = frame(&[
            add_order(1, b'B', 100, "AAPL", 1_750_000),
            add_order(2, b'B', 10, "MSFT", 3_000_000),
        ]);
        let mut bb = BookBuilder::for_symbol("AAPL");
        bb.replay(stream.as_slice()).unwrap();
        assert!(bb.book("AAPL").is_some());
        assert!(bb.book("MSFT").is_none());
    }

    #[test]
    fn reader_handles_clean_eof() {
        let stream = frame(&[add_order(1, b'B', 1, "X", 100)]);
        let msgs: Vec<_> = Reader::new(stream.as_slice()).collect();
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].is_ok());
    }
}
