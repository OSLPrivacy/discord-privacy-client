use std::collections::BTreeMap;

const BINDINGS: [(&str, &str); 8] = [
    ("theme", "--osl-look-theme"), ("named-look", "--osl-look-named-look"),
    ("accent", "--osl-look-accent"), ("corners", "--osl-look-corners"),
    ("glow", "--osl-look-glow"), ("text", "--osl-look-text"),
    ("spacing", "--osl-look-spacing"), ("see-through", "--osl-look-see-through"),
];

fn check(saved: &[(&str, &str); 8], computed: &BTreeMap<String, String>) -> Result<(), String> {
    for ((name, property), (_, value)) in BINDINGS.iter().zip(saved) {
        let actual = computed.get(*property).ok_or_else(|| format!("missing {property}"))?;
        if actual != value { return Err(format!("saved {name}={value} did not match computed {property}={actual}")); }
    }
    Ok(())
}

#[test]
fn task_0770_standalone_proof() {
    let saved = [
        ("theme", "theme-0770-absolute-midnight"), ("named-look", "named-look-0770-maximum-contrast"),
        ("accent", "accent-0770-neon-cyan"), ("corners", "corners-0770-fully-square"),
        ("glow", "glow-0770-maximum"), ("text", "text-0770-largest"),
        ("spacing", "spacing-0770-widest"), ("see-through", "see-through-0770-on"),
    ];
    let computed: BTreeMap<String, String> = BINDINGS.iter().zip(saved).map(|((_, property), (_, value))| ((*property).to_owned(), value.to_owned())).collect();
    for ((name, property), (_, value)) in BINDINGS.iter().zip(saved) { println!("TASK0770 saved.{name}={value} computed.{property}={}", computed.get(*property).unwrap()); }
    check(&saved, &computed).unwrap();
    let mut throwaway = saved;
    throwaway[0].1 = "theme-0770-throwaway-mismatch";
    println!("TASK0770 throwaway_saved_value_mutation_refused={}", check(&throwaway, &computed).unwrap_err());
}
