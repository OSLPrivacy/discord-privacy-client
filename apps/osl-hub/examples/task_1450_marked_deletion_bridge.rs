//! Request bridge for TASK 1450.
//!
//! The deletion review screen builds the `count` and `delete` requests; this
//! hands one of them to the real TASK 1449 command and prints the reply it
//! produced, unchanged. Recording those replies is what lets the screen's
//! check run against what the command actually answers rather than against a
//! hand-written imitation of it.
//!
//! usage: task_1450_marked_deletion_bridge <count|delete> <request-json>

use osl_privacy_hub::pro_marked_deletion::run_pro_marked_deletion_command;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [command, request_json] = args.as_slice() else {
        eprintln!("usage: task_1450_marked_deletion_bridge <count|delete> <request-json>");
        std::process::exit(2);
    };
    if command != "count" && command != "delete" {
        eprintln!("TASK1450_ERROR=unknown command {command}");
        std::process::exit(2);
    }
    println!("{}", run_pro_marked_deletion_command(command, request_json));
}
