//! One-shot runner for the description-to-parts migration (ticket
//! f65f2b32, spec 24b3d22b). Dry-run by default; `--apply` performs the
//! write pass, and only after an in-process dry-run just produced a
//! zero-low-confidence report.
//!
//! ```text
//! cargo run -p ticket-api --example migrate_descriptions -- <index-root> [--apply]
//! ```

use std::{
    env,
    path::PathBuf,
    process::ExitCode,
};

use ticket_api::model::schema_registry::SchemaRegistry;
use ticket_api::storage::store::TicketStore;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(index_root) = args.next() else {
        eprintln!(
            "usage: migrate_descriptions <index-root> [--apply]"
        );
        return ExitCode::FAILURE;
    };
    let apply = args.any(|a| a == "--apply");

    let index_root = PathBuf::from(index_root);
    let registry = SchemaRegistry::with_builtins();
    let store = match TicketStore::open_with(&index_root, registry) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("failed to open store at {}: {error}", index_root.display());
            return ExitCode::FAILURE;
        },
    };

    let report = match store.migration_dry_run() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("dry-run failed: {error}");
            return ExitCode::FAILURE;
        },
    };

    println!("=== DRY RUN: {} ===", index_root.display());
    println!("scanned:                 {}", report.scanned);
    println!("migratable:               {}", report.migratable.len());
    println!("no_recognized_headings:   {}", report.no_recognized_headings);
    println!("skipped_no_description:   {}", report.skipped_no_description);
    println!("skipped_already_migrated: {}", report.skipped_already_migrated);
    println!("low_confidence:           {}", report.low_confidence.len());
    if !report.low_confidence.is_empty() {
        for id in &report.low_confidence {
            println!("  LOW CONFIDENCE: {id}");
        }
    }

    for plan in &report.migratable {
        println!(
            "  {}  {}",
            &plan.id.to_string()[..8],
            plan.title.as_deref().unwrap_or("(untitled)")
        );
        for (kind, count) in &plan.matched_counts {
            println!("    -> {kind:<10} {count} section(s)");
        }
        println!("    -> objective   {} lines remain", plan.objective_lines);
    }

    if !report.low_confidence.is_empty() {
        eprintln!(
            "ABORT: {} ticket(s) failed the lossless concatenation check; refusing to apply",
            report.low_confidence.len()
        );
        return ExitCode::FAILURE;
    }

    if !apply {
        println!("(dry-run only; pass --apply to write)");
        return ExitCode::SUCCESS;
    }

    let apply_report =
        match store.migration_apply(&report, Some("migration-tool")) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("apply failed: {error}");
                return ExitCode::FAILURE;
            },
        };

    println!("=== APPLY: {} ===", index_root.display());
    println!("migrated:       {}", apply_report.migrated.len());
    println!("skipped_stale:  {}", apply_report.skipped_stale.len());
    println!("skipped_planned: {}", apply_report.skipped_planned.len());
    println!("parts_created:  {}", apply_report.parts_created);

    ExitCode::SUCCESS
}
