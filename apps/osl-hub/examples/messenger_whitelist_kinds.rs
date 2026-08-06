use osl_privacy_hub::messenger_whitelist_kinds::messenger_whitelist_kind_names;

fn main() {
    for kind in messenger_whitelist_kind_names() {
        println!("{kind}");
    }
}
