//! T14-T11: OSL Chat fetches on pointer arrival, before a conversation opens.

use std::collections::BTreeMap;

use osl_privacy_hub::{
    eager_fetch::{
        CipherStoreTransport, EagerFetchDriver, EncryptedBurnQueue, LocalMessageStore,
        PointerArrival,
    },
    osl_chat_delivery::receive_osl_chat_pointer,
};

#[derive(Default)]
struct Transport(BTreeMap<String, Vec<u8>>);
impl CipherStoreTransport for Transport {
    fn fetch(&mut self, blob_id: &str, _: &[u8]) -> Result<Vec<u8>, String> {
        self.0
            .get(blob_id)
            .cloned()
            .ok_or_else(|| "missing payload".to_owned())
    }
    fn burn(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct Store(BTreeMap<String, Vec<u8>>);
impl LocalMessageStore for Store {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        self.0.insert(blob_id.to_owned(), ciphertext.to_vec());
        Ok(())
    }
    fn destroy_local(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn t14_t11_payload_is_local_before_the_user_opens_the_conversation() {
    let path = std::env::temp_dir().join(format!("osl-chat-eager-{}", uuid::Uuid::new_v4()));
    let mut transport = Transport::default();
    transport
        .0
        .insert("pointer-1".to_owned(), b"ciphertext".to_vec());
    let queue = EncryptedBurnQueue::new(&path, [7; 32]);
    let mut driver = EagerFetchDriver::new(transport, Store::default(), queue);
    let pointer = PointerArrival {
        blob_id: "pointer-1".to_owned(),
        fetch_cap: vec![1],
        manage_cap: vec![2],
    };

    // No chat view/open callback exists in this call: pointer arrival itself
    // must put the payload in the durable local store.
    receive_osl_chat_pointer(&mut driver, &pointer).expect("arrival fetches and persists");
    let (_, store, _) = driver.into_parts();
    assert_eq!(store.0.get("pointer-1"), Some(&b"ciphertext".to_vec()));
}
