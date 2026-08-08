fn main() {
    match osl_privacy_hub::x_private_composer::render_prepared_x_private_composer_fixture() {
        Ok(report) => print!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
