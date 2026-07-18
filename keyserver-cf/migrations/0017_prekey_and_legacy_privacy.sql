-- Keep one identity's prekey pool bounded at the D1 write boundary. Clients
-- normally publish 100 OPKs at a time; 200 permits proactive replenishment
-- without allowing an abandoned or malicious identity to grow forever.
CREATE TRIGGER opk_pool_user_quota
BEFORE INSERT ON opk_pool
WHEN (
  SELECT COUNT(*) FROM opk_pool WHERE user_id = NEW.user_id
) >= 200
BEGIN
  SELECT RAISE(ABORT, 'OPK pool quota exceeded');
END;

-- The manual crypto-review flow was replaced by anonymous watcher invoices and
-- is no longer routed. Remove its historical email/address/transaction rows,
-- then make retirement irreversible even if stale code is accidentally revived.
DELETE FROM crypto_pending_payments;

CREATE TRIGGER crypto_pending_payments_retired
BEFORE INSERT ON crypto_pending_payments
BEGIN
  SELECT RAISE(ABORT, 'legacy crypto payment storage is retired');
END;
