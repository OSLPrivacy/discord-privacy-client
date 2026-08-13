use keystore::{
    AccountDevice, AccountRootKey, DevicePrivateKeys, StoredDeviceList,
    COPIED_IDENTITY_FILE_REFUSAL,
};

#[test]
fn task_4800_account_root_and_signed_device_list() {
    let temp = tempfile::tempdir().expect("tempdir");
    let sealer = keystore::MemorySealer::new();
    let root = AccountRootKey::generate();
    println!("account_root_id: {}", root.account_id());

    let mut active_devices: Vec<AccountDevice> = Vec::new();
    let mut stored: Option<StoredDeviceList> = None;
    let mut private_files = Vec::new();
    let mut devices = Vec::new();

    for version in 1..=3 {
        let device_name = format!("device-{version}");
        let device = DevicePrivateKeys::generate_on_device(device_name);
        let private_path = temp.path().join(format!("device-{version}.key"));
        device
            .save_sealed_private_key_file(&private_path, &sealer)
            .expect("write device private key file");
        let on_disk = std::fs::read(&private_path).expect("read device private key file");
        // TASK 5402: the device private key file is device-sealed, so its own
        // sixty-four secret bytes must not appear in it, and the sealed reader
        // must return exactly those bytes.
        let own_key_hits = on_disk
            .windows(keystore::DEVICE_PRIVATE_KEY_FILE_BYTES)
            .filter(|window| *window == device.private_key_file_bytes())
            .count();
        assert_eq!(
            own_key_hits, 0,
            "device-{version}.key carried its own private key in the clear"
        );
        let opened = keystore::DevicePrivateKeys::open_sealed_private_key_file(
            &private_path,
            &sealer,
        )
        .expect("the device sealer reopens the sealed private key file");
        assert_eq!(opened[..], device.private_key_file_bytes()[..]);
        private_files.push(on_disk);
        active_devices.push(root.sign_device(device.public_keys()));
        devices.push(device);

        let signed = root.sign_device_list(version, active_devices.clone());
        let accepted = StoredDeviceList::accept_next(stored.as_ref(), signed)
            .expect("accept higher-version signed device list");
        println!(
            "accepted list version {} devices {} names {}",
            accepted.version(),
            accepted.device_names().len(),
            accepted.device_names().join(",")
        );
        stored = Some(accepted);
    }

    let max_shared_run = max_pairwise_shared_run(&private_files);
    let foreign_private_key_hits = count_foreign_private_key_hits(&devices, &private_files);
    println!("device_private_key_files: {}", private_files.len());
    println!("max_shared_run_bytes: {max_shared_run}");
    println!("foreign_private_key_hits: {foreign_private_key_hits}");
    assert_eq!(private_files.len(), 3);
    assert!(
        max_shared_run < 16,
        "device private key files shared a run of {max_shared_run} bytes"
    );
    assert_eq!(foreign_private_key_hits, 0);

    active_devices.retain(|device| device.device.device_name != "device-2");
    let signed_v4 = root.sign_device_list(4, active_devices.clone());
    let accepted_v4 =
        StoredDeviceList::accept_next(stored.as_ref(), signed_v4).expect("accept version 4");
    println!(
        "accepted list version {} devices {} names {}",
        accepted_v4.version(),
        accepted_v4.device_names().len(),
        accepted_v4.device_names().join(",")
    );

    let stale_v4 = root.sign_device_list(4, active_devices);
    let stale_err = StoredDeviceList::accept_next(Some(&accepted_v4), stale_v4)
        .expect_err("same-version device list must be refused");
    println!("stale_version_refusal: {stale_err}");
    assert_eq!(stale_err.to_string(), "device list version must go up");

    println!("copied_identity_file_refusal: {COPIED_IDENTITY_FILE_REFUSAL}");
    assert!(COPIED_IDENTITY_FILE_REFUSAL.contains("copied identity file"));
    assert!(COPIED_IDENTITY_FILE_REFUSAL.contains("removing a device impossible"));
}

fn max_pairwise_shared_run(files: &[Vec<u8>]) -> usize {
    let mut max_run = 0;
    for left in 0..files.len() {
        for right in (left + 1)..files.len() {
            max_run = max_run.max(longest_common_contiguous_run(&files[left], &files[right]));
        }
    }
    max_run
}

fn longest_common_contiguous_run(left: &[u8], right: &[u8]) -> usize {
    let mut rows = vec![0usize; right.len() + 1];
    let mut best = 0;
    for &left_byte in left {
        for index in (0..right.len()).rev() {
            rows[index + 1] = if left_byte == right[index] {
                rows[index] + 1
            } else {
                0
            };
            best = best.max(rows[index + 1]);
        }
    }
    best
}

fn count_foreign_private_key_hits(devices: &[DevicePrivateKeys], files: &[Vec<u8>]) -> usize {
    let mut hits = 0;
    for (device_index, device) in devices.iter().enumerate() {
        for key in device.private_key_chunks() {
            for (file_index, file) in files.iter().enumerate() {
                if device_index != file_index && contains_slice(file, &key) {
                    hits += 1;
                }
            }
        }
    }
    hits
}

fn contains_slice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|candidate| candidate == needle)
}
