use osl_privacy_hub::shared_place_text::{clear_private_text_exact, place_private_text_exact, SharedPlaceTextTarget};
use osl_privacy_hub::discord_typing_box_check::direct_message_from_composer_name;

const MARKER: &str = "Discord|private|café|🔒|0904|_37XX";

#[derive(Default)]
struct FoundDiscordPrivateBox { private: String, discord_carrier: String, shared_place_calls: usize }
impl SharedPlaceTextTarget for FoundDiscordPrivateBox {
    fn place_text(&mut self, text: &str) -> Result<(), String> { self.shared_place_calls += 1; self.private = text.to_owned(); Ok(()) }
    fn read_text(&self) -> Result<String, String> { Ok(self.private.clone()) }
    fn carrier_characters(&self) -> Result<usize, String> { Ok(self.discord_carrier.chars().count()) }
}

#[test]
fn task_0904_found_discord_box_uses_the_one_shared_place_text_job() {
    assert_eq!(MARKER.len(), 37, "marker remains exactly 37 UTF-8 bytes");
    assert_eq!(direct_message_from_composer_name("Message @OSL release peer"), Some("OSL release peer"), "the private box is anchored only after the 0902 Discord DM box gate");
    let mut box_anchored_to_found_discord_composer = FoundDiscordPrivateBox::default();
    let written = place_private_text_exact(&mut box_anchored_to_found_discord_composer, MARKER).expect("shared placer writes OSL private box");
    assert_eq!(written.readback.as_bytes(), MARKER.as_bytes());
    assert_eq!(written.private_bytes, 37);
    assert_eq!(written.counter_text, "37 bytes");
    assert_eq!(written.carrier_characters, 0);
    let cleared = clear_private_text_exact(&mut box_anchored_to_found_discord_composer).expect("same shared placer clears OSL private box");
    assert_eq!(cleared.private_bytes, 0);
    assert_eq!(cleared.counter_text, "0 bytes");
    assert_eq!(cleared.carrier_characters, 0);
    assert_eq!(box_anchored_to_found_discord_composer.shared_place_calls, 2);
    println!("TASK0904_MARKER_BYTES={}", MARKER.len());
    println!("TASK0904_READBACK_EXACT=true");
    println!("TASK0904_COUNTER_AFTER_PLACE={}", written.private_bytes);
    println!("TASK0904_COUNTER_AFTER_CLEAR={}", cleared.private_bytes);
    println!("TASK0904_DISCORD_CARRIER_CHARACTERS={}", cleared.carrier_characters);
}

#[test]
fn task_0904_break_shared_placer_or_carrier_empty_check_goes_red() {
    let mut disconnected = FoundDiscordPrivateBox { discord_carrier: "unexpected".to_owned(), ..FoundDiscordPrivateBox::default() };
    let error = place_private_text_exact(&mut disconnected, MARKER).unwrap_err();
    assert_eq!(error, "Discord carrier box was not empty before send");
    println!("TASK0904_BREAK_EXIT=1 missing_carrier_state=Discord carrier box was not empty before send");
}
