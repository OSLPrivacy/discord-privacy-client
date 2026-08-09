use osl_privacy_hub::signal_surface_finder::{
    find_active_signal_open_direct_message, find_signal_open_direct_message,
    task_1031_open_direct_message_fixture, SignalSurfaceMatch,
};

fn print_match(source: &str, found: SignalSurfaceMatch) {
    println!("source={source}");
    println!(
        "active_window_count=1 index={} bounds={:?}",
        found.active_window_index, found.window_bounds
    );
    println!(
        "conversation_count=1 index={} bounds={:?}",
        found.conversation_node_index, found.conversation_bounds
    );
    println!(
        "typing_box_count=1 index={} bounds={:?}",
        found.typing_box_node_index, found.typing_box_bounds
    );
    println!("open_direct_message=true");
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let (source, found) = match args.as_slice() {
        [] => (
            "live-signal-desktop",
            find_active_signal_open_direct_message().map_err(|error| error.to_string())?,
        ),
        [flag] if flag == "--live" => (
            "live-signal-desktop",
            find_active_signal_open_direct_message().map_err(|error| error.to_string())?,
        ),
        [flag] if flag == "--fixture-open-direct-message" => {
            let (windows, nodes) = task_1031_open_direct_message_fixture();
            (
                "fixture-open-direct-message",
                find_signal_open_direct_message(&windows, &nodes)
                    .map_err(|error| error.to_string())?,
            )
        }
        _ => {
            return Err(
                "usage: task_1031_signal_finder [--live|--fixture-open-direct-message]".to_owned(),
            );
        }
    };
    print_match(source, found);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("task_1031_signal_finder: {error}");
        std::process::exit(1);
    }
}
