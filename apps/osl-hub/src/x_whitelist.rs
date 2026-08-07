use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct XWhitelistKindDto {
    pub id: &'static str,
    pub name: &'static str,
}

pub const X_WHITELIST_KINDS: [XWhitelistKindDto; 4] = [
pub const X_WHITELIST_KINDS: [XWhitelistKindDto; 2] = [
    XWhitelistKindDto {
        id: "direct_message",
        name: "direct message",
    },
    XWhitelistKindDto {
        id: "public_post",
        name: "public post",
    },
    XWhitelistKindDto {
        id: "reply",
        name: "reply",
    },
    XWhitelistKindDto {
        id: "quote_post",
        name: "quote post",
    },
];

pub fn cmd_list_x_whitelist_kinds() -> Vec<XWhitelistKindDto> {
    X_WHITELIST_KINDS.to_vec()
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
            vec!["direct message", "public post", "reply", "quote post"]
        );
        assert_eq!(kinds.len(), names.len());
    }

    fn x_kinds_command_returns_exactly_two_named_kinds() {
        let kinds = cmd_list_x_whitelist_kinds();
        let names = kinds.iter().map(|kind| kind.name).collect::<Vec<_>>();
        let json = serde_json::to_string(&kinds).unwrap();

        println!(
            "command=cmd_list_x_whitelist_kinds count={} names={names:?} json={json}",
            kinds.len()
        );

        assert_eq!(kinds.len(), 2);
        assert_eq!(names, vec!["direct message", "public post"]);
        assert_eq!(
            json,
            r#"[{"id":"direct_message","name":"direct message"},{"id":"public_post","name":"public post"}]"#
        );
    }
}
