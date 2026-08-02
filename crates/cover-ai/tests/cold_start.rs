//! Regression tests for the no-history cover conversation generator.

#[path = "../src/cold_start.rs"]
mod cold_start;

use std::collections::HashSet;

use cold_start::ColdStartGenerator;
use rand::RngCore;

struct SequenceRng(u64);

impl RngCore for SequenceRng {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn next_u64(&mut self) -> u64 {
        let value = self.0;
        self.0 = self.0.wrapping_add(1);
        value
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        for byte in destination {
            *byte = self.next_u32() as u8;
        }
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(destination);
        Ok(())
    }
}

#[test]
fn one_hundred_cold_starts_do_not_repeat() {
    let mut rng = SequenceRng(41);
    let mut generator = ColdStartGenerator::from_rng(&mut rng);
    let conversations: HashSet<_> = (0..100)
        .map(|_| generator.next_conversation().as_str().to_owned())
        .collect();

    assert_eq!(conversations.len(), 100);
}

#[test]
fn cold_start_is_a_complete_visible_exchange() {
    let mut rng = SequenceRng(7);
    let mut generator = ColdStartGenerator::from_rng(&mut rng);
    let conversation = generator.next_conversation();

    assert_eq!(conversation.as_str().lines().count(), 3);
    assert!(conversation.as_str().lines().all(|line| !line.is_empty()));
}
