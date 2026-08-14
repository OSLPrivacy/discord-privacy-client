//! Small runnable demonstration of the task-0071 shipping path.  The cover
//! contains only keyed observable choices; the pointer and ciphertext stay
//! inside `PairPointerProtocol`'s paired context.

use stego::{PairPointerProtocol, POINTER_BYTES, SHIPPING_CARRIED_BITS};

fn private_message_500_chars() -> String {
    let mut message = String::new();
    while message.len() < 500 {
        message.push_str(
            "private build note: pair alpha confirms the window, keeps the draft local, ",
        );
    }
    message.truncate(500);
    message
}

fn main() {
    let pair_key = [0x71; 32];
    let pointer = [0x07; POINTER_BYTES];
    let context = b"example=task-0071;authenticated-message=1";
    let private_message = private_message_500_chars();
    let sender = PairPointerProtocol::new(&pair_key, context);
    let (capture, record) = sender.seal(pointer, private_message.as_bytes());
    let recipient = PairPointerProtocol::new(&pair_key, context);
    let (recovered_pointer, recovered) =
        recipient.open(&capture, &record).expect("paired recovery");

    println!(
        "TASK0071 private_message_chars={}",
        private_message.chars().count()
    );
    println!("TASK0071 old_cover_bits=192");
    println!("TASK0071 new_cover_bits={SHIPPING_CARRIED_BITS}");
    println!(
        "TASK0071 observable_cover_words={}",
        capture.cover_text.split_ascii_whitespace().count()
    );
    println!(
        "TASK0071 recovered_matches={}",
        recovered == private_message.as_bytes() && recovered_pointer == pointer
    );
}
