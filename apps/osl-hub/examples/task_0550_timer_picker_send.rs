use osl_privacy_hub::broker::timer_picker_send_command_expiry_at;
use osl_privacy_hub::security::timer_picker_state;

fn usage() -> ! {
    eprintln!(
        "usage: task_0550_timer_picker_send <days> <hours> <minutes> <seconds> <now_unix_seconds>"
    );
    std::process::exit(2);
}

fn parse_part(raw: Option<&String>, label: &str) -> i64 {
    let Some(value) = raw else {
        usage();
    };
    value.parse::<i64>().unwrap_or_else(|_| {
        eprintln!("TASK0550 timer_picker.{label}=invalid");
        std::process::exit(2);
    })
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        usage();
    }
    let days = parse_part(args.first(), "days");
    let hours = parse_part(args.get(1), "hours");
    let minutes = parse_part(args.get(2), "minutes");
    let seconds = parse_part(args.get(3), "seconds");
    let now = parse_part(args.get(4), "now");
    if days < 0 || hours < 0 || minutes < 0 || seconds < 0 {
        eprintln!("TASK0550 timer_picker.error=negative value");
        std::process::exit(2);
    }
    let picker = match timer_picker_state(days as u32, hours as u32, minutes as u32, seconds as u32)
    {
        Ok(picker) => picker,
        Err(error) => {
            eprintln!("TASK0550 timer_picker.error={error}");
            std::process::exit(1);
        }
    };
    let expiry = match timer_picker_send_command_expiry_at(&picker, now) {
        Ok(expiry) => expiry,
        Err(error) => {
            eprintln!("TASK0550 send.error={error}");
            std::process::exit(1);
        }
    };
    println!("TASK0550 timer_picker.days={}", picker.days);
    println!("TASK0550 timer_picker.hours={}", picker.hours);
    println!("TASK0550 timer_picker.minutes={}", picker.minutes);
    println!("TASK0550 timer_picker.seconds={}", picker.seconds);
    println!("TASK0550 send.duration_seconds={}", expiry.duration_seconds);
    println!("TASK0550 send.now={now}");
    println!("TASK0550 send.expires_at={}", expiry.expires_at);
    println!("TASK0550 send.seconds_ahead={}", expiry.seconds_ahead);
}
