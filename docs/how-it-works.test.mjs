import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const page = readFileSync(new URL('./how-it-works.html', import.meta.url), 'utf8');
const text = page
  .replace(/<script\b[\s\S]*?<\/script>/gi, ' ')
  .replace(/<style\b[\s\S]*?<\/style>/gi, ' ')
  .replace(/<[^>]+>/g, ' ')
  .replace(/\s+/g, ' ')
  .trim();

const requiredPoints = [
  'Protected messaging is for messages you choose to send through OSL; it is separate from account scanning.',
  'Account scanning is only a review of connected accounts you explicitly select.',
  'Files are handled as attachments to protected messages, not as a device-wide file sweep.',
  'Polite pace means OSL slows service actions instead of trying to race a provider.',
  'Risk stays visible: scanning or deletion may affect an account, so OSL asks before it acts.',
];

test('TASK 1534 page text separates how-it-works points', () => {
  for (const point of requiredPoints) {
    assert.ok(text.includes(point), `missing page text: ${point}`);
    console.log(`TASK1534 point: ${point}`);
  }
});
