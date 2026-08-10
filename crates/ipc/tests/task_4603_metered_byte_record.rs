use ipc::metered_bytes::{MeteredByteClass, MeteredByteRecord};
use serde_json::{json, Value};

fn fixture_value(byte_class: Value) -> Value {
    json!({
        "month": "2026-08",
        "byte_count": 1,
        "byte_class": byte_class,
        "source_id": "fixture-source"
    })
}

fn accepted(value: Value) -> bool {
    serde_json::from_value::<MeteredByteRecord>(value).is_ok()
}

fn refusal_text(value: Value) -> String {
    match serde_json::from_value::<MeteredByteRecord>(value) {
        Ok(_) => "ACCEPTED (contract breach)".to_owned(),
        Err(error) => error.to_string(),
    }
}

#[test]
fn task_4603_one_strict_record_covers_exactly_six_byte_classes() {
    let other = fixture_value(json!("other"));

    let mut negative_bytes = fixture_value(json!("messages"));
    negative_bytes["byte_count"] = json!(-1);

    let mut message_text = fixture_value(json!("messages"));
    message_text["message_text"] = json!("text must never enter the meter");

    let mut file_name = fixture_value(json!("attachments"));
    file_name["file_name"] = json!("private-name.pdf");

    // Accumulate every breach so TASK 4603b can prove a single failing run
    // names both deliberate mutations instead of panicking on the first one.
    let mut breaches = Vec::new();
    if accepted(other.clone()) {
        breaches.push("seventh byte class other was accepted");
    }
    if accepted(negative_bytes.clone()) {
        breaches.push("negative byte count was accepted");
    }
    if accepted(message_text.clone()) {
        breaches.push("message text was accepted");
    }
    if accepted(file_name.clone()) {
        breaches.push("file name was accepted");
    }

    let records: Vec<_> = MeteredByteClass::ALL
        .into_iter()
        .enumerate()
        .map(|(index, byte_class)| {
            MeteredByteRecord::new(
                "2026-08",
                (index + 1) as u64,
                byte_class,
                format!("fixture-source-{index}"),
            )
        })
        .collect();

    if records.len() != 6 {
        breaches.push("fixture month did not contain exactly 6 records");
    }

    for record in &records {
        println!(
            "TASK4603_RECORD {}",
            serde_json::to_string(record).expect("fixture record serializes")
        );
    }

    let other_error = refusal_text(other);
    let negative_error = refusal_text(negative_bytes);
    let message_text_error = refusal_text(message_text);
    let file_name_error = refusal_text(file_name);

    println!("TASK4603_FIXTURE month=2026-08 records={}", records.len());
    println!("TASK4603_REFUSED class=other error={other_error}");
    println!("TASK4603_REFUSED negative_byte_count=-1 error={negative_error}");
    println!("TASK4603_REFUSED message_text error={message_text_error}");
    println!("TASK4603_REFUSED file_name error={file_name_error}");

    assert!(
        breaches.is_empty(),
        "TASK4603 contract breaches: {}",
        breaches.join("; ")
    );
}
