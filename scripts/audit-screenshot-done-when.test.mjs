import assert from 'node:assert/strict';
import test from 'node:test';

import {
  auditTasks,
  classifyDoneWhen,
  readTasksFromText,
} from './audit-screenshot-done-when.mjs';

test('classifyDoneWhen grades the three screenshot requirements independently', () => {
  assert.deepEqual(
    classifyDoneWhen('the screenshot shows page title "Discord", message box and Send; blank or nearly blank images are rejected'),
    { status: 'PASS', failures: [] },
  );

  assert.deepEqual(
    classifyDoneWhen('the screenshot shows the message box and Send; blank or nearly blank images are rejected'),
    { status: 'FLAGGED', failures: ['missing_page_title'] },
  );

  assert.deepEqual(
    classifyDoneWhen('the screenshot shows page title "Discord"; blank or nearly blank images are rejected'),
    { status: 'FLAGGED', failures: ['missing_control_names'] },
  );

  assert.deepEqual(
    classifyDoneWhen('the screenshot shows page title "Discord", message box and Send'),
    { status: 'FLAGGED', failures: ['missing_blank_rejection'] },
  );
});

test('auditTasks reads done-when screenshot lines and counts flagged checks', () => {
  const tasks = readTasksFromText('/tmp/tasks.txt', [
    'TASK 0001 - visual proof',
    'done when: the screenshot shows page title "Discord", message box and Send; blank or nearly blank images are rejected',
    'TASK 0002 - non visual proof',
    'done when: the direct command returns 1.',
    'TASK 0003 - weak visual proof',
    'done when: one image is saved.',
  ].join('\n'));

  const audit = auditTasks(tasks);

  assert.equal(audit.tasksRead, 3);
  assert.equal(audit.checks.length, 2);
  assert.equal(audit.flaggedCount, 1);
  assert.equal(audit.checks[0].status, 'PASS');
  assert.deepEqual(audit.checks[1].failures, [
    'missing_page_title',
    'missing_control_names',
    'missing_blank_rejection',
  ]);
});
