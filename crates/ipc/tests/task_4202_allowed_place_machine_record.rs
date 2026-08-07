use ipc::allowed_places::{
    read_allowed_place_machine_record_json, AllowedPlaceAccess, AllowedPlaceMachineRecord,
    TASK_4202_EXAMPLE_ALLOWED_PLACE_RECORD,
};

#[test]
fn task_4202_reads_one_six_part_allowed_place_record_and_names_missing_parts() {
    let record = read_allowed_place_machine_record_json(TASK_4202_EXAMPLE_ALLOWED_PLACE_RECORD)
        .expect("example allowed-place record is readable");

    println!("TASK4202_READER=read_allowed_place_machine_record_json");
    println!("TASK4202_FILLED_PART_COUNT={}", record.filled_part_count());
    println!("TASK4202_APP={}", record.app);
    println!("TASK4202_EXACT_PLACE_NAME={}", record.exact_place_name);
    println!("TASK4202_ACCESS={}", record.access.as_str());
    println!("TASK4202_SOURCE_TASK={}", record.source_task);
    println!("TASK4202_TYPING_BOX_CHECK={}", record.typing_box_check);
    println!("TASK4202_PERSON_CAN={}", record.person_can);

    assert_eq!(record.filled_part_count(), 6);
    assert_eq!(record.app, "Telegram");
    assert_eq!(record.exact_place_name, "Saved Messages");
    assert_eq!(record.access, AllowedPlaceAccess::Approved);
    assert_eq!(record.source_task, 4202);
    assert_eq!(
        record.typing_box_check,
        "native_telegram_adapter::tests::drive_the_real_telegram_composer"
    );
    assert_eq!(
        record.person_can,
        "Send protected text to the user's own Telegram saved chat."
    );

    for missing_part in AllowedPlaceMachineRecord::REQUIRED_PARTS {
        let mut value: serde_json::Value =
            serde_json::from_str(TASK_4202_EXAMPLE_ALLOWED_PLACE_RECORD)
                .expect("example record is JSON");
        value
            .as_object_mut()
            .expect("example record is an object")
            .remove(missing_part);
        let body = serde_json::to_string(&value).expect("mutated record stays JSON");
        let error =
            read_allowed_place_machine_record_json(&body).expect_err("missing part is refused");
        let message = error.to_string();

        println!("TASK4202_MISSING_PART_REFUSED={missing_part}");
        println!("TASK4202_MISSING_PART_ERROR={message}");
        assert!(
            message.contains(missing_part),
            "missing part name {missing_part:?} must appear in {message:?}"
        );
    }

    let mut half_filled: serde_json::Value =
        serde_json::from_str(TASK_4202_EXAMPLE_ALLOWED_PLACE_RECORD)
            .expect("example record is JSON");
    half_filled
        .as_object_mut()
        .expect("example record is an object")
        .remove("typingBoxCheck");
    let half_filled = serde_json::to_string(&half_filled).expect("throwaway record stays JSON");
    let output = std::process::Command::new(std::env::current_exe().expect("current test binary"))
        .arg("task_4202_missing_typing_box_check_probe_child")
        .arg("--exact")
        .arg("--ignored")
        .arg("--nocapture")
        .env("TASK4202_RECORD_JSON", half_filled)
        .output()
        .expect("run throwaway missing-check probe");
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!(
        "TASK4202B_MISSING_TYPING_BOX_CHECK_EXIT={}",
        output.status.code().unwrap_or(-1)
    );
    println!("TASK4202B_MISSING_TYPING_BOX_CHECK_ERROR={}", stderr.trim());
    println!(
        "TASK4202B_REAL_FILLED_PART_COUNT={}",
        read_allowed_place_machine_record_json(TASK_4202_EXAMPLE_ALLOWED_PLACE_RECORD)
            .expect("real example record remains complete")
            .filled_part_count()
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("typingBoxCheck"));
}

#[test]
#[ignore]
fn task_4202_missing_typing_box_check_probe_child() {
    let input = std::env::var("TASK4202_RECORD_JSON").expect("child record JSON is supplied");
    match read_allowed_place_machine_record_json(&input) {
        Ok(_) => std::process::exit(0),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
