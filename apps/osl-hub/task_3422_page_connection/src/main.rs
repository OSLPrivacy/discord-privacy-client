use task_3422_page_connection_placer::{run_direct_command, APP};

fn main() {
    let mut app = APP.to_owned();
    let mut text = "MAPLE-3422".to_owned();
    let mut front_window_grab = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--app" => app = args.next().unwrap_or_default(),
            "--text" => text = args.next().unwrap_or_default(),
            "--front-window-grab" => {
                front_window_grab = match args.next().as_deref() {
                    Some("off") => Some(false),
                    Some("on") => Some(true),
                    _ => None,
                };
            }
            _ => {
                eprintln!("usage: task-3422-page-connection-placer --front-window-grab off --app Messenger --text MAPLE-3422");
                std::process::exit(2);
            }
        }
    }
    let Some(front_window_grab) = front_window_grab else {
        eprintln!("--front-window-grab off is required");
        std::process::exit(2);
    };
    match run_direct_command(front_window_grab, &app, &text) {
        Ok(run) => {
            println!("TASK3422_COMMAND=page_connection_place_text");
            println!("TASK3422_FRONT_WINDOW_GRAB={}", run.front_window_grab);
            println!("TASK3422_APP={}", run.app);
            println!("TASK3422_PAGE_CONNECTION={}", run.page_connection);
            println!("TASK3422_APP_STARTS={}", run.app_starts);
            println!("TASK3422_PLACED_TEXT={:?}", run.text);
            println!("TASK3422_BOX_READBACK={:?}", run.readback);
            println!("TASK3422_READBACK_EXACT={}", run.readback == run.text);
            println!("TASK3422_SHARED_PLACER_BOX={}", run.proof.editable_box_name);
            println!("TASK3422_FRONT_AT_START={:?}", run.front_at_start);
            println!("TASK3422_FRONT_AT_END={:?}", run.front_at_end);
            println!(
                "TASK3422_FRONT_RESTORED={}",
                run.front_at_start == run.front_at_end
            );
        }
        Err(error) => {
            eprintln!("TASK3422_ERROR={error}");
            std::process::exit(1);
        }
    }
}
