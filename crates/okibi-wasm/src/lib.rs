//! What a service needs from okibi at request time, which is projection and
//! nothing else.
//!
//! A service writes `tile.qk` on its hot path, and getting it wrong is not
//! visible: the events keep arriving, the digest keeps aggregating, and the
//! cells are quietly in the wrong place. So the projection a service runs is
//! the same compiled code the planner runs, rather than a second
//! implementation of the same arithmetic that agrees with it today.

use okibi_core::{DigestQuery, Window, aggregate, query};
use okibi_qk::{Quadkey, Scheme, Tile};
use wasm_bindgen::prelude::*;

/// How a service numbers its tiles. Mirrors [`okibi_qk::Scheme`], which cannot
/// cross the boundary itself.
#[wasm_bindgen(js_name = Scheme)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsScheme {
    /// Web Mercator, y from the north. Slippy maps.
    WebMercator = "web-mercator",
    /// Web Mercator, y from the south.
    WebMercatorTms = "web-mercator-tms",
    /// Geographic (EPSG:4326), two root tiles wide, y from the north.
    Geographic = "geographic",
    /// Geographic, y from the south. Cesium's terrain tiles.
    GeographicTms = "geographic-tms",
}

impl From<JsScheme> for Scheme {
    fn from(scheme: JsScheme) -> Self {
        match scheme {
            JsScheme::WebMercator => Scheme::WebMercator,
            JsScheme::WebMercatorTms => Scheme::WebMercatorTms,
            JsScheme::Geographic => Scheme::Geographic,
            JsScheme::GeographicTms => Scheme::GeographicTms,
            JsScheme::__Invalid => Scheme::WebMercator,
        }
    }
}

/// The quadkey for a tile, as deep as the tile's own level.
///
/// This is the one every service calls: `tile.qk` is the tile itself, in the
/// shared space.
#[wasm_bindgen(js_name = quadkeyForTile)]
pub fn quadkey_for_tile(scheme: JsScheme, level: u8, x: u32, y: u32) -> Result<String, JsError> {
    quadkey_for_tile_at(scheme, level, x, y, level)
}

/// The same, at a level of your choosing.
#[wasm_bindgen(js_name = quadkeyForTileAt)]
pub fn quadkey_for_tile_at(
    scheme: JsScheme,
    level: u8,
    x: u32,
    y: u32,
    at: u8,
) -> Result<String, JsError> {
    let tile = Tile::new(scheme.into(), level, x, y).map_err(to_js)?;
    Ok(tile.quadkey(at).map_err(to_js)?.to_string())
}

/// The quadkey containing a point.
#[wasm_bindgen(js_name = quadkeyForPoint)]
pub fn quadkey_for_point(lon: f64, lat: f64, level: u8) -> Result<String, JsError> {
    let point = okibi_qk::LonLat::new(lon, lat).map_err(to_js)?;
    Ok(point.quadkey(level).map_err(to_js)?.to_string())
}

/// The eight-character form the digest aggregates by.
#[wasm_bindgen(js_name = qk8)]
pub fn qk8(quadkey: &str) -> Result<String, JsError> {
    Ok(quadkey.parse::<Quadkey>().map_err(to_js)?.qk8().to_string())
}

/// Whether `prefix` is `quadkey` or an ancestor of it — how an invalidation
/// scope is matched.
#[wasm_bindgen(js_name = startsWith)]
pub fn starts_with(quadkey: &str, prefix: &str) -> Result<bool, JsError> {
    let quadkey: Quadkey = quadkey.parse().map_err(to_js)?;
    let prefix: Quadkey = prefix.parse().map_err(to_js)?;
    Ok(quadkey.starts_with(&prefix))
}

fn to_js(error: okibi_qk::Error) -> JsError {
    JsError::new(&error.to_string())
}

/// The two queries a digest is made of, for a day.
///
/// Returns `{ "cells": "SELECT …", "topTiles": "SELECT …" }`. Running them and
/// keeping the answer is the caller's job; what the answer has to be asked for
/// is not, because two rules of the binding live in the text — every frequency
/// carries the sampling weight, and demand counts organic requests only —
/// and neither fails visibly when it is left out.
///
/// `topTiles` is for one service, because its row limit is spent per service:
/// ordered by demand across all of them, the busiest service takes every row
/// and the rest are planned from nothing. `service` may be left out when the
/// config names exactly one, which is the shape a service running its own
/// cron is in.
///
/// Where neither says which service, `topTiles` is `null` and only the cells
/// query comes back. That is the aggregating caller's order of work: the
/// cells query is what says which services wrote anything, and the top-tiles
/// query is asked once per service it named.
#[wasm_bindgen(js_name = digestQueries)]
pub fn digest_queries(
    config: JsValue,
    date: &str,
    service: Option<String>,
) -> Result<JsValue, JsError> {
    let config: DigestQuery = if config.is_undefined() || config.is_null() {
        DigestQuery::default()
    } else {
        serde_wasm_bindgen::from_value(config).map_err(|e| JsError::new(&e.to_string()))?
    };
    let window = Window::parse(date).map_err(|e| JsError::new(&e.to_string()))?;

    let service = match (service, config.services.as_slice()) {
        (Some(service), _) => Some(service),
        (None, [only]) => Some(only.clone()),
        (None, _) => None,
    };

    let queries = Queries {
        cells: query::cells(&config, &window),
        top_tiles: service.map(|service| query::top_tiles(&config, &window, &service)),
    };
    serde_wasm_bindgen::to_value(&queries).map_err(|e| JsError::new(&e.to_string()))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Queries {
    cells: String,
    top_tiles: Option<String>,
}

/// Roll query rows up into digest records.
///
/// The same function `okibi digest` uses, rather than a second one that agrees
/// with it today: which cell an unplaced request belongs to, how a tie between
/// two equally hot tiles is broken, what happens to a row that cannot be
/// placed — none of these fail loudly when they differ, and a digest that
/// means something slightly different is a plan that warms somewhere slightly
/// wrong.
///
/// Returns `{ records, skipped }`. What was skipped is reported rather than
/// dropped: a digest that quietly covered less than it was asked to reads as a
/// quiet day.
#[wasm_bindgen(js_name = assembleDigest)]
pub fn assemble_digest(
    cells: JsValue,
    tiles: JsValue,
    date: &str,
    top_n: usize,
) -> Result<JsValue, JsError> {
    let cells: Vec<aggregate::CellRow> =
        serde_wasm_bindgen::from_value(cells).map_err(|e| JsError::new(&e.to_string()))?;
    let tiles: Vec<aggregate::TileRow> =
        serde_wasm_bindgen::from_value(tiles).map_err(|e| JsError::new(&e.to_string()))?;
    let window = Window::parse(date).map_err(|e| JsError::new(&e.to_string()))?;

    let (records, skipped) = aggregate::assemble(cells, tiles, &window, top_n);
    let assembled = Assembled { records, skipped };

    serde_wasm_bindgen::to_value(&assembled).map_err(|e| JsError::new(&e.to_string()))
}

#[derive(serde::Serialize)]
struct Assembled {
    records: Vec<okibi_core::DigestRecord>,
    skipped: aggregate::Skipped,
}
