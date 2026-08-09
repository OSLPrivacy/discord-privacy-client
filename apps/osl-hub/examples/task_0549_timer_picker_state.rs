use osl_privacy_hub::security::{default_timer_picker_state, timer_picker_state};

fn usage() -> ! {
    eprintln!("usage: task_0549_timer_picker_state query-default | set <days> <hours> <minutes>");
    std::process::exit(2);
}

fn parse_part(raw: Option<&String>, label: &str) -> u32 {
    let Some(value) = raw else {
        usage();
    };
    value.parse::<u32>().unwrap_or_else(|_| {
        eprintln!("TASK0549 timer_picker.{label}=invalid");
        std::process::exit(2);
    })
}

fn print_state(prefix: &str, state: osl_privacy_hub::security::TimerPickerStateDto) {
    println!("{prefix}.days={}", state.days);
    println!("{prefix}.hours={}", state.hours);
    println!("{prefix}.minutes={}", state.minutes);
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("query-default") if args.len() == 1 => {
            print_state(
                "TASK0549 timer_picker.default",
                default_timer_picker_state(),
            );
        }
        Some("set") if args.len() == 4 => {
            let days = parse_part(args.get(1), "days");
            let hours = parse_part(args.get(2), "hours");
            let minutes = parse_part(args.get(3), "minutes");
            match timer_picker_state(days, hours, minutes) {
                Ok(state) => print_state("TASK0549 timer_picker.accepted", state),
                Err(error) => {
                    eprintln!("TASK0549 timer_picker.rejected_days={days}");
                    eprintln!("TASK0549 timer_picker.error={error}");
                    std::process::exit(1);
                }
            }
        }
        _ => usage(),
    }
}
