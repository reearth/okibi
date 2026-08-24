//! The `okibi` binary.
//!
//! Three commands carry the pipeline, one per trigger: `digest` on a daily
//! schedule, `plan` after a deploy, `warm` right after a plan. `estimate`,
//! `diff` and `explain` are for reading a plan rather than producing one.
//!
//! None of them exist yet.

fn main() {
    eprintln!("okibi: no commands yet");
    std::process::exit(1);
}
