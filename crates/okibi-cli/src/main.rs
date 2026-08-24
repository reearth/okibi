//! The `okibi` binary.
//!
//! Three commands carry the pipeline, one per trigger: `digest` on a daily
//! schedule, `plan` after a deploy, `warm` right after a plan. `estimate`,
//! `diff` and `explain` are for reading a plan rather than producing one.
//!
//! Only `digest` exists so far.

mod config;
mod digest;
mod sql;
mod wae;
mod window;

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::{config::Config, window::Window};

#[derive(Parser)]
#[command(name = "okibi", version, about = "Cache warming for on-demand tiles")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Aggregate a day of tile-demand events into a demand digest.
    ///
    /// One run covers every service in the config: under the Analytics Engine
    /// binding there is one dataset and the index is the service, so a
    /// per-service run would be the same query over again.
    Digest {
        /// The config saying which account, dataset and services to read.
        #[arg(long, default_value = "services.json")]
        config: PathBuf,

        /// The day to aggregate. Defaults to yesterday, in UTC.
        #[arg(long)]
        date: Option<String>,

        /// Where to write. A directory takes `<date>.jsonl`; `-` is stdout.
        #[arg(long, default_value = "-")]
        out: String,

        /// Print the queries instead of running them.
        #[arg(long)]
        print_sql: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Digest {
            config,
            date,
            out,
            print_sql,
        } => run_digest(&config, date.as_deref(), &out, print_sql).await,
    }
}

async fn run_digest(
    config_path: &Path,
    date: Option<&str>,
    out: &str,
    print_sql: bool,
) -> Result<()> {
    let config = Config::load(config_path)?;
    let window = match date {
        Some(date) => Window::parse(date)?,
        None => Window::yesterday(),
    };

    let cells_sql = sql::cells(&config, &window);
    let tiles_sql = sql::top_tiles(&config, &window);

    if print_sql {
        println!("-- cells\n{cells_sql}\n\n-- top tiles\n{tiles_sql}");
        return Ok(());
    }

    let client = wae::Client::new(&config.account_id, wae::token_from_env()?)?;
    let (cells, tiles) = tokio::try_join!(
        client.query::<digest::CellRow>(&cells_sql),
        client.query::<digest::TileRow>(&tiles_sql),
    )?;

    let asked_for = tiles.len();
    let (records, skipped) = digest::assemble(cells, tiles, &window, config.top_n);

    write_records(&records, out, &window)?;
    report(&skipped, asked_for, &config, records.len());
    Ok(())
}

fn write_records(records: &[okibi_core::DigestRecord], out: &str, window: &Window) -> Result<()> {
    let mut lines = String::new();
    for record in records {
        lines.push_str(&serde_json::to_string(record)?);
        lines.push('\n');
    }

    if out == "-" {
        std::io::stdout().write_all(lines.as_bytes())?;
        return Ok(());
    }

    let path = PathBuf::from(out);
    let path = if path.is_dir() || out.ends_with('/') {
        std::fs::create_dir_all(&path)?;
        path.join(format!("{}.jsonl", window.date))
    } else {
        path
    };

    std::fs::write(&path, lines).with_context(|| format!("writing {}", path.display()))?;
    eprintln!(
        "okibi: wrote {} records to {}",
        records.len(),
        path.display()
    );
    Ok(())
}

/// Say what was left out. A digest that silently described less than it was
/// asked to would read as a quiet day rather than as a truncated query.
fn report(skipped: &digest::Skipped, tile_rows: usize, config: &Config, records: usize) {
    if skipped.unknown_kind > 0 {
        eprintln!(
            "okibi: {} cells named a kind this version does not have",
            skipped.unknown_kind
        );
    }
    if skipped.unplaceable > 0 {
        eprintln!(
            "okibi: {} content cells had no qk8 and could not be placed",
            skipped.unplaceable
        );
    }
    if skipped.cells_without_top > 0 {
        eprintln!(
            "okibi: {} of {records} cells have no top tiles listed",
            skipped.cells_without_top
        );
    }
    if tile_rows >= config.top_rows {
        eprintln!(
            "okibi: the top-tiles query hit its {} row limit, so the coldest cells \
             may be described less finely than the rest",
            config.top_rows
        );
    }
}
