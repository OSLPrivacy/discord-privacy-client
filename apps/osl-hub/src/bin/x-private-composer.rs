fn main() {
    if std::env::args().nth(1).as_deref() == Some("task-1106-count-check") {
        run_task_1106_count_check();
        return;
    }

    match osl_privacy_hub::x_private_composer::render_prepared_x_private_composer_fixture() {
        Ok(report) => print!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run_task_1106_count_check() {
    use osl_privacy_hub::x_private_composer::{
        check_x_private_count, XPrivateBoxReader, XPrivateComposerBox,
    };

    struct CommandReader {
        stubbed_to_do_nothing: bool,
    }

    impl XPrivateBoxReader for CommandReader {
        fn read_private_byte_count(
            &mut self,
            private_box: &XPrivateComposerBox,
        ) -> Result<usize, String> {
            if self.stubbed_to_do_nothing {
                // Deliberately do not inspect `private_box`: this is the stale
                // initial count a reader that performed no post-command read
                // would leave behind.
                return Ok(0);
            }
            Ok(private_box.private_byte_count())
        }
    }

    let mut reader = CommandReader {
        stubbed_to_do_nothing: std::env::var_os("OSL_TASK_1106_STUB_X_PRIVATE_BOX_READER")
            .is_some(),
    };
    match check_x_private_count(&mut reader) {
        Ok(check) => print!(
            "TASK1106_FIXTURE_BYTES={}\nTASK1106_COUNT_AFTER_ENTER={}\nTASK1106_COUNT_AFTER_CLEAR={}\nTASK1106_X_COMPOSER_CHARS={}\n",
            check.fixture_bytes,
            check.count_after_enter,
            check.count_after_clear,
            check.x_composer_characters,
        ),
        Err(error) => {
            eprintln!("TASK1106_CHECK_FAILED={error}");
            std::process::exit(1);
        }
    }
}
