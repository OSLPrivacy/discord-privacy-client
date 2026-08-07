use osl_privacy_hub::osl_chat_file_limits::osl_chat_file_size_limits;

fn usage() -> ! {
    eprintln!("usage: task_1330_chat_file_size_rules limits");
    std::process::exit(2);
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("limits") if args.len() == 1 => {
            for limit in osl_chat_file_size_limits() {
                println!(
                    "TASK1330 osl_chats.file_limit.{}={}",
                    limit.tier, limit.label
                );
                println!(
                    "TASK1330 osl_chats.file_limit.{}.bytes={}",
                    limit.tier, limit.max_bytes
                );
            }
        }
        _ => usage(),
    }
}
