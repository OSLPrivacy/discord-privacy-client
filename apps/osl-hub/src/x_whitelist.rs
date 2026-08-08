use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct XWhitelistKindDto {
    pub id: &'static str,
    pub name: &'static str,
}

pub const X_WHITELIST_KINDS: [XWhitelistKindDto; 4] = [
    XWhitelistKindDto {
        id: "direct_message",
        name: "direct message",
    },
    XWhitelistKindDto {
        id: "public_post",
        name: "public post",
    },
    XWhitelistKindDto {
        id: "group_direct_message",
        name: "group direct message",
    },
    XWhitelistKindDto {
        id: "reply",
        name: "reply",
    },
];

pub fn cmd_list_x_whitelist_kinds() -> Vec<XWhitelistKindDto> {
    X_WHITELIST_KINDS.to_vec()
}

pub fn resolve_x_whitelist_kind(id: &str) -> Result<XWhitelistKindDto, String> {
    X_WHITELIST_KINDS
        .into_iter()
        .find(|kind| kind.id == id)
        .ok_or_else(|| format!("unsupported X place kind: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_4200_x_finish_line_names_the_full_kind_list() {
        let kinds = cmd_list_x_whitelist_kinds();
        let names = kinds.iter().map(|kind| kind.name).collect::<Vec<_>>();
        println!(
            "TASK4200_X count={} names={}",
            names.len(),
            names.join(", ")
        );
        assert_eq!(
            names,
            vec![
                "direct message",
                "public post",
                "group direct message",
                "reply"
            ]
        );
        assert_eq!(kinds.len(), names.len());
    }

    #[test]
    fn task_3743_x_kind_list_has_four_resolving_kinds_and_refuses_invented() {
        let expected = [
            ("direct_message", "direct message"),
            ("public_post", "public post"),
            ("group_direct_message", "group direct message"),
            ("reply", "reply"),
        ];
        let kinds = cmd_list_x_whitelist_kinds();
        let ids = kinds
            .iter()
            .map(|kind| kind.id)
            .collect::<Vec<_>>()
            .join(",");
        let names = kinds
            .iter()
            .map(|kind| kind.name)
            .collect::<Vec<_>>()
            .join(",");
        let json = serde_json::to_string(&kinds).unwrap();

        println!("TASK3743_X_KIND_COUNT={}", kinds.len());
        println!("TASK3743_X_KIND_IDS={ids}");
        println!("TASK3743_X_KIND_NAMES={names}");
        println!("TASK3743_X_KIND_JSON={json}");

        assert_eq!(
            kinds
                .iter()
                .map(|kind| (kind.id, kind.name))
                .collect::<Vec<_>>(),
            expected
        );

        for (id, name) in expected {
            let resolved = resolve_x_whitelist_kind(id).expect("known X kind resolves");
            println!(
                "TASK3743_X_RESOLVED id={} name={} result=allowed",
                resolved.id, resolved.name
            );
            assert_eq!(resolved.id, id);
            assert_eq!(resolved.name, name);
        }

        let invented = run_task_3743_x_kind_probe("invented_kind");
        let invented_stderr = String::from_utf8_lossy(&invented.stderr);
        println!(
            "TASK3743_X_INVENTED_KIND_EXIT={}",
            invented.status.code().unwrap_or(-1)
        );
        println!(
            "TASK3743_X_INVENTED_KIND_REFUSAL={}",
            invented_stderr.trim()
        );
        assert_eq!(invented.status.code(), Some(1));
        assert!(invented_stderr.contains("invented_kind"));

        for (task_id, kind) in [("1125", "group_direct_message"), ("1127", "reply")] {
            let result = resolve_x_whitelist_kind(kind)
                .map(|_| "allowed")
                .unwrap_or("refusal");
            println!("TASK3743_TASK_{task_id}_KIND={kind} RESULT={result}");
            assert_eq!(result, "allowed");
        }
    }

    fn run_task_3743_x_kind_probe(kind: &str) -> std::process::Output {
        std::process::Command::new(std::env::current_exe().expect("current test binary"))
            .arg("task_3743_x_kind_probe_child")
            .arg("--ignored")
            .arg("--nocapture")
            .env("TASK3743_X_KIND_PROBE", kind)
            .output()
            .expect("run task 3743 child probe")
    }

    #[test]
    #[ignore]
    fn task_3743_x_kind_probe_child() {
        let kind = std::env::var("TASK3743_X_KIND_PROBE").expect("probe kind");
        match resolve_x_whitelist_kind(&kind) {
            Ok(resolved) => println!("TASK3743_X_KIND_PROBE_ALLOWED={}", resolved.id),
            Err(refusal) => {
                eprintln!("{refusal}");
                std::process::exit(1);
            }
        }
    }
}
