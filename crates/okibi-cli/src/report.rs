//! The plan, as something to read on a pull request.
//!
//! This is the point of the dry run. A commit that changes one epoch string
//! costs money and time that nobody could see before, and putting the number
//! next to the diff is what makes "could we avoid raising the algo epoch here"
//! a question a reviewer can actually ask.

use okibi_core::{InvalidationEvent, WarmPlan};

/// A GitHub Actions job stops at six hours, so a plan longer than that cannot
/// be run inside one and has to be handed to something without that limit.
pub const ACTIONS_JOB_LIMIT_S: f64 = 6.0 * 3600.0;

/// Whether a plan could be run inside the job that produced it.
///
/// This is a fact about the plan's size, not a decision about where to run it.
/// Warming waits on IO for hours, which is free on a Worker and is a rented
/// two-core machine sitting idle in a job, so the executor is the normal
/// destination whatever the length. What this settles is whether running in
/// place is available at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    FitsInAJob,
    LongerThanAJob,
}

pub fn length_of(plan: &WarmPlan) -> Length {
    if plan.estimate.warm.wall_clock_s > ACTIONS_JOB_LIMIT_S {
        Length::LongerThanAJob
    } else {
        Length::FitsInAJob
    }
}

/// The marker that lets a second run find its own comment instead of adding
/// another one under it.
pub const MARKER: &str = "<!-- okibi:plan -->";

pub fn markdown(plan: &WarmPlan, event: &InvalidationEvent) -> String {
    let mut out = String::from(MARKER);
    out.push('\n');

    out.push_str(&format!(
        "**okibi** — `{}` / `{}`: the `{}` epoch moves `{}` → `{}`\n\n",
        event.service,
        event.tileset,
        axis(event),
        event.epoch_from,
        event.epoch_to,
    ));

    if plan.entries.is_empty() {
        out.push_str(
            "Nothing to warm: no tile in the invalidated range has been asked for in the \
             digests read. Whatever is requested next is generated on demand, as it would \
             have been anyway.\n",
        );
        return out;
    }

    let warm = &plan.estimate.warm;
    let no_warm = &plan.estimate.no_warm;

    out.push_str("| | |\n|---|---|\n");
    out.push_str(&format!(
        "| Warming | {} tiles, {} of the demand in scope |\n",
        thousands(warm.tiles as f64),
        percent(plan.stats.coverage_of_demand)
    ));
    out.push_str(&format!(
        "| Costs | {} · {} · {} added |\n",
        duration(warm.wall_clock_s),
        money(warm.usd),
        bytes(warm.storage_delta_bytes.unwrap_or(0.0))
    ));
    out.push_str(&format!(
        "| Not warming | {} first requests wait{} |\n",
        thousands(no_warm.affected_first_requests),
        match no_warm.p95_first_byte_ms {
            Some(p95) => format!(", p95 {}", duration(p95 / 1000.0)),
            None => String::new(),
        }
    ));

    if !plan.estimate.marginal.is_empty() {
        out.push_str("\nWhere to stop:\n\n");
        out.push_str("| Coverage | Tiles | Cost | Time |\n|---|---|---|---|\n");
        for point in &plan.estimate.marginal {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                percent(point.coverage),
                thousands(point.tiles as f64),
                money(point.usd),
                duration(point.wall_clock_s)
            ));
        }
    }

    out.push('\n');
    match length_of(plan) {
        Length::FitsInAJob => out.push_str(&format!(
            "Short enough to run in a job, with {} to spare, if there is no \
             executor to hand it to.\n",
            duration(ACTIONS_JOB_LIMIT_S - warm.wall_clock_s)
        )),
        Length::LongerThanAJob => out.push_str(&format!(
            "⚠ Longer than the {} a job may last: this one needs the executor.\n",
            duration(ACTIONS_JOB_LIMIT_S)
        )),
    }

    out
}

fn axis(event: &InvalidationEvent) -> &'static str {
    match event.axis {
        okibi_core::Axis::Source => "source",
        okibi_core::Axis::Algo => "algo",
        okibi_core::Axis::Param => "param",
    }
}

fn percent(fraction: f64) -> String {
    format!("{:.0}%", fraction * 100.0)
}

fn money(usd: f64) -> String {
    if usd > 0.0 && usd < 0.01 {
        // Rounding a real cost to $0.00 reads as free, which is a different
        // claim from "less than a cent".
        return "<$0.01".to_string();
    }
    format!("${usd:.2}")
}

fn thousands(value: f64) -> String {
    let digits = format!("{:.0}", value.max(0.0));
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn bytes(count: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut count = count.max(0.0);
    let mut unit = 0;
    while count >= 1024.0 && unit < UNITS.len() - 1 {
        count /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{count:.0}B")
    } else {
        format!("{count:.1}{}", UNITS[unit])
    }
}

fn duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    match (seconds / 3600, (seconds % 3600) / 60) {
        (0, 0) => format!("{}s", seconds % 60),
        (0, minutes) => format!("{minutes}m"),
        (hours, minutes) => format!("{hours}h{minutes:02}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use okibi_core::{
        Scope,
        invalidation::Axis,
        plan::{DerivedFrom, Estimate, Marginal, NoWarmCost, Stats, WarmCost},
    };

    fn event() -> InvalidationEvent {
        InvalidationEvent {
            event: "okibi-invalidation/1".into(),
            service: "papers".into(),
            tileset: "style-aoi-04".into(),
            axis: Axis::Param,
            epoch_from: "style-aoi-04@r12".into(),
            epoch_to: "style-aoi-04@r13".into(),
            scope: Scope::All,
            occurred_at: "2026-08-24T02:00:00Z".into(),
            deadline: None,
        }
    }

    fn plan(tiles: usize, wall_clock_s: f64, usd: f64) -> WarmPlan {
        WarmPlan {
            plan: "okibi-warm-plan/1".into(),
            derived_from: DerivedFrom {
                digest: vec![],
                invalidation: "sha256:0".into(),
                manifests: Default::default(),
            },
            entries: (0..tiles)
                .map(|i| okibi_core::Entry {
                    url: format!("https://example.test/{i}"),
                    service: "papers".into(),
                    priority: 1.0,
                    lane: okibi_core::Lane::Warm,
                    not_before: None,
                    expected_gen_ms: 30_000.0,
                    saved_req_estimate: Some(10.0),
                })
                .collect(),
            stats: Stats {
                total: tiles,
                sum_expected_gen_ms: 0.0,
                coverage_of_demand: 0.93,
                unwarmable: 0,
            },
            estimate: Estimate {
                pricing: "pricing/cloudflare-2026-08.json@sha256:0".into(),
                warm: WarmCost {
                    tiles,
                    wall_clock_s,
                    cpu_ms: None,
                    usd,
                    storage_delta_bytes: Some(4.2e8),
                },
                no_warm: NoWarmCost {
                    affected_first_requests: 7900.0,
                    user_wait_ms_total: 2.4e8,
                    p95_first_byte_ms: Some(34_000.0),
                },
                reclaimable: None,
                marginal: vec![Marginal {
                    coverage: 0.8,
                    tiles: 1680,
                    usd: 3.2,
                    wall_clock_s: 4100.0,
                }],
            },
        }
    }

    #[test]
    fn a_report_says_what_the_change_costs() {
        let report = markdown(&plan(4210, 8830.0, 6.42), &event());

        assert!(report.starts_with(MARKER));
        assert!(report.contains("`param` epoch moves `style-aoi-04@r12` → `style-aoi-04@r13`"));
        assert!(report.contains("4,210 tiles, 93% of the demand"));
        assert!(report.contains("2h27m · $6.42 · 400.5MB added"));
        assert!(report.contains("7,900 first requests wait, p95 34s"));
        assert!(report.contains("| 80% | 1,680 | $3.20 | 1h08m |"));
    }

    #[test]
    fn a_plan_that_fits_says_how_much_room_is_left() {
        let report = markdown(&plan(10, 3600.0, 1.0), &event());
        assert!(report.contains("5h00m to spare"), "{report}");
        assert_eq!(length_of(&plan(10, 3600.0, 1.0)), Length::FitsInAJob);
    }

    #[test]
    fn a_plan_too_long_for_a_job_says_so() {
        let long = plan(100_000, 7.0 * 3600.0, 90.0);
        assert_eq!(length_of(&long), Length::LongerThanAJob);
        assert!(markdown(&long, &event()).contains("needs the executor"));
    }

    /// An invalidation nobody has demand for is the good case, and saying
    /// nothing about it would look like the check failed to run.
    #[test]
    fn an_empty_plan_says_why_it_is_empty() {
        let report = markdown(&plan(0, 0.0, 0.0), &event());
        assert!(report.contains("Nothing to warm"), "{report}");
    }

    #[test]
    fn a_real_cost_is_not_rounded_down_to_free() {
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(0.004), "<$0.01");
        assert_eq!(money(6.421), "$6.42");
    }

    #[test]
    fn numbers_read_the_way_people_read_them() {
        assert_eq!(thousands(4210.0), "4,210");
        assert_eq!(thousands(999.0), "999");
        assert_eq!(thousands(1_234_567.0), "1,234,567");
        assert_eq!(bytes(4.2e8), "400.5MB");
        assert_eq!(bytes(512.0), "512B");
        assert_eq!(duration(8830.0), "2h27m");
    }
}
