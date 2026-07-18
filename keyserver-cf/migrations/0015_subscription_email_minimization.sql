-- Stripe remains the system of record for receipts and customer contact.
-- OSL entitlement checks need opaque Stripe IDs and status only, so erase the
-- redundant D1 copy and keep the NOT NULL legacy column as an empty sentinel.
UPDATE subscriptions SET customer_email = '' WHERE customer_email <> '';
