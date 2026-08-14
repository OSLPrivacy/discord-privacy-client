use std::{net::TcpListener, path::PathBuf};
use task_6214_durable_send::{provider_serve, Draft, Store, VolumeSnapshot};

fn main() {
    if let Err(error) = run() {
        eprintln!("TASK_6214_ERROR {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("provider") => {
            let root = PathBuf::from(&args[2]);
            let listener = TcpListener::bind(&args[3])?;
            println!("PROVIDER_ADDR={}", listener.local_addr()?);
            provider_serve(&root, listener)?;
        }
        Some("send") => {
            let store = Store::open(&args[2], [0x62; 32])?;
            let send_id = &args[3]; let provider = &args[4];
            let draft = Draft { account: args[5].clone(), recipient: args[6].clone(), body: args[7].as_bytes().to_vec() };
            store.accept(send_id, &format!("writer-{}", std::process::id()), &draft, VolumeSnapshot { free_bytes: u64::MAX / 4, total_bytes: 8 * 1024 * 1024 * 1024 })?;
            let record = store.ship(send_id, provider)?;
            println!("TASK_6214_SENT send_id={} authority_generation={} status={:?}", record.send_id, record.authority_generation, record.status);
        }
        Some("accept") => {
            let store = Store::open(&args[2], [0x62; 32])?;
            if let Ok(start) = std::env::var("OSL_6214_START") {
                while !PathBuf::from(&start).exists() { std::thread::sleep(std::time::Duration::from_millis(2)); }
            }
            let draft = Draft { account: args[5].clone(), recipient: args[6].clone(), body: args[7].as_bytes().to_vec() };
            let record = store.accept(&args[3], &args[4], &draft, VolumeSnapshot {
                free_bytes: args[8].parse()?, total_bytes: args[9].parse()?,
            })?;
            println!("TASK_6214_ACCEPTED send_id={} writer={} reservation={}", record.send_id, args[4], record.reserved_worst_case_bytes);
        }
        _ => return Err("usage: provider ROOT ADDR | send STORE SEND_ID PROVIDER ACCOUNT RECIPIENT BODY | accept STORE SEND_ID WRITER ACCOUNT RECIPIENT BODY FREE TOTAL".into()),
    }
    Ok(())
}
