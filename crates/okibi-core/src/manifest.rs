//! What a service says about itself. See `spec/okibi-contract.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const MANIFEST_VERSION: &str = "okibi-service/1";

/// Everything okibi is allowed to know about a service.
///
/// There is no other channel: what is not here or in a digest is not something
/// the planner can act on. That is what lets warming stay outside the services
/// it warms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceManifest {
    pub manifest: String,
    pub service: String,
    /// Substituting `{tileset}`, `{id}` and `{epoch.*}` rebuilds a URL that
    /// regenerates the tile.
    pub url_template: String,
    /// By tile kind, for the documents that have no coordinates.
    ///
    /// A `null` says the kind is asked for and there is nothing warming it
    /// would achieve — a document composed per request, or one whose freshness
    /// is measured in a minute. That is different from a kind the manifest
    /// never mentions, which is a URL somebody forgot to give, and is refused.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta_urls: BTreeMap<String, Option<String>>,
    pub cost: Cost,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lanes: Option<Lanes>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<Dependency>,
    pub zoom_semantics: ZoomSemantics,
}

/// What a manifest says about fetching a document with no coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum MetaUrl {
    /// Fetch it here.
    At(String),
    /// The service named this kind and said there is no URL worth fetching.
    NotWarmable,
    /// The manifest does not mention this kind at all.
    Unnamed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    /// For cells the digest has no measurement for. Anything with observed
    /// demand has a real number instead.
    pub default_gen_ms: f64,
    pub default_bytes: f64,
    /// What the origin will tolerate, and what turns a plan into a duration.
    pub concurrency_limit: u32,
    pub rate_per_s: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing: Option<Billing>,
}

/// Resources consumed per generation, and no prices.
///
/// Prices move for the vendor's reasons and counts move for the service's, so
/// they are kept apart: a manifest carrying prices would silently change what
/// an old estimate meant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Billing {
    pub pricing_profile: String,
    /// How much of each resource one generation spends.
    ///
    /// The keys are the pricing table's keys, so a cost is the sum of the two
    /// multiplied together and nothing has to map between them. That is also
    /// what lets a service bill for something this specification has never
    /// heard of — a container's memory-seconds, a GPU — without a revision:
    /// the resource is named in both files or it is priced at nothing.
    ///
    /// A `null` amount means "measure it": see [`Billing::MEASURED`] for the
    /// two that have a measurement to fall back on.
    #[serde(default)]
    pub per_gen: BTreeMap<String, Option<f64>>,
}

impl Billing {
    /// The resources a digest can stand in for when the amount is `null`.
    ///
    /// `cpu_ms` falls back to the measured generation time and `egress_byte`
    /// to the measured response size. Any other resource left null is priced
    /// at nothing, because there is no measurement that means it.
    pub const MEASURED: [&'static str; 2] = [
        crate::pricing::unit::CPU_MS,
        crate::pricing::unit::EGRESS_BYTE,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lanes {
    /// The origin keeps real misses ahead of warming.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive_priority: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub order: DependencyOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyOrder {
    Before,
}

/// Whether a shallower zoom is a coarser view of the same ground.
///
/// This is the whole of what decides whether warming ancestors is worth
/// anything. Under `SizeBucket` a shallower tile covers the same ground with
/// less geometry, so warming it rescues nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoomSemantics {
    Resolution,
    SizeBucket,
}

impl ServiceManifest {
    /// The URL that regenerates a tile.
    ///
    /// A service's own template, filled in. okibi never constructs a URL any
    /// other way, which is what keeps it from having to know what a tile is.
    pub fn url_for(&self, tileset: &str, id: &str, epoch: &Epoch) -> String {
        self.fill(&self.url_template, tileset, id, epoch)
    }

    /// What to do about a document with no coordinates.
    pub fn meta_url_for(&self, kind: &str, tileset: &str, id: &str, epoch: &Epoch) -> MetaUrl {
        match self.meta_urls.get(kind) {
            None => MetaUrl::Unnamed,
            Some(None) => MetaUrl::NotWarmable,
            Some(Some(template)) => MetaUrl::At(self.fill(template, tileset, id, epoch)),
        }
    }

    fn fill(&self, template: &str, tileset: &str, id: &str, epoch: &Epoch) -> String {
        template
            .replace("{tileset}", tileset)
            .replace("{id}", id)
            .replace("{epoch.source}", &epoch.source)
            .replace("{epoch.algo}", &epoch.algo)
            .replace("{epoch.param}", &epoch.param)
    }
}

/// The parts of a cache key that are not per-tile, spelled as the key spells
/// them.
///
/// Three names for what a part is for, rather than three parts every service
/// must have. A key made of two pieces fills two and leaves the third empty;
/// splitting one piece three ways to fill the names would put strings in an
/// event that are in no cache key, which is the one thing an epoch may not be.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epoch {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub algo: String,
    #[serde(default)]
    pub param: String,
}

impl Epoch {
    /// Whether this says anything at all.
    ///
    /// An event with no epoch could never be matched against an invalidation,
    /// so it would aggregate into a cell no plan can ever act on.
    pub fn is_empty(&self) -> bool {
        self.source.is_empty() && self.algo.is_empty() && self.param.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ServiceManifest {
        ServiceManifest {
            manifest: MANIFEST_VERSION.into(),
            service: "papers".into(),
            url_template: "https://papers.reearth.land/t/{tileset}/{id}?e={epoch.param}".into(),
            meta_urls: [(
                "tileset".to_string(),
                Some("https://p/{tileset}/meta.json".to_string()),
            )]
            .into_iter()
            .collect(),
            cost: Cost {
                default_gen_ms: 30000.0,
                default_bytes: 90000.0,
                concurrency_limit: 4,
                rate_per_s: 2.0,
                billing: None,
            },
            lanes: None,
            depends_on: vec![],
            zoom_semantics: ZoomSemantics::Resolution,
        }
    }

    #[test]
    fn a_url_is_the_services_own_template_filled_in() {
        let epoch = Epoch {
            source: "osm-2026-08-18".into(),
            algo: "ezu-0.7.1".into(),
            param: "style-aoi-04@r13".into(),
        };

        assert_eq!(
            manifest().url_for("style-aoi-04", "14/14552/6451", &epoch),
            "https://papers.reearth.land/t/style-aoi-04/14/14552/6451?e=style-aoi-04@r13"
        );
        assert_eq!(
            manifest().meta_url_for("tileset", "style-aoi-04", "meta.json", &epoch),
            MetaUrl::At("https://p/style-aoi-04/meta.json".to_string())
        );
        assert_eq!(
            manifest().meta_url_for("subtree", "style-aoi-04", "x", &epoch),
            MetaUrl::Unnamed
        );
    }

    /// A kind the service named and gave no URL for is not the same as one it
    /// never mentioned: the first is a document warming cannot help, the
    /// second is a URL somebody forgot.
    #[test]
    fn a_kind_can_be_named_and_still_have_nowhere_to_fetch() {
        let mut manifest = manifest();
        manifest.meta_urls.insert("subtree".into(), None);

        assert_eq!(
            manifest.meta_url_for("subtree", "style-aoi-04", "x", &Epoch::default()),
            MetaUrl::NotWarmable
        );
    }
}
