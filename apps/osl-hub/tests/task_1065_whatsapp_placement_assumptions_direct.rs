#![cfg(task1065_direct)]

// Direct wrapper for the focused adapter test. The hub crate currently has
// unrelated crate-wide test compilation failures, so this path-includes the
// shipping accessibility substrate and WhatsApp adapter and runs their real
// test module without compiling unrelated hub modules.
mod native_apps {
    pub(crate) fn whatsapp_store_package_family_name() -> &'static str {
        "5319275A.WhatsAppDesktop_cv1g1gvanyjgm"
    }
}

#[path = "../src/native_a11y.rs"]
mod native_a11y;
#[path = "../src/native_whatsapp_adapter.rs"]
mod native_whatsapp_adapter;
