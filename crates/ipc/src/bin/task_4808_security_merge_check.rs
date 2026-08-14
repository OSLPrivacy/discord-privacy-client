use ipc::ordinary_sync::{check_security_field_coverage, SECURITY_MERGE_RULES};

fn main() {
    let fields: Vec<String> = if std::env::args().len() > 1 {
        std::env::args().skip(1).collect()
    } else {
        SECURITY_MERGE_RULES
            .iter()
            .map(|rule| rule.field_name.to_owned())
            .collect()
    };

    match check_security_field_coverage(fields) {
        Ok(coverage) => {
            for field in coverage {
                println!(
                    "TASK4808 security_field={} route={} ordinary_route=last-writer-wins blocked=true",
                    field.field_name,
                    field.route.as_str()
                );
            }
            println!("TASK4808 security_check_exit=0");
        }
        Err(error) => {
            eprintln!("TASK4808 security_check_error={error}");
            std::process::exit(1);
        }
    }
}
