fn main() {
    match osl_privacy_hub::x_window_composer::render_prepared_x_text_placement() {
        Ok(report) => print!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
