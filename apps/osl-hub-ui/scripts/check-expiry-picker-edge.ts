import { expiryPickerMarkup, parseExpirySeconds, type ExpiryBounds } from "../src/expiry-picker";

const DAY_SECONDS = 24 * 60 * 60;
const bounds: ExpiryBounds = {
  minSeconds: 0,
  maxSeconds: 30 * DAY_SECONDS,
};

const rawDays = process.argv[2] ?? "";

if (!/^\d{2}$/u.test(rawDays)) {
  console.error(`picker ${rawDays}: rejected; expected exactly two day digits`);
  process.exit(1);
}

const days = Number(rawDays);
const seconds = days * DAY_SECONDS;
const parsed = parseExpirySeconds(String(seconds), bounds);
const markup = expiryPickerMarkup({ bounds, valueSeconds: seconds, nowMs: Date.UTC(2026, 0, 1, 0, 0, 0) });

if (parsed !== seconds || markup === "") {
  console.error(`picker ${rawDays} days: rejected seconds=${seconds} parsed=${parsed === null ? "null" : parsed} markup=${markup === "" ? "empty" : "present"}`);
  process.exit(1);
}

console.log(`picker ${rawDays} days: ok seconds=${seconds} parsed=${parsed} markup=present`);
