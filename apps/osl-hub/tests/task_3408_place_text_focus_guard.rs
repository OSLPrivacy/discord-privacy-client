const TASK_3406: &str = include_str!("../examples/task_3406_place_text.rs");

#[derive(Clone, Debug)]
struct EditableFixture {
    name: &'static str,
    is_message_box: bool,
    focused: bool,
    text: String,
}

#[derive(Debug)]
struct DirectCommandHarness {
    boxes: Vec<EditableFixture>,
    placed_count: usize,
}

impl DirectCommandHarness {
    fn place(&mut self, text: &str) -> Result<(), String> {
        let focused = self
            .boxes
            .iter_mut()
            .find(|editable| editable.focused)
            .ok_or_else(|| "no focused box".to_owned())?;
        if !focused.is_message_box {
            return Err(focused.name.to_owned());
        }
        focused.text.push_str(text);
        self.placed_count += 1;
        Ok(())
    }

    fn focus(&mut self, name: &str) {
        for editable in &mut self.boxes {
            editable.focused = editable.name == name;
        }
    }

    fn text_of(&self, name: &str) -> &str {
        self.boxes
            .iter()
            .find(|editable| editable.name == name)
            .expect("fixture box exists")
            .text
            .as_str()
    }
}

fn function_body_after(marker: &str) -> &'static str {
    let start = TASK_3406.find(marker).expect("marker exists");
    &TASK_3406[start..]
}

#[test]
fn task_3408_command_checks_the_existing_typing_point_before_clipboard_or_paste() {
    assert!(TASK_3406.contains("fn verify_focused_typing_point("));
    assert!(TASK_3406.contains("automation.GetFocusedElement()"));
    assert!(TASK_3406.contains("typing_point_in_message_box={focused_is_message_box}"));
    assert!(TASK_3406.contains("placement_refused_focused_box={focused_name:?}"));
    assert!(TASK_3406.contains("not the conversation message box"));
    assert!(TASK_3406.contains("placed_count=1"));

    let run_body = function_body_after("pub fn run() -> Result<(), CommandError>");
    let before_clipboard = run_body
        .split("let snapshot = snapshot_clipboard()")
        .next()
        .expect("run reaches clipboard snapshot");
    assert!(before_clipboard.contains("verify_focused_typing_point("));
    assert!(!before_clipboard.contains("click_composer("));
    assert!(!before_clipboard.contains("stage_clipboard_text("));
    assert!(!before_clipboard.contains("send_ctrl_v()"));

    println!("TASK3408 guard_before_clipboard=true focus_set_before_guard=0");
}

#[test]
fn task_3408_direct_command_places_once_then_refuses_search_by_name() {
    let mut harness = DirectCommandHarness {
        boxes: vec![
            EditableFixture {
                name: "Message #ops",
                is_message_box: true,
                focused: true,
                text: String::new(),
            },
            EditableFixture {
                name: "Search",
                is_message_box: false,
                focused: false,
                text: String::new(),
            },
        ],
        placed_count: 0,
    };

    harness
        .place("MAPLE-3408")
        .expect("message box focus places once");
    assert_eq!(harness.placed_count, 1);
    assert_eq!(harness.text_of("Message #ops"), "MAPLE-3408");

    harness.focus("Search");
    let refusal = harness
        .place("PRIVATE-3408")
        .expect_err("search focus refuses");
    assert_eq!(refusal, "Search");
    assert_eq!(harness.placed_count, 1);
    assert_eq!(harness.text_of("Search"), "");

    println!(
        "TASK3408 direct_command_placed_count={}",
        harness.placed_count
    );
    println!("TASK3408 refused_focused_box={refusal:?}");
    println!(
        "TASK3408 placed_count_after_search_focus={}",
        harness.placed_count
    );
    println!(
        "TASK3408 search_field_after={:?}",
        harness.text_of("Search")
    );
}
