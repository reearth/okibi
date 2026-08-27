//! The `okibi` binary.
//!
//! Three commands carry the pipeline, one per trigger: `digest` on a daily
//! schedule, `plan` after a deploy, `warm` right after a plan. `diff` and
//! `explain` are for reading a plan rather than producing one.

mod config;
mod inputs;
mod invalidation;
mod report;
mod review;
mod verify;
mod wae;
mod warm;
mod window;

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use okibi_core::{
    WarmPlan, aggregate,
    planner::{PlanInput, PlanOptions},
    query,
};

use crate::config::Config;

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

    /// Derive a warm plan from digests, an invalidation and the manifests.
    Plan {
        #[command(flatten)]
        inputs: PlanArgs,

        /// Where to write the plan. `-` is stdout.
        #[arg(long, default_value = "-")]
        out: String,

        /// Stop once the plan would cost this much.
        #[arg(long)]
        budget_usd: Option<f64>,

        /// Stop once this much of the demand is covered.
        #[arg(long)]
        coverage: Option<f64>,

        /// How fast older evidence stops counting, in days.
        #[arg(long, default_value_t = 7.0)]
        half_life: f64,

        /// Plan everything, whatever the event's deadline allows.
        #[arg(long)]
        ignore_deadline: bool,
    },

    /// Fetch a plan's entries, in order, within the manifests' limits.
    Warm {
        /// The plan to run.
        plan: PathBuf,

        /// The manifests, for the limits each origin will tolerate.
        #[arg(long)]
        manifests: Option<PathBuf>,

        /// Override the concurrency the manifests declare.
        #[arg(long)]
        concurrency: Option<usize>,

        /// Override the rate the manifests declare.
        #[arg(long)]
        rate_per_s: Option<f64>,

        /// Print the URLs instead of fetching them.
        #[arg(long)]
        dry_run: bool,
    },

    /// Derive invalidation events from two versions of okibi.epochs.json.
    ///
    /// A service does not write these by hand: it edits the file its cache
    /// keys come from, and the event is what the diff means.
    Invalidation {
        /// The epochs as they were, e.g. from `git show HEAD^:okibi.epochs.json`.
        #[arg(long)]
        before: PathBuf,

        /// The epochs as they are now.
        #[arg(long, default_value = "okibi.epochs.json")]
        after: PathBuf,

        /// When the change happened, as an RFC 3339 timestamp.
        #[arg(long)]
        occurred_at: String,

        /// When the warming should be finished by.
        #[arg(long)]
        deadline: Option<String>,

        /// Where to write. A directory takes `<tileset>.json`; `-` is stdout.
        #[arg(long, default_value = "-")]
        out: String,
    },

    /// Write a plan up as markdown, for a pull request to carry.
    Report {
        plan: PathBuf,

        /// The invalidation the plan answers.
        #[arg(long)]
        invalidation: PathBuf,
    },

    /// Say what changed between two plans.
    Diff { before: PathBuf, after: PathBuf },

    /// Say why a URL is where it is in a plan.
    Explain {
        plan: PathBuf,

        #[arg(long)]
        url: String,
    },
}

#[derive(Args)]
struct PlanArgs {
    /// Digest files or directories of them.
    #[arg(long, required = true, num_args = 1..)]
    digests: Vec<PathBuf>,

    /// The invalidation event.
    #[arg(long)]
    invalidation: PathBuf,

    /// A manifest, an array of them, or a directory of them.
    #[arg(long)]
    manifests: PathBuf,

    /// The pricing table to cost the plan with.
    #[arg(long)]
    pricing: PathBuf,

    /// The service's okibi.epochs.json, for the URLs.
    #[arg(long, default_value = "okibi.epochs.json")]
    epochs: PathBuf,

    /// Ask the origin whether a sample of the plan's URLs exist.
    ///
    /// The planner fills a template and cannot know the template was right.
    /// When it is wrong every entry answers 404 and the plan looks exactly
    /// like a correct one.
    #[arg(long, num_args = 0..=1, default_missing_value = "5")]
    verify: Option<usize>,
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

        Command::Plan {
            inputs,
            out,
            budget_usd,
            coverage,
            half_life,
            ignore_deadline,
        } => {
            run_plan(
                &inputs,
                &out,
                PlanOptions {
                    half_life_days: half_life,
                    budget_usd,
                    coverage,
                    honour_deadline: !ignore_deadline,
                },
            )
            .await
        }

        Command::Warm {
            plan,
            manifests,
            concurrency,
            rate_per_s,
            dry_run,
        } => {
            run_warm(
                &plan,
                manifests.as_deref(),
                concurrency,
                rate_per_s,
                dry_run,
            )
            .await
        }

        Command::Invalidation {
            before,
            after,
            occurred_at,
            deadline,
            out,
        } => run_invalidation(&before, &after, &occurred_at, deadline.as_deref(), &out),

        Command::Report { plan, invalidation } => run_report(&plan, &invalidation),

        Command::Diff { before, after } => run_diff(&before, &after),
        Command::Explain { plan, url } => run_explain(&plan, &url),
    }
}

async fn run_digest(
    config_path: &Path,
    date: Option<&str>,
    out: &str,
    print_sql: bool,
) -> Result<()> {
    let config = Config::load_or_default(config_path)?;
    let window = match date {
        Some(date) => window::parse(date)?,
        None => window::yesterday()?,
    };

    let cells_sql = query::cells(&config.query, &window);

    if print_sql {
        // The top-tiles query is run once per service, and which services
        // exist is an answer the cells query returns rather than a list kept
        // here. With nothing to read, the placeholder stands in for the name
        // that would be filled in.
        let named = if config.query.services.is_empty() {
            vec!["<service>".to_string()]
        } else {
            config.query.services.clone()
        };
        println!("-- cells\n{cells_sql}");
        for service in named {
            let sql = query::top_tiles(&config.query, &window, &service);
            println!("\n-- top tiles: {service}\n{sql}");
        }
        return Ok(());
    }

    let account = wae::account_from_env(config.account_id.as_deref())?;
    let client = wae::Client::new(&account, wae::token_from_env()?)?;
    let cells = client.query::<aggregate::CellRow>(&cells_sql).await?;

    // One top-tiles query per service. The limit is a row count, and a single
    // query spends all of it on whichever service is busiest — which is not
    // the one that most needs warming.
    let services: BTreeSet<String> = cells.iter().map(|cell| cell.service.clone()).collect();
    let mut tiles: Vec<aggregate::TileRow> = Vec::new();
    let mut truncated: Vec<String> = Vec::new();
    for service in &services {
        let sql = query::top_tiles(&config.query, &window, service);
        let rows = client.query::<aggregate::TileRow>(&sql).await?;
        if rows.len() >= config.query.top_rows {
            truncated.push(service.clone());
        }
        tiles.extend(rows);
    }

    let (records, skipped) = aggregate::assemble(cells, tiles, &window, config.query.top_n);

    let mut lines = String::new();
    for record in &records {
        lines.push_str(&serde_json::to_string(record)?);
        lines.push('\n');
    }
    write_out(&lines, out, &format!("{}.jsonl", window.date()))?;

    report_digest(&skipped, &truncated, &config, records.len());
    Ok(())
}

async fn run_plan(args: &PlanArgs, out: &str, options: PlanOptions) -> Result<()> {
    let loaded = inputs::load(inputs::Paths {
        digests: &args.digests,
        invalidation: &args.invalidation,
        manifests: &args.manifests,
        pricing: &args.pricing,
        epochs: &args.epochs,
    })?;

    let plan = okibi_core::plan(&PlanInput {
        digests: &loaded.digests,
        invalidation: &loaded.invalidation,
        manifests: &loaded.manifests,
        pricing: &loaded.pricing,
        epoch: loaded.epoch,
        sources: loaded.sources,
        options,
    })?;

    let mut json = serde_json::to_string_pretty(&plan)?;
    json.push('\n');
    write_out(&json, out, "plan.json")?;

    eprintln!(
        "okibi: {} entries, {:.0}% of the demand in scope, ${:.2}, {}",
        plan.stats.total,
        plan.stats.coverage_of_demand * 100.0,
        plan.estimate.warm.usd,
        duration(plan.estimate.warm.wall_clock_s),
    );
    if plan.stats.unwarmable > 0 {
        eprintln!(
            "okibi: {} document(s) the manifest says warming cannot help, left out",
            plan.stats.unwarmable
        );
    }
    if plan.stats.too_fast > 0 {
        eprintln!(
            "okibi: {} tile(s) generate faster than this service warms at, left out",
            plan.stats.too_fast
        );
    }

    if let Some(size) = args.verify {
        return report_verified(
            verify::verify(&plan, size, warm::secret_from_env().as_deref()).await,
        );
    }
    Ok(())
}

/// Say what the sample answered, and refuse a plan whose URLs are not URLs.
///
/// Written after the plan rather than instead of it: the plan is worth having
/// even when it is wrong, because reading it is how the template gets fixed.
fn report_verified(checked: Result<Vec<verify::Checked>>) -> Result<()> {
    let checked = checked?;
    if checked.is_empty() {
        eprintln!("okibi: nothing to verify");
        return Ok(());
    }

    for check in &checked {
        let answer = match (check.status, &check.error) {
            (Some(status), _) => status.to_string(),
            (None, Some(error)) => error.clone(),
            (None, None) => "no answer".to_string(),
        };
        let mark = if check.ok() { " " } else { "!" };
        // The service is named because a plan can hold two of them and one
        // template can be wrong on its own.
        eprintln!(
            "okibi: {mark} #{} {answer} {} {}",
            check.rank, check.service, check.url
        );
    }

    let wrong = checked.iter().filter(|check| check.is_wrong()).count();
    let ok = checked.iter().filter(|check| check.ok()).count();
    eprintln!("okibi: {ok} of {} sampled URLs answered", checked.len());

    if wrong > 0 {
        // Refused rather than reported, because a plan whose URLs do not exist
        // costs a real request each and warms nothing, and the failure looks
        // exactly like success from every other number in the plan.
        anyhow::bail!(
            "{wrong} of {} sampled URLs do not exist: the manifest's template, \
             the digest's ids, or the epochs do not rebuild this service's URLs",
            checked.len()
        );
    }
    Ok(())
}

async fn run_warm(
    plan_path: &Path,
    manifests: Option<&Path>,
    concurrency: Option<usize>,
    rate_per_s: Option<f64>,
    dry_run: bool,
) -> Result<()> {
    let plan: WarmPlan = inputs::read_json(plan_path)?;

    let mut limits: BTreeMap<String, warm::Limits> = BTreeMap::new();
    if let Some(path) = manifests {
        let manifests: Vec<okibi_core::ServiceManifest> = match inputs::read_json(path) {
            Ok(many) => many,
            Err(_) => vec![inputs::read_json(path)?],
        };
        for manifest in manifests {
            let mut manifest_limits = warm::Limits::from_manifest(&manifest);
            if let Some(concurrency) = concurrency {
                manifest_limits.concurrency = concurrency.max(1);
            }
            if let Some(rate) = rate_per_s {
                manifest_limits.rate_per_s = rate;
            }
            limits.insert(manifest.service, manifest_limits);
        }
    }

    let mut default = warm::Limits::default();
    if let Some(concurrency) = concurrency {
        default.concurrency = concurrency.max(1);
    }
    if let Some(rate) = rate_per_s {
        default.rate_per_s = rate;
    }

    if manifests.is_none() && !dry_run {
        eprintln!(
            "okibi: no manifests given, so using {} at a time and {}/s",
            default.concurrency, default.rate_per_s
        );
    }

    let secret = warm::secret_from_env();
    if secret.is_none() && !dry_run {
        eprintln!(
            "okibi: no {} set, so these requests will be counted as demand",
            warm::SECRET_VARS[0]
        );
    }

    let report = warm::warm(&plan, &limits, &default, secret.as_deref(), dry_run).await?;

    eprintln!("okibi: warmed {} of {}", report.fetched, plan.entries.len());
    for (url, error) in &report.failed {
        eprintln!("okibi: {url} — {error}");
    }

    // A failed warm is not a failed deploy. The tiles that did not warm are
    // generated on demand, as they would have been anyway.
    Ok(())
}

fn run_invalidation(
    before: &Path,
    after: &Path,
    occurred_at: &str,
    deadline: Option<&str>,
    out: &str,
) -> Result<()> {
    let before: inputs::EpochsFile = inputs::read_json(before)?;
    let after: inputs::EpochsFile = inputs::read_json(after)?;

    let events = invalidation::between(&before, &after, occurred_at, deadline);
    if events.is_empty() {
        eprintln!("okibi: no epoch moved, so nothing was invalidated");
        return Ok(());
    }

    if out == "-" {
        let mut json = serde_json::to_string_pretty(&events)?;
        json.push('\n');
        std::io::stdout().write_all(json.as_bytes())?;
        return Ok(());
    }

    // One file per event, because one event is what `plan` takes: a deploy
    // that moved two tilesets is two plans, not one plan of both.
    let dir = PathBuf::from(out);
    std::fs::create_dir_all(&dir)?;
    for event in &events {
        let path = dir.join(format!("{}.json", event.tileset));
        let mut json = serde_json::to_string_pretty(event)?;
        json.push('\n');
        std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        eprintln!("okibi: wrote {}", path.display());
    }
    Ok(())
}

fn run_report(plan_path: &Path, invalidation: &Path) -> Result<()> {
    let plan: WarmPlan = inputs::read_json(plan_path)?;
    let event: okibi_core::InvalidationEvent = inputs::read_json(invalidation)?;

    print!("{}", report::markdown(&plan, &event));
    Ok(())
}

fn run_diff(before: &Path, after: &Path) -> Result<()> {
    let before: WarmPlan = inputs::read_json(before)?;
    let after: WarmPlan = inputs::read_json(after)?;
    let diff = review::diff(&before, &after);

    println!(
        "{} added, {} removed, {} moved",
        diff.added.len(),
        diff.removed.len(),
        diff.moved.len()
    );
    println!(
        "${:.2} -> ${:.2}, coverage {:.0}% -> {:.0}%",
        diff.usd_before,
        diff.usd_after,
        diff.coverage_before * 100.0,
        diff.coverage_after * 100.0
    );

    for url in &diff.added {
        println!("+ {url}");
    }
    for url in &diff.removed {
        println!("- {url}");
    }
    for (url, was, now) in &diff.moved {
        println!("~ {url} ({was} -> {now})");
    }
    Ok(())
}

fn run_explain(plan_path: &Path, url: &str) -> Result<()> {
    let plan: WarmPlan = inputs::read_json(plan_path)?;

    let Some(explanation) = review::explain(&plan, url) else {
        println!("{url} is not in this plan");
        return Ok(());
    };

    let entry = explanation.entry;
    println!("{url}");
    println!(
        "  {} of {} in the plan, priority {:.3}, lane {:?}",
        explanation.position + 1,
        explanation.of,
        entry.priority,
        entry.lane
    );
    println!(
        "  {:.0}ms to generate, {:.0} requests riding on it ({:.1}% of the plan's demand)",
        entry.expected_gen_ms,
        entry.saved_req_estimate.unwrap_or(0.0),
        explanation.share_of_demand * 100.0
    );
    println!("  derived from {}", plan.derived_from.digest.join(", "));
    Ok(())
}

/// Write to stdout or to a file, taking `name` if the target is a directory.
fn write_out(content: &str, out: &str, name: &str) -> Result<()> {
    if out == "-" {
        std::io::stdout().write_all(content.as_bytes())?;
        return Ok(());
    }

    let path = PathBuf::from(out);
    let path = if path.is_dir() || out.ends_with('/') {
        std::fs::create_dir_all(&path)?;
        path.join(name)
    } else {
        path
    };

    std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
    eprintln!("okibi: wrote {}", path.display());
    Ok(())
}

fn duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    match (seconds / 3600, (seconds % 3600) / 60) {
        (0, 0) => format!("{}s", seconds % 60),
        (0, minutes) => format!("{minutes}m"),
        (hours, minutes) => format!("{hours}h{minutes:02}m"),
    }
}

/// Say what was left out. A digest that silently described less than it was
/// asked to would read as a quiet day rather than as a truncated query.
fn report_digest(
    skipped: &aggregate::Skipped,
    truncated: &[String],
    config: &Config,
    records: usize,
) {
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
    // Per service, because that is how the limit is spent. A total across
    // services would say nothing about which of them was cut short.
    if !truncated.is_empty() {
        eprintln!(
            "okibi: the top-tiles query hit its {} row limit for {}, so the coldest \
             cells there may be described less finely than the rest",
            config.query.top_rows,
            truncated.join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_reads_as_one() {
        assert_eq!(duration(45.0), "45s");
        assert_eq!(duration(600.0), "10m");
        assert_eq!(duration(8830.0), "2h27m");
    }
}
