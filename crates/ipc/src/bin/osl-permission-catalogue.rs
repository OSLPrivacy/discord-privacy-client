use std::env;
use std::fs;
use std::io::{self, Read};
use std::process;

use ipc::permission_catalogue::{check_permission_catalogue_text, render_permission_catalogue};

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("print") => {
            print!("{}", render_permission_catalogue());
        }
        Some("check") => {
            let text = match args.next() {
                Some(path) => fs::read_to_string(path).unwrap_or_else(|error| {
                    eprintln!("permission catalogue check failed: {error}");
                    process::exit(1);
                }),
                None => {
                    let mut input = String::new();
                    io::stdin()
                        .read_to_string(&mut input)
                        .unwrap_or_else(|error| {
                            eprintln!("permission catalogue check failed: {error}");
                            process::exit(1);
                        });
                    input
                }
            };

            match check_permission_catalogue_text(&text) {
                Ok(report) => {
                    println!("TASK4851 section_names={}", report.section_names);
                    println!("TASK4851 permission_rows={}", report.permission_rows);
                    println!("TASK4851 enforcement_tags={}", report.enforcement_tags);
                    println!("TASK4851 allowed_tags=KEY,RELAY,TRUST");
                    println!("TASK4851 required_rows={}", report.required_rows.join("|"));
                }
                Err(error) => {
                    eprintln!("permission catalogue check failed: {error}");
                    process::exit(1);
                }
            }
        }
        Some(other) => {
            eprintln!("usage: osl-permission-catalogue [print|check [path]], got {other}");
            process::exit(2);
        }
    }
}
