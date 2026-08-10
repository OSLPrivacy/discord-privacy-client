use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let program = args
        .next()
        .unwrap_or_else(|| "task_3123_public_name_proving_services".into());
    let service = args.next();

    if args.next().is_some() {
        eprintln!("usage: {} [proving-service]", program.to_string_lossy());
        return ExitCode::FAILURE;
    }

    match service {
        None => {
            println!(
                "TASK3123_ALLOWED_PROVING_SERVICES=[{}]",
                keystore::allowed_proving_services().join(",")
            );
            println!(
                "TASK3123_PROOFS_NEEDED={}",
                keystore::required_outside_proof_count()
            );
            ExitCode::SUCCESS
        }
        Some(service) => match service.into_string() {
            Ok(service) => match keystore::validate_proving_service(&service) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("TASK3123_NOT_ALLOWED={error}");
                    ExitCode::FAILURE
                }
            },
            Err(_) => {
                eprintln!("proving service is not valid Unicode");
                ExitCode::FAILURE
            }
        },
    }
}
