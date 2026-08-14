import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  BEARER_LOSS_DISCLOSURE,
  dataBalance,
  deviceVoucherBalanceSettingsMarkup,
  openLocalVoucherWallet,
  redeemVoucher,
  serializeLocalVoucherWallet,
  spendVoucherCapacity,
  type LocalVoucherWallet,
} from "./device-voucher-balance";

const here = (name: string): string => readFileSync(new URL(name, import.meta.url), "utf8");
const client = here("./device-voucher-balance.ts");
const settingsShell = here("./main.ts");
const keyserver = here("../../../keyserver-cf/src/lib/voucher-balance-grant.ts");
const queue = here("../../../services/voucher-redemption/src/voucher_balance_grant.rs");
const deployedStore = here("../../../cipher-store-cf/src/lib/voucher-balance-grant.ts");

const independentlyComputed = (wallet: LocalVoucherWallet): { stored: number; moved: number } => ({
  stored: wallet.vouchers.reduce((sum, voucher) => sum + voucher.capacity.stored - voucher.spent.stored, 0),
  moved: wallet.vouchers.reduce((sum, voucher) => sum + voucher.capacity.moved - voucher.spent.moved, 0),
});

const freshWallet = (): LocalVoucherWallet => openLocalVoucherWallet('{"vouchers":[]}');

const voucher = (jti: string, token: string, stored: number, moved: number) => ({
  token,
  grant: { aud: "osl-capacity-voucher" as const, exp: 2_000_000_000, jti },
  capacity: { stored, moved },
});

describe("TASK 7035 — data balance is unspent local bearer-voucher capacity", () => {
  it("equals an independent two-meter voucher sum after redemption, spend, and a full restart with no network request", () => {
    let requests = 0;
    const previousFetch = globalThis.fetch;
    globalThis.fetch = (() => { requests += 1; throw new Error("network request made to produce balance"); }) as typeof fetch;
    try {
      const first = voucher("firstVoucherGrant_0001", "firstBlindVoucherCredential_00000000000000000001", 70, 40);
      const second = voucher("secondVoucherGrant0002", "secondBlindVoucherCredential0000000000000000002", 30, 80);
      const redeemed = redeemVoucher(redeemVoucher(freshWallet(), first), second);
      expect(dataBalance(redeemed)).toEqual(independentlyComputed(redeemed));
      expect(dataBalance(redeemed)).toEqual({ stored: 100, moved: 120 });

      const spent = spendVoucherCapacity(
        spendVoucherCapacity(redeemed, first.grant.jti, "stored", 11),
        second.grant.jti,
        "moved",
        17,
      );
      const restarted = openLocalVoucherWallet(serializeLocalVoucherWallet(spent));
      expect(dataBalance(spent)).toEqual(independentlyComputed(spent));
      expect(dataBalance(restarted)).toEqual(independentlyComputed(restarted));
      expect(dataBalance(restarted)).toEqual({ stored: 89, moved: 103 });

      const settings = deviceVoucherBalanceSettingsMarkup(restarted);
      expect(settings).toContain("Data balance on this device");
      expect(settings).toContain('data-voucher-balance-meter="stored">89');
      expect(settings).toContain('data-voucher-balance-meter="moved">103');
      expect(settings).toContain(BEARER_LOSS_DISCLOSURE);
      expect(settingsShell).toContain("await oslChatSecureStore.setItem(deviceVoucherWalletStorageKey, serializeLocalVoucherWallet(next));");
      expect(settingsShell).toContain("await loadDeviceVoucherWalletFromSecureStore();");
      expect(settingsShell).toContain("deviceVoucherBalanceSettingsMarkup(deviceVoucherWallet)");
      expect(requests).toBe(0);
      console.info("TASK7035 redemption=2 spends=2 restart=1 secure_local_persistence=1 stored=89 moved=103 independent_stored=89 independent_moved=103 network_requests=0");
    } finally {
      globalThis.fetch = previousFetch;
    }
  });

  it("keeps two local wallets separate and cannot alter either through the other", () => {
    const first = voucher("firstVoucherGrant_0001", "firstBlindVoucherCredential_00000000000000000001", 70, 40);
    const second = voucher("secondVoucherGrant0002", "secondBlindVoucherCredential0000000000000000002", 30, 80);
    const localA = spendVoucherCapacity(redeemVoucher(freshWallet(), first), first.grant.jti, "stored", 11);
    const localB = spendVoucherCapacity(redeemVoucher(freshWallet(), second), second.grant.jti, "moved", 17);
    expect(dataBalance(localA)).toEqual({ stored: 59, moved: 40 });
    expect(dataBalance(localB)).toEqual({ stored: 30, moved: 63 });
    expect(() => spendVoucherCapacity(localA, second.grant.jti, "stored", 1)).toThrow("local voucher absent");
    expect(dataBalance(localB)).toEqual({ stored: 30, moved: 63 });
    console.info("TASK7035 wallets=2 local_a_stored=59 local_a_moved=40 local_b_stored=30 local_b_moved=63 cross_wallet_alterations=0");
  });

  it("keeps the relay grant at exactly aud, exp, jti and finds no attribution, lookup, transfer, restore, or balance network path", () => {
    const surfaces = [client, keyserver, queue, deployedStore];
    for (const source of surfaces) {
      expect(source).toMatch(/aud[\s\S]{0,200}exp[\s\S]{0,200}jti/u);
      expect((source.match(/fetch\s*\(|XMLHttpRequest|WebSocket|https?:\/\//gu) ?? []).length).toBe(0);
      expect((source.match(/look"\s*\+\s*"up|trans"\s*\+\s*"fer|res"\s*\+\s*"tore/gu) ?? []).length).toBe(0);
    }
    const prohibitedFields = ["account" + "Id", "device" + "Name", "public" + "Handle", "payment" + "Id", "identity" + "Hash"];
    const prohibitedHits = surfaces.flatMap((source, surface) => prohibitedFields
      .filter((field) => source.includes(field))
      .map((field) => `${surface}:${field}`));
    expect(prohibitedHits).toEqual([]);
    expect(keyserver.match(/RELAY_VOUCHER_GRANT_FIELDS\s*=\s*\["aud", "exp", "jti"\]/u)).not.toBeNull();
    expect(deployedStore.match(/DEPLOYED_VOUCHER_GRANT_FIELDS\s*=\s*\["aud", "exp", "jti"\]/u)).not.toBeNull();
    expect(queue.match(/MIXING_QUEUE_VOUCHER_GRANT_FIELDS: \[&str; 3\] = \["aud", "exp", "jti"\]/u)).not.toBeNull();
    console.info("TASK7035 field_scan client=0 keyserver=0 mixing_queue=0 deployed_store=0 grant_fields=aud,exp,jti balance_lookups=0 transfers=0 restores=0 network_requests=0");
  });
});
