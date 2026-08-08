use std::collections::{BTreeMap, HashMap, HashSet};

#[path = "../../../apps/osl-hub/src/bundled_model_pack.rs"]
mod bundled_model_pack;

use bundled_model_pack::{
    ensure_bundled_model_pack, BundledCoverWriter, CoverShapeConstraints, CoverWriterObserver,
};
use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

const RUNS: usize = 10;
const PRIVATE_CHARACTERS_PER_MESSAGE: usize = 200;

fn private_character(run: usize) -> char {
    char::from_u32(0xE100 + u32::try_from(run).expect("run fits u32"))
        .expect("fixture uses Unicode private-use characters")
}

fn private_message(run: usize) -> String {
    std::iter::repeat_n(private_character(run), PRIVATE_CHARACTERS_PER_MESSAGE).collect()
}

#[derive(Default)]
struct PrivacyWatch {
    private_characters: HashSet<char>,
    writer_input_events: usize,
    writer_output_events: usize,
    machine_egress_events: usize,
    writer_private_characters_seen: usize,
    writer_output_private_characters: usize,
    private_characters_left_machine: usize,
    seen_details: BTreeMap<char, usize>,
}

impl PrivacyWatch {
    fn for_messages(messages: &[String]) -> Self {
        Self {
            private_characters: messages
                .iter()
                .flat_map(|message| message.chars())
                .collect(),
            ..Self::default()
        }
    }

    fn count_private(&mut self, bytes: &[u8], remember: bool) -> usize {
        let mut count = 0;
        for character in String::from_utf8_lossy(bytes).chars() {
            if self.private_characters.contains(&character) {
                count += 1;
                if remember {
                    *self.seen_details.entry(character).or_default() += 1;
                }
            }
        }
        count
    }

    fn machine_egress(&mut self, cover: &str) {
        self.machine_egress_events += 1;
        self.private_characters_left_machine += self.count_private(cover.as_bytes(), false);
    }

    fn named_seen_characters(&self) -> String {
        if self.seen_details.is_empty() {
            return "none".to_owned();
        }
        self.seen_details
            .iter()
            .map(|(character, count)| format!("U+{:04X}={count}", *character as u32))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl CoverWriterObserver for PrivacyWatch {
    fn writer_received(&mut self, input: &[u8]) {
        self.writer_input_events += 1;
        self.writer_private_characters_seen += self.count_private(input, true);
    }

    fn writer_returned(&mut self, output: &[u8]) {
        self.writer_output_events += 1;
        self.writer_output_private_characters += self.count_private(output, false);
    }
}

#[test]
fn task_3523_private_characters_never_reach_the_ai_writer_or_leave_the_machine() {
    let messages = (0..RUNS).map(private_message).collect::<Vec<_>>();
    assert!(messages
        .iter()
        .all(|message| message.chars().count() == PRIVATE_CHARACTERS_PER_MESSAGE));

    let install = tempfile::tempdir().expect("fresh model-pack root");
    let status = ensure_bundled_model_pack(install.path())
        .expect("bundled local model installs and verifies");
    let mut writer =
        BundledCoverWriter::load(&status.artifact_path).expect("verified local writer loads");
    let cipher = ConversationCipher::from_salt(b"task-3523-conversation");
    let detection_key = b"task-3523-secret-detection-key";
    let mut relay = HashMap::<[u8; TOKEN_ID_BYTES], String>::new();
    let mut covers = HashSet::new();
    let mut watch = PrivacyWatch::for_messages(&messages);
    let mut exact_readbacks = 0;

    for (run, private_message) in messages.iter().enumerate() {
        let shape = CoverShapeConstraints::new(
            private_message.chars().count(),
            vec![u32::try_from(private_message.chars().count()).expect("fixture length fits u32")],
        )
        .expect("bounded count-only writer request");
        let entropy = writer
            .generate_cover_entropy_observed(&shape, &mut watch)
            .expect("local writer runs from the observed count-only request")
            .into_bytes();

        let mut pointer = [0u8; TOKEN_ID_BYTES];
        pointer[..8].copy_from_slice(&(run as u64 + 1).to_be_bytes());
        pointer[8..].copy_from_slice(&entropy[..12]);
        relay.insert(pointer, private_message.clone());

        let cover = encode_token(&cipher, detection_key, &pointer);
        watch.machine_egress(&cover);
        covers.insert(cover.clone());
        let opened = decode_token(&cipher, detection_key, &cover)
            .filter(|recovered_pointer| recovered_pointer == &pointer)
            .and_then(|recovered_pointer| relay.remove(&recovered_pointer));
        exact_readbacks += usize::from(opened.as_ref() == Some(private_message));
    }

    println!(
        "TASK3523 messages={} writer_input_events={} writer_output_events={} writer_private_characters_seen={} writer_output_private_characters={} machine_egress_events={} private_characters_left_machine={} cover_messages={} exact_readbacks={}/{} named_private_characters_seen={}",
        RUNS,
        watch.writer_input_events,
        watch.writer_output_events,
        watch.writer_private_characters_seen,
        watch.writer_output_private_characters,
        watch.machine_egress_events,
        watch.private_characters_left_machine,
        covers.len(),
        exact_readbacks,
        RUNS,
        watch.named_seen_characters(),
    );

    assert_eq!(
        watch.writer_private_characters_seen,
        0,
        "AI writer saw private characters: {}",
        watch.named_seen_characters()
    );
    assert_eq!(
        watch.private_characters_left_machine, 0,
        "private characters left the machine"
    );
    assert_eq!(watch.writer_input_events, RUNS);
    assert_eq!(watch.writer_output_events, RUNS);
    assert_eq!(watch.writer_output_private_characters, 0);
    assert_eq!(watch.machine_egress_events, RUNS);
    assert_eq!(covers.len(), RUNS);
    assert_eq!(exact_readbacks, RUNS);
    assert!(relay.is_empty());
}
