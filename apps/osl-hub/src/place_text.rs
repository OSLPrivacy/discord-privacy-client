//! Shared text-placement actions for provider composers.
//!
//! Provider adapters supply only three mechanism calls.  The transaction in
//! this module owns the important policy: read back the exact bytes that were
//! requested, clear after every successful write (including a bad read-back),
//! and independently prove that the clear left no bytes behind.

/// The only composer mutations available to a text-placement integration.
///
/// Keeping this provider-neutral prevents individual adapters from growing a
/// second, unreviewed input path.  A provider driver may use UI Automation,
/// browser accessibility, or another attested mechanism behind these calls.
pub trait PlaceTextActions {
    type Error;

    fn place_text(&self, text: &[u8]) -> Result<(), Self::Error>;
    fn read_back_text(&self) -> Result<Vec<u8>, Self::Error>;
    fn clear_text(&self) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactTextPlacementReceipt {
    pub placed_bytes: usize,
    pub read_back_bytes: usize,
    pub cleared_bytes: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ExactTextPlacementError<E> {
    EmptyFixture,
    Place(E),
    ReadBack(E),
    ReadBackMismatch { expected: Vec<u8>, actual: Vec<u8> },
    Clear(E),
    ClearReadBack(E),
    ClearLeftBytes(Vec<u8>),
}

/// Place marked bytes, read the provider's value back exactly, then clear and
/// prove the provider reports zero bytes.
///
/// Once `place_text` succeeds, `clear_text` is attempted even when the first
/// read-back errors or differs.  Thus a failed proof does not strand fixture
/// text in a person's composer.
pub fn place_text_exact_read_back_and_clear<A>(
    actions: &A,
    marked_bytes: &[u8],
) -> Result<ExactTextPlacementReceipt, ExactTextPlacementError<A::Error>>
where
    A: PlaceTextActions + ?Sized,
{
    if marked_bytes.is_empty() {
        return Err(ExactTextPlacementError::EmptyFixture);
    }

    actions
        .place_text(marked_bytes)
        .map_err(ExactTextPlacementError::Place)?;

    // Capture every outcome before returning. In particular, the clear is not
    // skipped just because the exact read-back check is going to fail.
    let read_back = actions.read_back_text();
    let clear = actions.clear_text();
    let after_clear = actions.read_back_text();

    clear.map_err(ExactTextPlacementError::Clear)?;
    let cleared = after_clear.map_err(ExactTextPlacementError::ClearReadBack)?;
    if !cleared.is_empty() {
        return Err(ExactTextPlacementError::ClearLeftBytes(cleared));
    }

    let read_back = read_back.map_err(ExactTextPlacementError::ReadBack)?;
    if read_back != marked_bytes {
        return Err(ExactTextPlacementError::ReadBackMismatch {
            expected: marked_bytes.to_vec(),
            actual: read_back,
        });
    }

    Ok(ExactTextPlacementReceipt {
        placed_bytes: marked_bytes.len(),
        read_back_bytes: read_back.len(),
        cleared_bytes: cleared.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fixture {
        value: Mutex<Vec<u8>>,
        alter_read_back: bool,
    }

    impl PlaceTextActions for Fixture {
        type Error = ();

        fn place_text(&self, text: &[u8]) -> Result<(), Self::Error> {
            *self.value.lock().unwrap() = text.to_vec();
            Ok(())
        }

        fn read_back_text(&self) -> Result<Vec<u8>, Self::Error> {
            let mut value = self.value.lock().unwrap().clone();
            if self.alter_read_back && !value.is_empty() {
                value[0] ^= 1;
            }
            Ok(value)
        }

        fn clear_text(&self) -> Result<(), Self::Error> {
            self.value.lock().unwrap().clear();
            Ok(())
        }
    }

    #[test]
    fn exact_transaction_clears_even_when_one_read_back_byte_changes() {
        let fixture = Fixture {
            value: Mutex::new(Vec::new()),
            alter_read_back: true,
        };
        let error = place_text_exact_read_back_and_clear(&fixture, b"marked")
            .expect_err("a one-byte read-back change must fail");
        assert!(matches!(
            error,
            ExactTextPlacementError::ReadBackMismatch { .. }
        ));
        assert!(fixture.value.lock().unwrap().is_empty());
    }
}
