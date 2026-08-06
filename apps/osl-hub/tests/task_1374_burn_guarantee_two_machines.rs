#![cfg(feature = "core")]

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::broker::{
    self, HubBrokerState, HubConversationContext, HubConversationKind,
};
use osl_privacy_hub::core_bridge::HubCoreState;

const PASSWORD: &str = "task-1374-burn-guarantee-password";
const DISABLE_BURN_REMOVAL_ON: &str = "TASK1374_DISABLE_BURN_REMOVAL_ON";

#[test]
fn task_1374_burn_guarantee_on_two_machines() {
    let _guard = global_state_lock().lock().expect("task fixture lock poisoned");
    let result = run();
    if result.is_err() {
        reset_process();
    }
    match result {
        Ok(()) => {}
        Err(error) => {
            panic!("{error}");
        }
    }
}

fn run() -> Result<(), String> {
    let mark = format!("TASK1374_MARK_{}", uuid::Uuid::new_v4());
    let mut first = Machine::new("machine-1", 0x41)?;
    let mut second = Machine::new("machine-2", 0x42)?;

    first.put_mark(&mark)?;
    second.put_mark(&mark)?;

    let first_read = first.read_exact_mark(&mark, 10)?;
    let second_read = second.read_exact_mark(&mark, 20)?;

    let (first_burn_deleted, second_burn_deleted) = burn_both_sides_once(&first, &second)?;

    let first_reopened = first.reopen_and_count_exact_mark(&mark, 50)?;
    let second_reopened = second.reopen_and_count_exact_mark(&mark, 60)?;

    println!("TASK1374 mark={mark}");
    println!(
        "TASK1374 machine=machine-1 first_exact_mark={} first_count={}",
        first_read.exact_mark, first_read.count
    );
    println!(
        "TASK1374 machine=machine-2 first_exact_mark={} first_count={}",
        second_read.exact_mark, second_read.count
    );
    println!(
        "TASK1374 burn_choice=Both Sides burn_invocations=1 machine-1_deleted_count={first_burn_deleted} machine-2_deleted_count={second_burn_deleted}"
    );
    println!(
        "TASK1374 machine=machine-1 reopened_count={} marked_content_absent={}",
        first_reopened.count, first_reopened.marked_content_absent
    );
    println!(
        "TASK1374 machine=machine-2 reopened_count={} marked_content_absent={}",
        second_reopened.count, second_reopened.marked_content_absent
    );

    let mut failure = None;
    if !first_read.exact_mark {
        failure = Some("machine-1 did not first show the exact mark".to_owned());
    } else if first_read.count != 1 {
        failure = Some(format!("machine-1 first count was {}", first_read.count));
    } else if !second_read.exact_mark {
        failure = Some("machine-2 did not first show the exact mark".to_owned());
    } else if second_read.count != 1 {
        failure = Some(format!("machine-2 first count was {}", second_read.count));
    } else if first_reopened.count != 0 || !first_reopened.marked_content_absent {
        println!("TASK1374_STILL_HOLDING_MARKED_CONTENT=machine-1");
        failure = Some("machine-1 still holding the marked content".to_owned());
    } else if second_reopened.count != 0 || !second_reopened.marked_content_absent {
        println!("TASK1374_STILL_HOLDING_MARKED_CONTENT=machine-2");
        failure = Some("machine-2 still holding the marked content".to_owned());
    } else if first_burn_deleted != 1 {
        failure = Some(format!(
            "machine-1 burn deleted {first_burn_deleted} marked rows"
        ));
    } else if second_burn_deleted != 1 {
        failure = Some(format!(
            "machine-2 burn deleted {second_burn_deleted} marked rows"
        ));
    }

    first.cleanup();
    second.cleanup();
    reset_process();

    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[derive(Clone)]
struct Machine {
    name: &'static str,
    root: PathBuf,
    identity: keystore::Identity,
    conversation_id: String,
    capsule: Option<String>,
}

struct ReadFacts {
    exact_mark: bool,
    count: usize,
}

struct ReopenFacts {
    count: usize,
    marked_content_absent: bool,
}

impl Machine {
    fn new(name: &'static str, entropy_byte: u8) -> Result<Self, String> {
        let root = isolated_root(name)?;
        std::fs::create_dir_all(&root).map_err(|error| format!("create {name} root: {error}"))?;
        configure_process(&root)?;
        ipc::main_password::set_main_password(&root, PASSWORD)?;
        let identity = keystore::identity_from_entropy(
            [entropy_byte; 16],
            format!("task-1374-{name}"),
        );
        keystore::save_identity(
            &root.join("identity.json"),
            &identity,
            &keystore::NoOpSealer::new(),
        )
        .map_err(|error| format!("save {name} identity: {error}"))?;
        Ok(Self {
            name,
            root,
            identity,
            conversation_id: format!("task-1374-{name}-conversation"),
            capsule: None,
        })
    }

    fn put_mark(&mut self, mark: &str) -> Result<(), String> {
        let (core, broker, token) = self.open_app(1)?;
        let prepared =
            broker::prepare_local_protected_text(&core, &broker, &token, mark.to_owned())?;
        if !prepared.state_persisted {
            return Err(format!("{} did not persist the marked message", self.name));
        }
        self.capsule = Some(prepared.capsule);
        Ok(())
    }

    fn read_exact_mark(&self, mark: &str, host_generation: u64) -> Result<ReadFacts, String> {
        let (core, broker, token) = self.open_app(host_generation)?;
        let opened = broker::decrypt_local_protected_capsule(
            &core,
            &broker,
            &token,
            self.capsule()?.to_owned(),
        )?;
        let exact_mark = opened.plaintext == mark;
        Ok(ReadFacts {
            exact_mark,
            count: usize::from(exact_mark),
        })
    }

    fn reopen_and_count_exact_mark(
        &self,
        mark: &str,
        host_generation: u64,
    ) -> Result<ReopenFacts, String> {
        let (core, broker, token) = self.open_app(host_generation)?;
        let opened = broker::decrypt_local_protected_capsule(
            &core,
            &broker,
            &token,
            self.capsule()?.to_owned(),
        );
        let count = match opened {
            Ok(opened) => usize::from(opened.plaintext == mark),
            Err(_) => 0,
        };
        Ok(ReopenFacts {
            count,
            marked_content_absent: count == 0,
        })
    }

    fn open_app(
        &self,
        host_generation: u64,
    ) -> Result<(HubCoreState, HubBrokerState, String), String> {
        configure_process(&self.root)?;
        ipc::main_password::verify_main_password(&self.root, PASSWORD)?;
        let reopened_identity = keystore::load_identity(
            &self.root.join("identity.json"),
            &keystore::NoOpSealer::new(),
        )
        .map_err(|error| format!("load {} identity: {error}", self.name))?;
        if reopened_identity.user_id != self.identity.user_id {
            return Err(format!("{} reopened the wrong identity", self.name));
        }
        let core = HubCoreState::default();
        *core.osl.identity.lock().expect("identity lock") = Some(reopened_identity);
        let broker = HubBrokerState::default();
        let lease = broker.activate(self.context(), host_generation)?;
        Ok((core, broker, lease.context_token))
    }

    fn context(&self) -> HubConversationContext {
        HubConversationContext {
            service_id: "email".to_owned(),
            account_id: format!("task-1374-{}", self.name),
            conversation_kind: HubConversationKind::Dm,
            conversation_id: self.conversation_id.clone(),
            space_id: None,
            participant_osl_ids: vec![self.identity.user_id.clone()],
            self_osl_id: self.identity.user_id.clone(),
        }
    }

    fn capsule(&self) -> Result<&str, String> {
        self.capsule
            .as_deref()
            .ok_or_else(|| format!("{} has no marked capsule", self.name))
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn burn_both_sides_once(first: &Machine, second: &Machine) -> Result<(usize, usize), String> {
    let first_deleted = burn_on_machine(first)?;
    let second_deleted = burn_on_machine(second)?;
    Ok((first_deleted, second_deleted))
}

fn burn_on_machine(machine: &Machine) -> Result<usize, String> {
    if std::env::var(DISABLE_BURN_REMOVAL_ON)
        .ok()
        .as_deref()
        == Some(machine.name)
    {
        println!("TASK1374_BURN_REMOVAL_DISABLED_ON={}", machine.name);
        return Ok(0);
    }
    let (core, broker, token) = machine.open_app(30)?;
    broker::burn_local_protected_context(&core, &broker, &token)
}

fn configure_process(root: &std::path::Path) -> Result<(), String> {
    keystore::set_active_account_dir(Some(root.to_owned()));
    keystore::set_base_dir_override(Some(root.to_owned()));
    ipc::main_password::set_file_storage_key(None);
    Ok(())
}

fn reset_process() {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(None);
    ipc::main_password::set_file_storage_key(None);
}

fn isolated_root(machine: &str) -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before epoch: {error}"))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "osl-task-1374-{machine}-{}-{nanos}",
        std::process::id()
    )))
}

fn global_state_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
