//! Reading the planner's inputs off disk, and recording what was read.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use okibi_core::{
    DigestRecord, InvalidationEvent, PricingTable, ServiceManifest, manifest::Epoch,
    planner::Sources,
};
use sha2::{Digest, Sha256};

pub use okibi_core::EpochsFile;

/// Everything `plan` read, and the hashes that say so.
pub struct Loaded {
    pub digests: Vec<DigestRecord>,
    pub invalidation: InvalidationEvent,
    pub manifests: Vec<ServiceManifest>,
    pub pricing: PricingTable,
    pub epoch: Epoch,
    pub sources: Sources,
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// `sha256:` and sixty-four hex characters, over the bytes as they are on disk.
pub fn hash_of(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

/// Every `.jsonl` under a directory, or the file itself, sorted.
///
/// Sorted because the plan names them and the plan has to be the same file
/// twice; directory order is not.
pub fn digest_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in
                std::fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?
            {
                let entry = entry?.path();
                if entry.extension().is_some_and(|e| e == "jsonl") {
                    found.push(entry);
                }
            }
        } else {
            found.push(path.clone());
        }
    }
    found.sort();

    if found.is_empty() {
        bail!("no digests found");
    }
    Ok(found)
}

fn read_digest_file(path: &Path) -> Result<Vec<DigestRecord>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).with_context(|| format!("{}:{}", path.display(), i + 1))
        })
        .collect()
}

/// Manifests, from a JSON array or from a directory of single manifests.
fn read_manifests(path: &Path) -> Result<Vec<ServiceManifest>> {
    if !path.is_dir() {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        // One service or several: a repository holding one manifest should not
        // have to wrap it in an array to be read.
        return match serde_json::from_str::<Vec<ServiceManifest>>(&text) {
            Ok(many) => Ok(many),
            Err(_) => Ok(vec![
                serde_json::from_str(&text)
                    .with_context(|| format!("parsing {}", path.display()))?,
            ]),
        };
    }

    let mut paths: Vec<PathBuf> = std::fs::read_dir(path)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();

    paths.iter().map(|p| read_json(p)).collect()
}

pub struct Paths<'a> {
    pub digests: &'a [PathBuf],
    pub invalidation: &'a Path,
    pub manifests: &'a Path,
    pub pricing: &'a Path,
    pub epochs: &'a Path,
}

pub fn load(paths: Paths<'_>) -> Result<Loaded> {
    let digest_paths = digest_paths(paths.digests)?;
    let mut digests = Vec::new();
    for path in &digest_paths {
        digests.extend(read_digest_file(path)?);
    }

    let invalidation: InvalidationEvent = read_json(paths.invalidation)?;
    let manifests = read_manifests(paths.manifests)?;
    let pricing: PricingTable = read_json(paths.pricing)?;

    let epochs: EpochsFile = read_json(paths.epochs)?;
    let epoch = epochs.epoch_for(&invalidation.tileset);

    let mut manifest_hashes = BTreeMap::new();
    if paths.manifests.is_dir() {
        for manifest in &manifests {
            let path = paths.manifests.join(format!("{}.json", manifest.service));
            if path.exists() {
                manifest_hashes.insert(manifest.service.clone(), hash_of(&path)?);
            }
        }
    } else {
        let hash = hash_of(paths.manifests)?;
        for manifest in &manifests {
            manifest_hashes.insert(manifest.service.clone(), hash.clone());
        }
    }

    let sources = Sources {
        digest: digest_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        invalidation: hash_of(paths.invalidation)?,
        manifests: manifest_hashes,
        pricing: format!("{}@{}", paths.pricing.display(), hash_of(paths.pricing)?),
    };

    Ok(Loaded {
        digests,
        invalidation,
        manifests,
        pricing,
        epoch,
        sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_is_over_the_bytes_on_disk() {
        let dir = std::env::temp_dir().join("okibi-inputs-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.json");
        std::fs::write(&path, "{}").unwrap();

        // sha256 of "{}", which is a value anyone can check.
        assert_eq!(
            hash_of(&path).unwrap(),
            "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
    }

    /// A service whose versions live in a cache key has nothing to put here,
    /// and asking it for a file anyway would be asking for something nothing
    /// reads.
    #[test]
    fn a_tileset_with_no_recorded_epochs_has_none() {
        let epochs = EpochsFile {
            service: "papers".into(),
            tilesets: Default::default(),
        };

        assert_eq!(epochs.epoch_for("style-aoi-04"), Epoch::default());
    }
}
