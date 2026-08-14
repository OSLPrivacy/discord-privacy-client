//! The deliberately narrow timer promise for protected email.
//!
//! Mail providers cannot recall a delivered cover.  A timer therefore destroys
//! only OSL's protected object; it is never available for ordinary email.

pub const EMAIL_TIMER_DISCLOSURE_KEY: &str = "mail.timer.pointer_only_disclosure";
pub const EMAIL_TIMER_CONTROL: &str = "Stop this being readable after…";
pub const EMAIL_TIMER_DISCLOSURE: &str =
    "The private part stops working. The cover email stays in their inbox and cannot be recalled.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedEmailTimer {
    pub cover_id: String,
    pub due_at: i64,
    protected_object: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XTimerInventory {
    pub timer_controls: usize,
    pub records: usize,
    pub live_effects: usize,
    pub shipping_claims: usize,
    pub available: bool,
}

pub fn arm_protected_email_timer(
    cover_id: impl Into<String>,
    protected_object: impl Into<String>,
    due_at: i64,
) -> Result<ProtectedEmailTimer, &'static str> {
    let cover_id = cover_id.into();
    let protected_object = protected_object.into();
    if cover_id.trim().is_empty() || protected_object.trim().is_empty() {
        return Err("ordinary email is refused: protected email is required; records=0");
    }
    Ok(ProtectedEmailTimer {
        cover_id,
        due_at,
        protected_object: Some(protected_object),
    })
}

impl ProtectedEmailTimer {
    pub fn readable(&self, now: i64) -> bool {
        now < self.due_at && self.protected_object.is_some()
    }

    /// Destroy the OSL object at/after its due time.  Deliberately has no
    /// carrier-delete branch: the cover continues to exist at `cover_id`.
    pub fn destroy_due_object(&mut self, now: i64) -> bool {
        if now >= self.due_at && self.protected_object.take().is_some() {
            return true;
        }
        false
    }

    pub fn protected_object_exists(&self) -> bool {
        self.protected_object.is_some()
    }
}

/// X is a contract-only row in this release, not a timer surface.
pub const fn installed_x_timer_inventory() -> XTimerInventory {
    XTimerInventory {
        timer_controls: 0,
        records: 0,
        live_effects: 0,
        shipping_claims: 0,
        available: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_email_is_readable_before_due_then_loses_only_its_osl_object() {
        let mut protected = arm_protected_email_timer("cover-3301", "object-3301", 100)
            .expect("protected target arms");
        assert!(protected.readable(99));
        assert!(protected.destroy_due_object(100));
        assert!(!protected.readable(100));
        assert!(!protected.protected_object_exists());
        assert_eq!(protected.cover_id, "cover-3301");
    }

    #[test]
    fn ordinary_email_is_refused_without_a_record_and_x_is_not_installed() {
        let refusal = arm_protected_email_timer("ordinary-cover", "", 100)
            .expect_err("ordinary email does not get a timer");
        assert!(refusal.contains("records=0"));
        assert_eq!(installed_x_timer_inventory(), XTimerInventory {
            timer_controls: 0,
            records: 0,
            live_effects: 0,
            shipping_claims: 0,
            available: false,
        });
    }
}
