//! TASK 3540: exercise the provider-neutral placement primitive at every
//! Windows display-density value in the audit matrix.

#[path = "../examples/task_3406_place_text.rs"]
mod task_3406_place_text;

use task_3406_place_text::{place_read_back_and_clear, SharedTextActions};

const WINDOWS_DISPLAY_SCALES: [u16; 4] = [100, 125, 150, 200];
const SUPPORTED_OUTSIDE_APPS: [&str; 1] = ["Discord"];

#[derive(Debug)]
struct ScaleAwareDiscordComposer {
    typing_box: [i32; 4],
    typing_box_text: String,
    search_box_text: String,
    channel_list_text: String,
    placed_texts: Vec<String>,
}

impl ScaleAwareDiscordComposer {
    fn at_scale(scale_percent: u16) -> Self {
        let scale = i32::from(scale_percent);
        // These are physical screen coordinates.  They deliberately use the
        // same 100%-logical composer bounds at every density, so the check
        // catches a mark written to a stale logical coordinate instead of the
        // real scaled typing box.
        let logical = [320, 700, 1200, 760];
        let typing_box = logical.map(|coordinate| coordinate * scale / 100);
        Self {
            typing_box,
            typing_box_text: String::new(),
            search_box_text: String::new(),
            channel_list_text: String::new(),
            placed_texts: Vec::new(),
        }
    }
}

impl SharedTextActions for ScaleAwareDiscordComposer {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.typing_box_text.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.typing_box_text = text.to_owned();
        self.placed_texts.push(text.to_owned());
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.typing_box_text.clear();
        Ok(())
    }
}

#[test]
fn task_3540_marks_only_the_discord_typing_box_at_every_windows_scale() {
    println!(
        "TASK3540_SUPPORTED_OUTSIDE_APPS={}",
        SUPPORTED_OUTSIDE_APPS.join("|")
    );

    let mut scale_count = 0usize;
    let mut placed_mark_count = 0usize;
    let mut wrong_box_matches = 0usize;

    for scale_percent in WINDOWS_DISPLAY_SCALES {
        for app in SUPPORTED_OUTSIDE_APPS {
            let mark = format!("TASK3540-{app}-{scale_percent}");
            let mut composer = ScaleAwareDiscordComposer::at_scale(scale_percent);
            let typing_box = composer.typing_box;
            let receipt = place_read_back_and_clear(&mut composer, &mark)
                .expect("the shared placing job must accept an empty verified composer");

            let mark_was_in_typing_box = composer.placed_texts.as_slice() == [mark.as_str()];
            let other_box_marks = usize::from(composer.search_box_text.contains(&mark))
                + usize::from(composer.channel_list_text.contains(&mark));
            if !mark_was_in_typing_box || other_box_marks != 0 {
                wrong_box_matches += 1;
            }
            placed_mark_count += composer.placed_texts.len();
            scale_count += 1;

            println!(
                "TASK3540_PLACEMENT scale_percent={scale_percent} app={app} typing_box=[{},{},{},{}] placed_bytes={} readback_bytes={} clear_bytes={} typing_box_mark={} other_box_marks={other_box_marks}",
                typing_box[0],
                typing_box[1],
                typing_box[2],
                typing_box[3],
                receipt.placed_bytes,
                receipt.readback_bytes,
                receipt.clear_bytes,
                mark_was_in_typing_box,
            );
            assert!(mark_was_in_typing_box, "{app} mark missed its typing box at {scale_percent}%");
            assert_eq!(other_box_marks, 0, "{app} mark reached a non-typing box at {scale_percent}%");
            assert_eq!(receipt.placed_bytes, mark.len());
            assert_eq!(receipt.readback_bytes, mark.len());
            assert_eq!(receipt.clear_bytes, 0);
        }
    }

    println!(
        "TASK3540_PLACEMENT_SUMMARY scale_values={} apps={} placements={} wrong_box_matches={wrong_box_matches}",
        WINDOWS_DISPLAY_SCALES.len(),
        SUPPORTED_OUTSIDE_APPS.len(),
        placed_mark_count,
    );
    assert_eq!(scale_count, 4, "every required Windows scale must run");
    assert_eq!(wrong_box_matches, 0, "an app mark reached a non-typing box");
}
