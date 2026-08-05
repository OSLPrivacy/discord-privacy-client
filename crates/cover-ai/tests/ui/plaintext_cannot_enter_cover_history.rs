use cover_ai::cover_history::{CoverHistory, CoverHistoryAead, CoverScope, SealedHistory};

struct UnusedAead;

impl CoverHistoryAead for UnusedAead {
    type Error = ();

    fn seal(&self, _aad: &[u8], _plaintext: &[u8]) -> Result<SealedHistory, Self::Error> {
        unimplemented!()
    }

    fn open(&self, _aad: &[u8], _sealed: &SealedHistory) -> Result<Vec<u8>, Self::Error> {
        unimplemented!()
    }
}

fn main() {
    let mut history = CoverHistory::new(UnusedAead);
    let _ = history.record(
        CoverScope::from_hash([0u8; 32]),
        "do not retain this private message",
        60,
        100,
    );
}
