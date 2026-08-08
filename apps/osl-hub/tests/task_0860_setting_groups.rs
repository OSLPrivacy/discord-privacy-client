use osl_privacy_hub::setting_groups::{saved_setting_group, SAVED_SETTING_GROUPS};

#[test]
fn task_0860_every_saved_setting_direct_read_names_its_settings_group() {
    assert_eq!(SAVED_SETTING_GROUPS.len(), 29);
    let direct_reads = SAVED_SETTING_GROUPS
        .iter()
        .map(|entry| {
            let group = saved_setting_group(entry.setting).expect("saved setting has a group");
            assert_eq!(group, entry.group);
            format!("{}={group}", entry.setting)
        })
        .collect::<Vec<_>>();

    println!(
        "TASK0860 direct_read_count={} {}",
        direct_reads.len(),
        direct_reads.join(" | ")
    );
}

#[test]
fn task_0860_setting_without_a_group_is_refused_by_name() {
    let missing = "unassigned-setting-0860";
    let error = saved_setting_group(missing).expect_err("unassigned setting must be refused");
    assert!(error.contains(missing));
    println!("TASK0860 refusal={error}");
}
