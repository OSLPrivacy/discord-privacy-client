-- Freeze the payment policy and quote timestamp on each anonymous invoice so
-- later configuration changes cannot weaken or strand an outstanding quote.

ALTER TABLE crypto_invoices_v2
  ADD COLUMN confirmations_required INTEGER NOT NULL DEFAULT 2
  CHECK (confirmations_required > 0);

UPDATE crypto_invoices_v2
   SET confirmations_required = CASE payment_method WHEN 'xmr' THEN 10 ELSE 2 END;

ALTER TABLE crypto_invoices_v2
  ADD COLUMN price_locked_at INTEGER NOT NULL DEFAULT 0;

UPDATE crypto_invoices_v2
   SET price_locked_at = created_at
 WHERE price_locked_at = 0;
