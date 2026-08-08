//! TASK 4263 - honest refusal for the three half restored services.
//!
//! X, Instagram and Messenger were cut and are being put back. Gates 4255-4257
//! and 4261 put them back in the CATALOGUE and on the picker; that restore is
//! deliberately catalogue-only. The parts that would make them actually work
//! are still open tasks:
//!
//! * the web reader insides (4259 wrote the SHAPE and left every reader job
//!   answering "not built yet"), so nothing can be received on any of the three;
//! * the signed page control table (4260, itself waiting on the key decision in
//!   4260a), so nothing can be typed into or sent from Instagram or Messenger;
//! * X's watched send permission (4264), which is held back on purpose: X may
//!   place text and may not send it.
//!
//! A half restored service must never look like a working one. Every attempt to
//! send or receive on one of the three goes through this module and comes back
//! as a refusal that names the app and names the missing part. There is no path
//! through here that succeeds, and none that quietly returns nothing: the guards
//! return `Result<(), SurfaceRefusal>` where the error carries both names, so a
//! caller cannot drop the reason on the floor and still compile.
//!
//! When a service's own tasks pass, its row comes out of `HALF_RESTORED_APPS`
//! and the guards start answering `Ok` for it. Removing a row is the only way to
//! turn a service on, and that is the point: the switch is the proof, not a flag.

use std::fmt;

/// Which half of a conversation the caller was trying to use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceDirection {
    Send,
    Receive,
}

impl SurfaceDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }

    /// The verb used in the refusal a person reads.
    pub const fn attempt_words(self) -> &'static str {
        match self {
            Self::Send => "send a message on",
            Self::Receive => "receive messages from",
        }
    }
}

impl fmt::Display for SurfaceDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One named piece of work that has not landed yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingPart {
    /// Stable id for machine checks.
    pub id: &'static str,
    /// What a person is told is missing.
    pub name: &'static str,
    /// The plan task that will supply it.
    pub gating_task: &'static str,
}

/// A service that is back in the catalogue but not built through.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HalfRestoredApp {
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub missing_to_send: MissingPart,
    pub missing_to_receive: MissingPart,
}

impl HalfRestoredApp {
    pub const fn missing_part(&self, direction: SurfaceDirection) -> MissingPart {
        match direction {
            SurfaceDirection::Send => self.missing_to_send,
            SurfaceDirection::Receive => self.missing_to_receive,
        }
    }
}

const WEB_READER_INSIDES: MissingPart = MissingPart {
    id: "web_reader_insides",
    name: "the web reader insides",
    gating_task: "TASK 4259",
};

const SIGNED_PAGE_CONTROL_TABLE: MissingPart = MissingPart {
    id: "signed_page_control_table",
    name: "the signed page control table",
    gating_task: "TASK 4260",
};

const WATCHED_SEND_PERMISSION: MissingPart = MissingPart {
    id: "watched_send_permission",
    name: "the watched send permission",
    gating_task: "TASK 4264",
};

/// The three. A row leaves this table only when that service's own tasks pass.
pub const HALF_RESTORED_APPS: [HalfRestoredApp; 3] = [
    HalfRestoredApp {
        app_id: "x",
        display_name: "X",
        // 4264 holds X to placing text. Sending is not X's to do yet.
        missing_to_send: WATCHED_SEND_PERMISSION,
        missing_to_receive: WEB_READER_INSIDES,
    },
    HalfRestoredApp {
        app_id: "instagram",
        display_name: "Instagram",
        missing_to_send: SIGNED_PAGE_CONTROL_TABLE,
        missing_to_receive: WEB_READER_INSIDES,
    },
    HalfRestoredApp {
        app_id: "messenger",
        display_name: "Messenger",
        missing_to_send: SIGNED_PAGE_CONTROL_TABLE,
        missing_to_receive: WEB_READER_INSIDES,
    },
];

/// A refusal that names the app and names the missing part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceRefusal {
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub direction: SurfaceDirection,
    pub missing_part: MissingPart,
}

impl SurfaceRefusal {
    /// The exact words. Both names are in here on purpose: a refusal that says
    /// only "not available" is the same thing as silence to the person reading
    /// it, and this task exists because silence is what a half restored service
    /// used to give.
    pub fn message(&self) -> String {
        format!(
            "OSL refuses: cannot {verb} {app} yet because {part} is missing. \
             {gate} has not passed.",
            app = self.display_name,
            verb = self.direction.attempt_words(),
            part = self.missing_part.name,
            gate = self.missing_part.gating_task,
        )
    }

    /// One machine-readable line for the 4263 check.
    pub fn evidence_line(&self) -> String {
        format!(
            "app_id={app_id} app={app} direction={direction} missing_part={part_id} \
             missing_part_name={part_name} gate={gate}",
            app_id = self.app_id,
            app = self.display_name,
            direction = self.direction.as_str(),
            part_id = self.missing_part.id,
            part_name = self.missing_part.name,
            gate = self.missing_part.gating_task,
        )
    }
}

impl fmt::Display for SurfaceRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

/// Look up one of the three by the id used in an allowed-place record.
///
/// Matching is exact and lowercase, the same rule `service_kind_from_id` uses in
/// the hub: "X", "Instagram " and "instagram.com" are not this app, and must not
/// slip past the guard by looking like it.
pub fn half_restored_app(app_id: &str) -> Option<HalfRestoredApp> {
    HALF_RESTORED_APPS
        .iter()
        .copied()
        .find(|app| app.app_id == app_id)
}

/// Refuse if `app_id` is one of the three; otherwise allow.
pub fn require_surface_ready(
    app_id: &str,
    direction: SurfaceDirection,
) -> Result<(), SurfaceRefusal> {
    match half_restored_app(app_id) {
        None => Ok(()),
        Some(app) => Err(SurfaceRefusal {
            app_id: app.app_id,
            display_name: app.display_name,
            direction,
            missing_part: app.missing_part(direction),
        }),
    }
}

/// Refuse an attempt to send on one of the three.
pub fn require_send_ready(app_id: &str) -> Result<(), SurfaceRefusal> {
    require_surface_ready(app_id, SurfaceDirection::Send)
}

/// Refuse an attempt to receive on one of the three.
pub fn require_receive_ready(app_id: &str) -> Result<(), SurfaceRefusal> {
    require_surface_ready(app_id, SurfaceDirection::Receive)
}

/// The `Result<_, String>` shape the `cmd_osl_*` command surface uses.
pub fn guard_surface_direction(
    app_id: &str,
    direction: SurfaceDirection,
) -> Result<(), String> {
    require_surface_ready(app_id, direction).map_err(|refusal| refusal.message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_are_the_three() {
        let ids: Vec<&str> = HALF_RESTORED_APPS.iter().map(|app| app.app_id).collect();
        assert_eq!(ids, vec!["x", "instagram", "messenger"]);
    }

    #[test]
    fn every_direction_on_every_one_of_the_three_refuses_by_name() {
        let mut refused = 0usize;
        for app in HALF_RESTORED_APPS {
            for direction in [SurfaceDirection::Send, SurfaceDirection::Receive] {
                let refusal = require_surface_ready(app.app_id, direction)
                    .expect_err("a half restored surface must never report ready");
                let words = refusal.message();
                assert!(words.contains(app.display_name), "{words}");
                assert!(words.contains(refusal.missing_part.name), "{words}");
                assert!(!refusal.missing_part.name.is_empty());
                refused += 1;
            }
        }
        assert_eq!(refused, 6);
    }

    #[test]
    fn a_service_that_is_not_one_of_the_three_is_not_blocked() {
        for allowed in ["discord", "telegram", "signal", "whatsapp", "email"] {
            assert!(require_send_ready(allowed).is_ok(), "{allowed}");
            assert!(require_receive_ready(allowed).is_ok(), "{allowed}");
        }
    }

    #[test]
    fn near_miss_ids_do_not_match_one_of_the_three() {
        for near in ["X", "Instagram", "instagram ", "instagram.com", "messenger/"] {
            assert!(half_restored_app(near).is_none(), "{near}");
        }
    }
}
