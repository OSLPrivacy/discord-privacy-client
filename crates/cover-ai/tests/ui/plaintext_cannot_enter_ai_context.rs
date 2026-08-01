use cover_ai::{context::AiContext, context::PlaintextMessage};

fn main() {
    let plaintext = PlaintextMessage::new("do not send this to the model");
    let _: AiContext = plaintext.into();
}
