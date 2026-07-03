# Changelog

## 0.3.0 (unreleased)

### Added
- **ITCH 5.0 parser** (`itch` module): 15 message types, 48-bit timestamps,
  length-framed `Reader` iterator, truncation-safe errors.
- **`BookBuilder`**: replays sessions into per-symbol orderbooks with
  add/exec/cancel/delete/replace/trade stats; optional single-symbol filter.
- **`open_session`**: transparent gzip decompression for `*.NASDAQ_ITCH50.gz`
  (`gz` feature, on by default).
- **Matching engine**: order tracking by id, price-time priority FIFO levels,
  `limit`/`market` submission producing `Fill` records.
- **Analytics**: `top_of_book` (spread abs/bps, mid, microprice, imbalance),
  `depth_within_bps`.
- **TTL disk cache**: `Client::with_cache(dir, ttl)`.
- **CLI**: `itch` subcommand (table/JSON), `fetch --json`, `--cache`.
- CI (GitHub Actions), examples, MIT LICENSE file.

### Changed
- `orderbook` rewritten around order ids; old aggregate-only `add` API removed.

## 0.2.0

- Edition 2024; typed `Client`/`Table` API with thiserror errors.
- FTP with HTTPS fallback (`Transport::Auto`).
- Dropped polars/reqwest/yahoo-finance/ftp for suppaftp/ureq/csv/clap.
- Real orderbook (fixed-point prices), CLI, offline tests.

## 0.1.0

- Original FTP symbol fetcher (panic-based, polars DataFrames).
