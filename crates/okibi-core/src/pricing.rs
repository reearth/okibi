//! What a unit of resource costs. See `spec/okibi-contract.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const PRICING_VERSION: &str = "okibi-pricing/1";

/// Unit prices for one profile in one month.
///
/// Append-only: a price change is a new file, because editing an old one makes
/// every estimate that cites it unreproducible. A plan records the hash of the
/// table it used so that an old estimate stays checkable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingTable {
    pub pricing: String,
    /// Matched against a manifest's `billing.pricing_profile`.
    pub profile: String,
    /// The month these prices are for, as `YYYY-MM`.
    pub effective: String,
    pub currency: String,
    /// Where these prices were read from.
    ///
    /// Required, because the failure this format is most exposed to is a
    /// plausible number nobody can check. A table that cannot say where it
    /// came from is a table someone remembered.
    pub source: Vec<String>,
    /// The day the source was read, as `YYYY-MM-DD`.
    pub retrieved: String,
    /// The price of one of each resource a manifest counts.
    pub units: BTreeMap<String, f64>,
}

/// The two resource names okibi itself knows, because a digest measures them.
///
/// Every other name is the vendor's and lives only in the two files that use
/// it — which is what lets a service bill for something this crate has never
/// heard of.
pub mod unit {
    /// Falls back to the digest's measured generation time.
    pub const CPU_MS: &str = "cpu_ms";
    /// Falls back to the digest's measured response size.
    pub const EGRESS_BYTE: &str = "egress_byte";
}

impl PricingTable {
    /// The price of one unit, or zero if this table does not price it.
    ///
    /// Zero rather than an error: a table that says nothing about egress is
    /// saying egress is free here, which for R2 it is, and an estimate that
    /// refused to be computed would be less useful than one that is honest
    /// about what it counted.
    pub fn unit(&self, name: &str) -> f64 {
        self.units.get(name).copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> PricingTable {
        serde_json::from_str(
            r#"{"pricing":"okibi-pricing/1","profile":"cloudflare","effective":"2026-08",
                "currency":"USD",
                "source":["https://developers.cloudflare.com/workers/platform/pricing/"],
                "retrieved":"2026-08-25",
                "units":{"cpu_ms":0.00000002}}"#,
        )
        .unwrap()
    }

    #[test]
    fn prices_what_it_knows_and_zero_for_the_rest() {
        assert_eq!(table().unit(unit::CPU_MS), 0.00000002);
        assert_eq!(table().unit(unit::EGRESS_BYTE), 0.0);
    }

    /// A table that cannot say where its numbers came from is a table someone
    /// remembered, and this format's whole job is to be checkable years later.
    #[test]
    fn refuses_a_table_with_no_source() {
        let sourceless = r#"{"pricing":"okibi-pricing/1","profile":"cloudflare",
            "effective":"2026-08","currency":"USD","units":{"cpu_ms":0.00000002}}"#;
        assert!(serde_json::from_str::<PricingTable>(sourceless).is_err());
    }
}
