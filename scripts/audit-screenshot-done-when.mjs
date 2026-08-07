#!/usr/bin/env node

// Audits active OSL build-plan done-when lines that name screenshots, images,
// pictures, PNGs, photos, or captures. Each matching line is checked for three
// independent requirements: page title, page controls, and blank-image
// rejection.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const DEFAULT_PLAN_DIR = '/home/liamw/osl-plan/OSL-AUDITS/todo';

const VISUAL_KEYWORD = /\b(?:screenshots?|images?|captures?|captured|png|pictures?|photos?)\b/i;
const PAGE_TITLE = /\b(?:page\s+title|exact\s+title|title\s+(?:"[^"]+"|'[^']+'|[A-Z][^,;.]+)|screen\s+title)\b/i;
const CONTROL_NOUN = /\b(?:controls?|buttons?|choices?|switch(?:es)?|fields?|links?|boxes?|inputs?|tabs?|menus?|headings?|message\s+list|reading\s+pane|compose\s+box|send|continue|back|download\s+heading|remove\s+buttons?)\b/i;
const COMPLETE_CONTROL_CLAIM = /\b(?:every|each|all|both|named|visible)\s+(?:named\s+)?(?:controls?|buttons?|choices?|switch(?:es)?|fields?|links?|boxes?|inputs?|tabs?|menus?|headings?)\b/i;
const BLANK_REJECTION = /\b(?:(?:blank|nearly\s+blank).{0,90}(?:reject\w*|refus\w*|fail\w*|exit\s+1|go(?:es)?\s+red|not\s+count\w*)|(?:reject\w*|refus\w*|fail\w*|exit\s+1|go(?:es)?\s+red|not\s+count\w*).{0,90}(?:blank|nearly\s+blank)|not\s+blank\s+or\s+nearly\s+blank|replacing.{0,60}blank.{0,90}fail\w*)\b/i;

function shorten(filePath) {
  const relative = path.relative(REPO_ROOT, filePath);
  return relative.startsWith('..') ? filePath : relative;
}

export function classifyDoneWhen(doneWhen) {
  const failures = [];
  if (!PAGE_TITLE.test(doneWhen)) failures.push('missing_page_title');
  if (!CONTROL_NOUN.test(doneWhen) && !COMPLETE_CONTROL_CLAIM.test(doneWhen)) {
    failures.push('missing_control_names');
  }
  if (!BLANK_REJECTION.test(doneWhen)) failures.push('missing_blank_rejection');
  return {
    status: failures.length === 0 ? 'PASS' : 'FLAGGED',
    failures,
  };
}

export function readTasksFromText(filePath, text) {
  const tasks = [];
  let current = null;
  const lines = text.split(/\r?\n/);

  lines.forEach((line, index) => {
    const task = line.match(/^TASK\s+([0-9]+[a-z]?)\s*(?:\[x\]\s*)?-\s*(.+)$/i);
    if (task) {
      current = {
        id: task[1],
        title: task[2].trim(),
        file: filePath,
        line: index + 1,
        doneWhen: null,
        doneWhenLine: null,
      };
      tasks.push(current);
      return;
    }

    const doneWhen = line.match(/^done when:\s*(.+)$/i);
    if (doneWhen && current && current.doneWhen === null) {
      current.doneWhen = doneWhen[1].trim();
      current.doneWhenLine = index + 1;
    }
  });

  return tasks;
}

export function auditTasks(tasks) {
  const checks = tasks
    .filter((task) => task.doneWhen && VISUAL_KEYWORD.test(task.doneWhen))
    .map((task) => ({
      ...task,
      ...classifyDoneWhen(task.doneWhen),
    }));

  return {
    tasksRead: tasks.length,
    checks,
    flaggedCount: checks.filter((check) => check.status === 'FLAGGED').length,
  };
}

export function readPlanTasks(planDir = DEFAULT_PLAN_DIR) {
  if (!existsSync(planDir)) {
    throw new Error(`plan todo directory not found: ${planDir}`);
  }

  const files = readdirSync(planDir)
    .filter((name) => /^[0-9][0-9]-.*\.txt$/.test(name))
    .sort()
    .map((name) => path.join(planDir, name));

  if (files.length === 0) {
    throw new Error(`plan todo directory contains no active task files: ${planDir}`);
  }

  return files.flatMap((file) => readTasksFromText(file, readFileSync(file, 'utf8')));
}

function renderReport(audit) {
  const lines = [
    `tasks_read: ${audit.tasksRead}`,
    `screenshot_checks: ${audit.checks.length}`,
    `flagged_count: ${audit.flaggedCount}`,
    '',
  ];

  for (const check of audit.checks) {
    const failures = check.failures.length ? check.failures.join(',') : 'none';
    lines.push(`- TASK ${check.id} ${check.status} failures=${failures}`);
    lines.push(`  source: ${shorten(check.file)}:${check.doneWhenLine}`);
    lines.push(`  done when: ${check.doneWhen}`);
  }

  return `${lines.join('\n')}\n`;
}

function parseArgs(argv) {
  const args = { planDir: DEFAULT_PLAN_DIR, stdinLines: false, format: 'report' };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--plan-dir') {
      args.planDir = argv[index + 1];
      index += 1;
    } else if (arg === '--stdin-lines') {
      args.stdinLines = true;
    } else if (arg === '--json') {
      args.format = 'json';
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

async function readStdin() {
  let text = '';
  for await (const chunk of process.stdin) text += chunk;
  return text;
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  let audit;

  if (args.stdinLines) {
    const text = await readStdin();
    const tasks = text.split(/\r?\n/)
      .filter((line) => line.trim())
      .map((line, index) => ({
        id: `fixture-${index + 1}`,
        title: `fixture ${index + 1}`,
        file: '<stdin>',
        line: index + 1,
        doneWhen: line.trim().replace(/^done when:\s*/i, ''),
        doneWhenLine: index + 1,
      }));
    audit = auditTasks(tasks);
  } else {
    audit = auditTasks(readPlanTasks(args.planDir));
  }

  const output = args.format === 'json' ? `${JSON.stringify(audit, null, 2)}\n` : renderReport(audit);
  process.stdout.write(output);
  return audit;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`audit-screenshot-done-when: ${error.message}`);
    process.exit(1);
  });
}
