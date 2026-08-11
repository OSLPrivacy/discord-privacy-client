use keystore::{AccountRootKey, RecoveryKit, RecoveryServiceState};

fn main() {
    let root_a = AccountRootKey::generate();
    let root_b = AccountRootKey::generate();
    let kit = RecoveryKit::generate(root_a.public_key());
    let mut service = RecoveryServiceState::new(kit.public_authority(), kit.initial_signed_state())
        .expect("valid recovery genesis");
    let mut declaration = kit.declare_recovery(root_a.public_key(), 4, root_b.public_key());
    if std::env::var_os("OSL_TASK5168_STARVE_RECOVERY_SIGNATURE").is_some() {
        declaration = declaration.without_independent_signature();
    }

    match service.accept_recovery(declaration) {
        Ok(()) => {
            println!(
                "TASK5168b recovery_check=green epoch={}",
                service.recovery_epoch()
            );
        }
        Err(error) => {
            eprintln!("TASK5168b recovery_check=red error={error}");
            std::process::exit(1);
        }
    }
}
