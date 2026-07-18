import type { SubscriptionStatus } from "./subscriptions.js";
import { updateSubscriptionStatus } from "./subscriptions.js";

const precedence: Record<SubscriptionStatus, number> = {
  PENDING: 1,
  GRACE: 2,
  ACTIVE: 3,
  CANCELLED: 4,
  EXPIRED: 5,
  REVOKED: 6,
};

interface ObservationInput {
  subscriptionId: string;
  customerId?: string;
  status: SubscriptionStatus;
  currentPeriodEnd?: number | null;
  cancelAtPeriodEnd?: boolean;
  eventCreated: number;
  eventType: string;
}

export interface StripeSubscriptionObservationRow {
  status: SubscriptionStatus;
  current_period_end: number | null;
  cancel_at_period_end: number;
  event_type: string;
}

export async function getLatestSubscriptionObservation(
  db: D1Database,
  subscriptionId: string,
): Promise<StripeSubscriptionObservationRow | null> {
  return await db.prepare(
    `SELECT status, current_period_end, cancel_at_period_end, event_type
       FROM stripe_subscription_observations WHERE subscription_id = ?`,
  ).bind(subscriptionId).first<StripeSubscriptionObservationRow>();
}

export async function recordSubscriptionObservation(
  db: D1Database,
  input: ObservationInput,
): Promise<boolean> {
  const result = await db.prepare(
    `INSERT INTO stripe_subscription_observations (
       subscription_id, customer_id, status, status_precedence,
       current_period_end, cancel_at_period_end, event_created, event_type
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(subscription_id) DO UPDATE SET
       customer_id = COALESCE(excluded.customer_id, customer_id),
       status = excluded.status,
       status_precedence = excluded.status_precedence,
       current_period_end = COALESCE(excluded.current_period_end, current_period_end),
       cancel_at_period_end = excluded.cancel_at_period_end,
       event_created = excluded.event_created,
       event_type = excluded.event_type
     WHERE stripe_subscription_observations.status NOT IN ('REVOKED', 'EXPIRED')
       AND (
         excluded.event_created > stripe_subscription_observations.event_created
         OR (
           excluded.event_created = stripe_subscription_observations.event_created
           AND excluded.status_precedence >= stripe_subscription_observations.status_precedence
         )
       )`,
  ).bind(
    input.subscriptionId,
    input.customerId ?? null,
    input.status,
    precedence[input.status],
    input.currentPeriodEnd ?? null,
    input.cancelAtPeriodEnd ? 1 : 0,
    input.eventCreated,
    input.eventType,
  ).run();
  return (result.meta?.changes ?? 0) === 1;
}

export async function applyLatestSubscriptionObservation(
  db: D1Database,
  subscriptionId: string,
): Promise<StripeSubscriptionObservationRow | null> {
  const observed = await getLatestSubscriptionObservation(db, subscriptionId);
  if (!observed) return null;
  const patch: Parameters<typeof updateSubscriptionStatus>[2] = {
    status: observed.status,
    cancel_at_period_end: observed.cancel_at_period_end,
  };
  if (observed.current_period_end !== null) {
    patch.current_period_end = observed.current_period_end;
  }
  await updateSubscriptionStatus(db, subscriptionId, patch);
  return observed;
}

export async function recordAndApplySubscriptionObservation(
  db: D1Database,
  input: ObservationInput,
): Promise<boolean> {
  const accepted = await recordSubscriptionObservation(db, input);
  if (accepted) await applyLatestSubscriptionObservation(db, input.subscriptionId);
  return accepted;
}
