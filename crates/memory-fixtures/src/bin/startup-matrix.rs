use memory_fixtures::{
    run_startup_matrix,
    startup_matrix_succeeded,
    StartupMatrixClass,
    StartupMatrixOutcome,
    StartupMatrixResult,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    let toon = std::env::args()
        .skip(1)
        .any(|argument| argument == "--toon");
    let results = match run_startup_matrix() {
        Ok(results) => results,
        Err(error) => {
            eprintln!("startup matrix could not run: {error}");
            return ExitCode::from(2);
        },
    };

    if toon {
        match toon_format::encode_default(&results) {
            Ok(rendered) => println!("{rendered}"),
            Err(error) => {
                eprintln!("failed to encode startup matrix TOON: {error}");
                return ExitCode::from(2);
            },
        }
    } else {
        for result in &results {
            println!(
                "{:<22} | {:<10} | {}{}",
                result.tool,
                class_name(result.class),
                outcome_name(result.outcome),
                render_paths(result),
            );
        }
    }

    if startup_matrix_succeeded(&results) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn class_name(class: StartupMatrixClass) -> &'static str {
    match class {
        StartupMatrixClass::McpServer => "mcp-server",
        StartupMatrixClass::Viewer => "viewer",
    }
}

fn outcome_name(outcome: StartupMatrixOutcome) -> &'static str {
    match outcome {
        StartupMatrixOutcome::CleanStart => "clean start",
        StartupMatrixOutcome::StoreRefusal => "store refusal",
        StartupMatrixOutcome::PollutionDetected => "pollution detected",
        StartupMatrixOutcome::UnexpectedFailure => "unexpected failure",
    }
}

fn render_paths(result: &StartupMatrixResult) -> String {
    let mut details = Vec::new();
    if !result.created_paths.is_empty() {
        details.push(format!(" created={}", result.created_paths.join(",")));
    }
    if !result.removed_paths.is_empty() {
        details.push(format!(" removed={}", result.removed_paths.join(",")));
    }
    if !result.changed_paths.is_empty() {
        details.push(format!(" changed={}", result.changed_paths.join(",")));
    }
    if let Some(detail) = &result.detail {
        details.push(format!(" detail={detail}"));
    }
    details.concat()
}
