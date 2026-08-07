use ipc::named_places::{
    named_places_from_json, validate_look_only_way_to_yes, LookOnlyWayToYesReport,
};
use std::path::PathBuf;

const DEFAULT_TODO_DIR: &str = "/home/liamw/osl-plan/OSL-AUDITS/todo";

fn main() {
    let args = Args::parse(std::env::args().skip(1).collect());
    let records_path = args.records.unwrap_or_else(default_records_path);
    let todo_dir = args.todo.unwrap_or_else(|| PathBuf::from(DEFAULT_TODO_DIR));

    let exit_code = match run(&records_path, &todo_dir) {
        Ok(report) => {
            print_report(&report);
            if report.is_valid() {
                0
            } else {
                1
            }
        }
        Err(error) => {
            println!("TASK4221_ERROR={error}");
            1
        }
    };
    std::process::exit(exit_code);
}

fn run(records_path: &PathBuf, todo_dir: &PathBuf) -> Result<LookOnlyWayToYesReport, String> {
    let records = std::fs::read_to_string(records_path)
        .map_err(|error| format!("read records {}: {error}", records_path.display()))?;
    let rows = named_places_from_json(&records)?;
    validate_look_only_way_to_yes(&rows, todo_dir)
}

fn print_report(report: &LookOnlyWayToYesReport) {
    for row in &report.look_only_rows {
        if !row.has_named_way_to_yes() {
            println!(
                "TASK4221_REFUSED_ROW={} with no way to yes",
                row.place.to_ascii_lowercase()
            );
        }
        println!(
            "TASK4221_LOOK_ONLY_ROW place=\"{}\" research_task={} research_task_exists={} research_task_done={} build_task={} build_task_exists={} build_task_done={}",
            row.place,
            row.research_task.as_deref().unwrap_or(""),
            row.research_task_exists,
            row.research_task_done,
            row.build_task.as_deref().unwrap_or(""),
            row.build_task_exists,
            row.build_task_done
        );
    }
    for task in &report.missing_task_numbers {
        println!("TASK4221_NAMED_TASK_NOT_FOUND={task}");
    }
    println!("TASK4221_LOOK_ONLY_ROWS={}", report.look_only_row_count());
    println!(
        "TASK4221_LOOK_ONLY_ROWS_WITH_EXISTING_RESEARCH_AND_BUILD={}",
        report.rows_with_existing_research_and_build_tasks()
    );
    println!(
        "TASK4221_REFUSED_ROWS_WITH_NO_NAMED_WAY_TO_YES={}",
        report.refused_rows_with_no_named_way_to_yes
    );
    println!(
        "TASK4221_REFUSED_ROWS_WITH_DONE_RESEARCH_TASK={}",
        report.refused_rows_with_done_research_task
    );
    println!(
        "TASK4221_REFUSED_ROWS_WITH_DONE_BUILD_TASK={}",
        report.refused_rows_with_done_build_task
    );
    println!(
        "TASK4221_MISSING_TASK_NUMBERS={}",
        report.missing_task_numbers.len()
    );
}

#[derive(Debug, Default)]
struct Args {
    records: Option<PathBuf>,
    todo: Option<PathBuf>,
}

impl Args {
    fn parse(args: Vec<String>) -> Self {
        let mut parsed = Self::default();
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--records" => {
                    if let Some(path) = args.get(index + 1) {
                        parsed.records = Some(PathBuf::from(path));
                    }
                    index += 2;
                }
                "--todo" => {
                    if let Some(path) = args.get(index + 1) {
                        parsed.todo = Some(PathBuf::from(path));
                    }
                    index += 2;
                }
                _ => {
                    index += 1;
                }
            }
        }
        parsed
    }
}

fn default_records_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/allowed-places-4203.json")
}
