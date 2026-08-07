use ipc::named_places::{named_places_from_json, try_register_look_only_and_open_typing_box};
use std::path::PathBuf;

fn main() {
    let args = Args::parse(std::env::args().skip(1).collect());
    let place = args.place.unwrap_or_default();
    let records_path = args.records.unwrap_or_else(default_records_path);

    let exit_code = match run(&records_path, &place) {
        Ok(attempt) => {
            println!("TASK4214_REFUSED_WORDS={}", attempt.place_words);
            println!("TASK4214_REGISTER_EXIT={}", attempt.register_exit);
            println!(
                "TASK4214_TYPING_BOXES_OPENED={}",
                attempt.typing_boxes_opened
            );
            println!(
                "TASK4214_REFUSAL={} refused: research task {} has to answer first",
                attempt.place_words, attempt.research_task
            );
            attempt.register_exit
        }
        Err(error) => {
            println!("TASK4214_ERROR={error}");
            1
        }
    };
    std::process::exit(exit_code);
}

fn run(
    records_path: &PathBuf,
    place: &str,
) -> Result<ipc::named_places::LookOnlyShutAttempt, String> {
    if place.trim().is_empty() {
        return Err("missing --place".to_owned());
    }
    let records = std::fs::read_to_string(records_path)
        .map_err(|error| format!("read records {}: {error}", records_path.display()))?;
    let rows = named_places_from_json(&records)?;
    try_register_look_only_and_open_typing_box(&rows, place)
}

#[derive(Debug, Default)]
struct Args {
    place: Option<String>,
    records: Option<PathBuf>,
}

impl Args {
    fn parse(args: Vec<String>) -> Self {
        let mut parsed = Self::default();
        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--place" => {
                    if let Some(place) = args.get(index + 1) {
                        parsed.place = Some(place.to_owned());
                    }
                    index += 2;
                }
                "--records" => {
                    if let Some(path) = args.get(index + 1) {
                        parsed.records = Some(PathBuf::from(path));
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
