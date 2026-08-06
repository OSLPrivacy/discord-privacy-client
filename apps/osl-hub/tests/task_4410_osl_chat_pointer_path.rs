use std::collections::BTreeMap;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use osl_privacy_hub::eager_fetch::{
    CipherStoreTransport, EagerFetchDriver, EncryptedBurnQueue, FetchReservations,
    LocalMessageStore, PointerArrival,
};
use osl_privacy_hub::osl_chat_delivery::receive_osl_chat_authorized_fetch;
use osl_privacy_hub::realtime_client::{
    BlobId, CarrierPointer, RealtimeClient, FRAME_BYTES, TICK_INTERVAL,
};
use osl_privacy_hub::realtime_pipe::drain_scheduled_fetches;

#[derive(Clone, Default)]
struct SharedTransport {
    state: Arc<Mutex<TransportState>>,
}

#[derive(Default)]
struct TransportState {
    blobs: BTreeMap<String, Vec<u8>>,
    fetches: BTreeMap<String, usize>,
}

impl SharedTransport {
    fn insert(&self, blob_id: String, body: Vec<u8>) {
        self.state.lock().unwrap().blobs.insert(blob_id, body);
    }

    fn duplicate_fetches(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .fetches
            .values()
            .map(|count| count.saturating_sub(1))
            .sum()
    }
}

impl CipherStoreTransport for SharedTransport {
    fn fetch(&mut self, blob_id: &str, _: &[u8]) -> Result<Vec<u8>, String> {
        let body = {
            let mut state = self.state.lock().unwrap();
            *state.fetches.entry(blob_id.to_owned()).or_default() += 1;
            state
                .blobs
                .get(blob_id)
                .cloned()
                .ok_or_else(|| "missing payload".to_owned())?
        };
        thread::sleep(Duration::from_millis(2));
        Ok(body)
    }

    fn burn(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct SharedStore {
    rows: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
}

impl SharedStore {
    fn contains(&self, blob_id: &str) -> bool {
        self.rows.lock().unwrap().contains_key(blob_id)
    }

    fn len(&self) -> usize {
        self.rows.lock().unwrap().len()
    }
}

impl LocalMessageStore for SharedStore {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        self.rows
            .lock()
            .unwrap()
            .insert(blob_id.to_owned(), ciphertext.to_vec());
        Ok(())
    }

    fn destroy_local(&mut self, blob_id: &str) -> Result<(), String> {
        self.rows.lock().unwrap().remove(blob_id);
        Ok(())
    }
}

fn blob(index: u8) -> BlobId {
    BlobId::from_bytes([index; 16])
}

fn pointer(index: u8) -> PointerArrival {
    PointerArrival {
        blob_id: blob(index).to_hex(),
        fetch_cap: format!("fetch-cap-{index:02}").into_bytes(),
        manage_cap: format!("manage-cap-{index:02}").into_bytes(),
    }
}

fn wakeup(tag: u8, blob: u8) -> String {
    let body = format!(
        r#"{{"delivery_tag":"{}","blob_id":"{}"}}"#,
        format!("{tag:02x}").repeat(16),
        format!("{blob:02x}").repeat(16),
    );
    format!("{body:<FRAME_BYTES$}")
}

fn pointer_path_fetch(
    driver: &mut EagerFetchDriver<SharedTransport, SharedStore>,
    index: u8,
    enabled: bool,
) -> Result<usize, String> {
    let mut client = RealtimeClient::new(Duration::ZERO);
    client.remember_carrier_pointer(
        blob(index),
        CarrierPointer::from_carrier(format!("fetch-cap-{index:02}")),
    );
    client
        .receive_frame(&wakeup(index, index))
        .map_err(|error| format!("wakeup failed: {error:?}"))?;
    let report = drain_scheduled_fetches(
        &mut client,
        |fetch| {
            receive_osl_chat_authorized_fetch(driver, fetch, enabled)?;
            Ok::<(), String>(())
        },
        |decoy| decoy.fetch_with(|_, _| Ok::<(), String>(())),
    )?;
    Ok(report.authorized_fetches)
}

fn driver(
    transport: SharedTransport,
    store: SharedStore,
    reservations: FetchReservations,
    path: std::path::PathBuf,
    key: u8,
) -> EagerFetchDriver<SharedTransport, SharedStore> {
    EagerFetchDriver::with_reservations(
        transport,
        store,
        EncryptedBurnQueue::new(path, [key; 32]),
        reservations,
    )
}

#[test]
fn task_4410_osl_chats_pointer_path_opens_fast_and_does_not_race_polling() {
    let temp = tempfile::tempdir().unwrap();

    let transport = SharedTransport::default();
    let store = SharedStore::default();
    let reservations = FetchReservations::default();

    let fast = pointer(1);
    transport.insert(fast.blob_id.clone(), b"pointer path plaintext".to_vec());
    let mut fast_driver = driver(
        transport.clone(),
        store.clone(),
        reservations.clone(),
        temp.path().join("fast.enc"),
        1,
    );
    assert_eq!(pointer_path_fetch(&mut fast_driver, 1, true).unwrap(), 1);
    let pointer_open_seconds = TICK_INTERVAL.as_secs();
    assert!(store.contains(&fast.blob_id));
    assert!(pointer_open_seconds < 15);

    let race_transport = SharedTransport::default();
    let race_store = SharedStore::default();
    let race_reservations = FetchReservations::default();
    for index in 2..52 {
        let arrival = pointer(index);
        race_transport.insert(
            arrival.blob_id.clone(),
            format!("race payload {index}").into_bytes(),
        );
        let barrier = Arc::new(Barrier::new(2));
        let pointer_barrier = Arc::clone(&barrier);
        let poll_barrier = Arc::clone(&barrier);
        let mut pointer_driver = driver(
            race_transport.clone(),
            race_store.clone(),
            race_reservations.clone(),
            temp.path().join(format!("race-pointer-{index}.enc")),
            index,
        );
        let mut poll_driver = driver(
            race_transport.clone(),
            race_store.clone(),
            race_reservations.clone(),
            temp.path().join(format!("race-poll-{index}.enc")),
            index.wrapping_add(1),
        );
        let polled = arrival.clone();

        let pointer_thread = thread::spawn(move || {
            pointer_barrier.wait();
            pointer_path_fetch(&mut pointer_driver, index, true)
        });
        let poll_thread = thread::spawn(move || {
            poll_barrier.wait();
            poll_driver.on_poll_arrival(&polled)
        });

        pointer_thread.join().unwrap().unwrap();
        poll_thread.join().unwrap().unwrap();
    }
    let duplicate_fetches = race_transport.duplicate_fetches();
    assert_eq!(duplicate_fetches, 0);
    assert_eq!(race_store.len(), 50);

    let off_transport = SharedTransport::default();
    let off_store = SharedStore::default();
    let off_reservations = FetchReservations::default();
    for index in 60..110 {
        let arrival = pointer(index);
        off_transport.insert(
            arrival.blob_id.clone(),
            format!("fallback payload {index}").into_bytes(),
        );
        let mut pointer_driver = driver(
            off_transport.clone(),
            off_store.clone(),
            off_reservations.clone(),
            temp.path().join(format!("off-pointer-{index}.enc")),
            index,
        );
        assert_eq!(
            pointer_path_fetch(&mut pointer_driver, index, false).unwrap(),
            1
        );
        assert!(
            !off_store.contains(&arrival.blob_id),
            "disabled pointer path must not fetch before the poll"
        );
        let mut poll_driver = driver(
            off_transport.clone(),
            off_store.clone(),
            off_reservations.clone(),
            temp.path().join(format!("off-poll-{index}.enc")),
            index.wrapping_add(1),
        );
        assert!(poll_driver.on_poll_arrival(&arrival).unwrap());
    }
    let fallback_lost = 50usize.saturating_sub(off_store.len());
    assert_eq!(fallback_lost, 0);

    println!("TASK4410_POINTER_PATH_ENABLED=true");
    println!("TASK4410_POINTER_OPEN_SECONDS={pointer_open_seconds}");
    println!("TASK4410_POINTER_OPEN_UNDER_SECONDS=15");
    println!("TASK4410_POLL_INTERVAL_SECONDS=30");
    println!("TASK4410_RACE_MESSAGES=50");
    println!("TASK4410_DUPLICATE_FETCHES={duplicate_fetches}");
    println!("TASK4410_POINTER_PATH_OFF=false");
    println!("TASK4410_POINTER_PATH_OFF_FALLBACK_SECONDS=30");
    println!(
        "TASK4410_POINTER_PATH_OFF_MESSAGES_OPENED={}",
        off_store.len()
    );
    println!("TASK4410_POINTER_PATH_OFF_MESSAGES_LOST={fallback_lost}");
}
