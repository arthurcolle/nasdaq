# ROADMAP — nasdaq: from toolkit to useful library

**Thesis.** Point solutions already exist and are good: `itchy` does fast zero-alloc
ITCH parsing; `orderbook-rs` does lock-free price levels. Nobody owns the
*integrated market-structure pipeline*: symbol universe → session replay →
book state → microstructure analytics → strategy backtest, in one coherent,
correct, documented crate. That is the niche. We win on **integration and
correctness**, then close the performance gap — not the other way around.

**Positioning sentence.** "Load a Nasdaq session file and be asking
microstructure questions in five lines."

---

## Phase 0 — Table stakes (make it real)

*Goal: a stranger can find, trust, and adopt the crate.*

| # | Deliverable | Detail | Acceptance |
|---|---|---|---|
| 0.1 | CI | GitHub Actions: build + test + clippy + fmt on stable/beta/MSRV, macOS + Linux | green badge |
| 0.2 | MSRV policy | pin & test minimum supported rustc (target: 1.85, matrix's toolchain) | CI job |
| 0.3 | crates.io publication | check `nasdaq` name availability; fallback: `nasdaq-tools`, `totalview`, `itch50` | `cargo add` works |
| 0.4 | docs.rs | doc comments on every public item, `#![deny(missing_docs)]`, examples in rustdoc that compile (doctests) | docs.rs renders, 0 missing |
| 0.5 | Examples dir | `examples/replay_session.rs`, `examples/top_of_book.rs`, `examples/symbol_universe.rs` | `cargo run --example` works |
| 0.6 | Semver discipline | CHANGELOG.md, 0.x → API review before 1.0 | changelog exists |
| 0.7 | License + contrib files | MIT LICENSE file (declared but missing), CONTRIBUTING.md | files present |

**Effort:** 1–2 days. **This phase is non-negotiable — without it nothing else matters.**

---

## Phase 1 — Correctness proof (the trust moat)

*Goal: demonstrably correct against real Nasdaq data. This is the differentiator
vs. speed-first crates.*

| # | Deliverable | Detail | Acceptance |
|---|---|---|---|
| 1.1 | Real-session validation | download one official sample from `emi.nasdaq.com/ITCH/Nasdaq ITCH/` (~5GB gz); replay the full day; zero parse errors; message-type counts match published stats | integration test (ignored, `--features live-data`) |
| 1.2 | Gzip streaming | `flate2`-gated feature so `*.NASDAQ_ITCH50.gz` replays directly without a 30GB unpack | `nasdaq itch file.gz` works |
| 1.3 | Book invariant checks | debug-mode assertions: level qty == Σ order qtys, no crossed book after passive ops, order_symbol map consistent | `cargo test` w/ invariants feature |
| 1.4 | Cross-validation vs `itchy` | parse same file with both crates, diff decoded fields on 10M messages | divergence report = empty |
| 1.5 | Fuzzing | `cargo-fuzz` targets: `itch::parse`, framed `Reader`, directory `Table::parse` | 24h fuzz, no panics |
| 1.6 | Property tests | proptest: random op sequences on `Orderbook` — matching conserves qty, price-time priority holds, cancel/replace idempotency | in CI |
| 1.7 | Remaining ITCH messages | I (NOII), N (RPII), K (IPO quoting), J (LULD auction collar), h (operational halt), O (direct listing) — currently `Unsupported` | full 5.0 coverage table in README |

**Effort:** 3–5 days. **Output artifact:** "Validated against full Nasdaq session
YYYY-MM-DD: N messages, 0 errors" in README — that one line is the adoption trigger.

---

## Phase 2 — Performance (close the gap honestly)

*Goal: fast enough that speed is never a reason to leave. Benchmark, publish
numbers, don't overclaim.*

| # | Deliverable | Detail | Acceptance |
|---|---|---|---|
| 2.1 | Criterion benches | parse throughput (msgs/sec), replay throughput (msgs/sec incl. book), book ops (adds/cancels per sec) | `benches/` in CI (no-run check) |
| 2.2 | Zero-copy parse path | `Message` borrows `&[u8]` for stock/mpid (Cow or lifetime variant `MessageRef<'a>`); today's String-allocating path stays as the ergonomic API | ≥5x parse throughput vs 0.3.0 |
| 2.3 | Symbol interning | `stock locate` (u16, already in every header) → intern table from 'R' directory messages; kills per-message String on the hot path | replay ≥2x |
| 2.4 | Book micro-opts | reserve level queues, arena for orders (slab), avoid retain() on cancel via tombstones or index map | adds/cancels ≥3x |
| 2.5 | Mmap replay | memory-map uncompressed files; chunked streaming for gz | full-day replay < 60s on M2 Max |
| 2.6 | Published numbers table | honest README table vs `itchy` on same hardware/file | table in README |

**Target:** ≥10M msgs/sec parse-only, ≥2M msgs/sec full book-building replay.
**Effort:** 4–6 days.

---

## Phase 3 — Analytics depth (the reason to stay)

*Goal: the questions a quant actually asks of a session, answerable in one call.*

| # | Deliverable | Detail |
|---|---|---|
| 3.1 | Time-series sampling | `Sampler` that snapshots TopOfBook every N ms of session time (ITCH ns timestamps) → Vec<(ns, TopOfBook)>; CSV/JSON export |
| 3.2 | Trading-state awareness | consume 'H'/'Y' messages: halt/pause windows annotated, analytics exclude halted periods; LULD auction handling |
| 3.3 | Event studies | book state ±N ms around trades / around imbalance thresholds; realized spread, effective spread, price impact (5s/30s/5min horizons) |
| 3.4 | Queue analytics | queue position estimator for a hypothetical order (shares ahead at level over time), fill-probability curves — **directly feeds market-maker sizing** |
| 3.5 | Volume profiles | per-symbol volume by price, by time bucket; VWAP; participation curves |
| 3.6 | Session summary | one-call `SessionReport`: OHLC from trades, volume, spread distribution (p50/p90/p99), quote-to-trade ratio, cancel ratio, top movers |
| 3.7 | Export surface | everything serializable; optional `arrow`/`parquet` feature for handoff to Python/polars — meet the quant researcher where they live |

**Effort:** 5–8 days. This phase is what makes it *useful* rather than *correct and fast*.

---

## Phase 4 — Backtesting substrate (the strategic payoff)

*Goal: replay-driven strategy evaluation. This is where the library connects to
revenue (market-making parameter tuning).*

| # | Deliverable | Detail |
|---|---|---|
| 4.1 | `Strategy` trait | callbacks: `on_book_update`, `on_trade`, `on_timer(ns)`; submits limit/cancel through a simulated gateway |
| 4.2 | Fill simulation | strategy orders join real replayed queues (queue-position model from 3.4); conservative + optimistic fill models |
| 4.3 | Latency model | configurable feed + order latency (ns); orders see the book as of t-λ |
| 4.4 | P&L accounting | position, cash, realized/unrealized, inventory penalties, fees |
| 4.5 | Reference strategy | Avellaneda-Stoikov market maker as `examples/avellaneda.rs` — ports the KalshiMarketMaker math onto real Nasdaq microstructure for parameter calibration (γ, k, σ estimation from replayed data) |
| 4.6 | dsco integration | expose replay/backtest as a dsco tool; session reports into the market-intel pipeline |

**Effort:** 8–12 days. **Revenue link:** σ and k in Avellaneda-Stoikov are estimated
from exactly the data this produces; calibrated parameters transfer to the Kalshi MM.

---

## Phase 5 — Live data (only if demanded)

*Deliberately last: real-time distribution of TotalView requires Nasdaq licensing;
most users only ever need historical replay.*

| # | Deliverable | Detail |
|---|---|---|
| 5.1 | MoldUDP64 | Nasdaq's UDP framing (sequence numbers, gap detection, retransmission requests) — feature-gated |
| 5.2 | SoupBinTCP | TCP session protocol (login, sequenced messages, heartbeats) — feature-gated |
| 5.3 | Gap-fill + recovery | snapshot + incremental reconciliation on late join |
| 5.4 | Other ITCH dialects | BX/PSX use the same 5.0 format; Nordic/Baltic ITCH differs — dialect enum if requested |

**Effort:** 10+ days. Gate on actual user demand (GitHub issues).

---

## Sequencing & decision points

```
Phase 0 ──► Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► (Phase 5?)
 2 days      5 days      6 days      8 days      12 days      on demand
             │                        │
             └─ publish 0.4.0         └─ publish 0.6.0 "analytics"
                "validated"              ▲ 1.0 candidate after API review
```

- **Publish early, version honestly.** 0.4 after Phase 1, 0.5 after Phase 2,
  0.6 after Phase 3; 1.0 only after the API survives Phase 4's dogfooding.
- **Kill criteria.** If Phase 1's real-session validation finds structural spec
  misreads, fix before any performance work — correctness debt compounds.
- **Non-goals.** Options/multi-asset books, FIX, order routing, anything
  requiring exchange agreements. Equities microstructure replay, done well.

## Success metrics (12 weeks)

| Metric | Target |
|---|---|
| crates.io downloads | 500+ |
| GitHub stars | 50+ |
| Full-session replay | < 60s, 0 parse errors |
| docs.rs coverage | 100% public items |
| External issue/PR | ≥ 3 (signal of real users) |
| Internal use | Kalshi MM parameters calibrated via Phase 4 |
