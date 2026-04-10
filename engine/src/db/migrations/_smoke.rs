//! Smoke-test migration proving the `#[migration]` macro round-trips end-to-end
//! through `inventory`. Only compiled for tests; delete once a real migration
//! lives alongside it.

#![cfg(test)]

use frona_derive::migration;

#[migration("1970-01-01T00:00:00Z")]
fn smoke_noop_sql() -> &'static str {
    "SELECT 1;"
}
