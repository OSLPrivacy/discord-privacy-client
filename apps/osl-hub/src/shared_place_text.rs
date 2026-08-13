//! The one placement job used by native private composers.
//!
//! A carrier adapter may find and bind its own box, but it must not grow a
//! second way of putting text in OSL's private editor.  This job owns the
//! write, byte-exact read-back, byte counter, and clear contract.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTextPlacementReceipt {
    pub private_bytes: usize,
    pub readback: String,
    pub counter_text: String,
    pub carrier_characters: usize,
}

pub trait SharedPlaceTextTarget {
    /// This is the same input boundary used by TASK 3406: one named placement
    /// operation, not provider-specific DOM or keyboard code.
    fn place_text(&mut self, text: &str) -> Result<(), String>;
    fn read_text(&self) -> Result<String, String>;
    /// The independently observed carrier box. It must remain empty while the
    /// private draft is being prepared.
    fn carrier_characters(&self) -> Result<usize, String>;
}

pub fn place_private_text_exact(
    target: &mut dyn SharedPlaceTextTarget,
    text: &str,
) -> Result<PrivateTextPlacementReceipt, String> {
    if text.is_empty() {
        return Err("shared place-text refuses an empty private draft".to_owned());
    }
    target.place_text(text)?;
    receipt(target, text)
}

pub fn clear_private_text_exact(
    target: &mut dyn SharedPlaceTextTarget,
) -> Result<PrivateTextPlacementReceipt, String> {
    target.place_text("")?;
    receipt(target, "")
}

fn receipt(
    target: &dyn SharedPlaceTextTarget,
    expected: &str,
) -> Result<PrivateTextPlacementReceipt, String> {
    let readback = target.read_text()?;
    if readback.as_bytes() != expected.as_bytes() {
        return Err("shared place-text read-back was not byte exact".to_owned());
    }
    let carrier_characters = target.carrier_characters()?;
    if carrier_characters != 0 {
        return Err("Discord carrier box was not empty before send".to_owned());
    }
    let private_bytes = readback.len();
    Ok(PrivateTextPlacementReceipt {
        private_bytes,
        readback,
        counter_text: format!("{private_bytes} bytes"),
        carrier_characters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct BoxPair { private: String, carrier: String, calls: usize }
    impl SharedPlaceTextTarget for BoxPair {
        fn place_text(&mut self, text: &str) -> Result<(), String> { self.calls += 1; self.private = text.to_owned(); Ok(()) }
        fn read_text(&self) -> Result<String, String> { Ok(self.private.clone()) }
        fn carrier_characters(&self) -> Result<usize, String> { Ok(self.carrier.chars().count()) }
    }

    #[test]
    fn one_shared_job_counts_utf8_bytes_reads_exactly_and_clears() {
        let marker = "Discord|private|café|🔒|0904|_37XX";
        assert_eq!(marker.len(), 37);
        let mut boxes = BoxPair::default();
        let placed = place_private_text_exact(&mut boxes, marker).unwrap();
        assert_eq!(placed.private_bytes, 37);
        assert_eq!(placed.counter_text, "37 bytes");
        assert_eq!(placed.readback.as_bytes(), marker.as_bytes());
        assert_eq!(placed.carrier_characters, 0);
        let cleared = clear_private_text_exact(&mut boxes).unwrap();
        assert_eq!(cleared.private_bytes, 0);
        assert_eq!(cleared.counter_text, "0 bytes");
        assert_eq!(cleared.carrier_characters, 0);
        assert_eq!(boxes.calls, 2, "place and clear use one shared job");
    }

    #[test]
    fn shared_job_fails_when_the_carrier_is_not_empty() {
        let mut boxes = BoxPair { carrier: "leak".to_owned(), ..BoxPair::default() };
        assert_eq!(place_private_text_exact(&mut boxes, "Discord|private|café|🔒|0904|_37XX").unwrap_err(), "Discord carrier box was not empty before send");
    }
}
