#!/usr/bin/env node

import { readFileSync } from "node:fs";

const pricing = JSON.parse(readFileSync(new URL("../data/pricing.json", import.meta.url), "utf8"));
const text = pricing?.model?.approved_pricing_text;
const requiredFacts = [
  "5 dollars",
  "one month from code entry",
  "no renewal",
  "no OSL card storage",
];

if (typeof text !== "string" || text.length === 0) {
  throw new Error("data/pricing.json model.approved_pricing_text is missing");
}

for (const fact of requiredFacts) {
  if (!text.includes(fact)) {
    throw new Error(`approved pricing text is missing fact: ${fact}`);
  }
}

console.log("TASK1511 direct_content_command=shared_pricing_words");
console.log(`TASK1511 approved_pricing_text=${text}`);
for (const [index, fact] of requiredFacts.entries()) {
  console.log(`TASK1511 fact_${index + 1}=${fact}`);
}
