//! What a service says about itself. See `spec/okibi-contract.md`.

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
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub meta_urls: std::collections::BTreeMap<String, String>,
    pub cost: Cost,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lanes: Option<Lanes>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<Dependency>,
    pub zoom_semantics: ZoomSemantics,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_ms_per_gen: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subrequests_per_gen: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_class_a_per_gen: Option<f64>,
    /// `null` means the digest's `avg_bytes` is used instead.
    ///
    /// Written even when absent, unlike the counts above it. Here the empty
    /// value means something — measure it — rather than meaning the field was
    /// not filled in, and a reader should see that said rather than inferred.
    #[serde(default)]
    pub egress_bytes_per_gen: Option<f64>,
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

    /// The URL for a document with no coordinates, if the service named one.
    pub fn meta_url_for(
        &self,
        kind: &str,
        tileset: &str,
        id: &str,
        epoch: &Epoch,
    ) -> Option<String> {
        let template = self.meta_urls.get(kind)?;
        Some(self.fill(template, tileset, id, epoch))
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

/// The three axes a cache key is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epoch {
    pub source: String,
    pub algo: String,
    pub param: String,
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
                "https://p/{tileset}/meta.json".to_string(),
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
            Some("https://p/style-aoi-04/meta.json".to_string())
        );
        assert_eq!(
            manifest().meta_url_for("subtree", "style-aoi-04", "x", &epoch),
            None
        );
    }
}
