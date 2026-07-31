const RAW_TEXT_ELEMENTS = new Set([
  'iframe',
  'noembed',
  'noframes',
  'noscript',
  'script',
  'style',
  'textarea',
  'title',
  'xmp',
]);
const HEAD_ELEMENTS = new Set([
  'base',
  'head',
  'link',
  'meta',
  'noscript',
  'script',
  'style',
  'template',
  'title',
]);

function decodeNumericCharacterReferences(value) {
  return value.replace(/&#(?:x([0-9a-f]+)|([0-9]+));?/gi, (match, hex, decimal) => {
    const point = Number.parseInt(hex ?? decimal, hex ? 16 : 10);
    if (!Number.isInteger(point) || point === 0 || point > 0x10ffff) return '\ufffd';
    try {
      return String.fromCodePoint(point);
    } catch {
      return '\ufffd';
    }
  });
}

function tagEnd(html, start) {
  let quote = '';
  for (let index = start; index < html.length; index += 1) {
    const char = html[index];
    if (quote) {
      if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      continue;
    }
    if (char === '>') return index + 1;
  }
  throw new Error('unterminated HTML tag');
}

function attributes(raw, offset) {
  const parsed = [];
  let index = offset;
  while (index < raw.length) {
    while (/\s/.test(raw[index] || '')) index += 1;
    if (index >= raw.length || raw[index] === '>' || raw[index] === '/') break;

    const nameStart = index;
    while (index < raw.length && !/[\s=/>]/.test(raw[index])) index += 1;
    const name = raw.slice(nameStart, index).toLowerCase();
    if (!name) break;
    while (/\s/.test(raw[index] || '')) index += 1;

    let value = null;
    if (raw[index] === '=') {
      index += 1;
      while (/\s/.test(raw[index] || '')) index += 1;
      if (raw[index] === '"' || raw[index] === "'") {
        const quote = raw[index];
        index += 1;
        const valueStart = index;
        while (index < raw.length && raw[index] !== quote) index += 1;
        if (index >= raw.length) throw new Error(`unterminated ${name} attribute`);
        value = raw.slice(valueStart, index);
        index += 1;
      } else {
        const valueStart = index;
        while (index < raw.length && !/[\s>]/.test(raw[index])) index += 1;
        value = raw.slice(valueStart, index);
      }
    }
    parsed.push({ name, value });
  }
  return parsed;
}

function firstAttribute(tag, name) {
  const value = tag.attributes.find((attribute) => attribute.name === name)?.value;
  return value === null || value === undefined
    ? null
    : decodeNumericCharacterReferences(value);
}

function attributeCount(tag, name) {
  return tag.attributes.filter((attribute) => attribute.name === name).length;
}

function tokenize(html) {
  const tags = [];
  let cursor = 0;
  let inHead = false;
  let templateDepth = 0;

  while (cursor < html.length) {
    const start = html.indexOf('<', cursor);
    if (start < 0) break;
    if (html.startsWith('<!--', start)) {
      const end = html.indexOf('-->', start + 4);
      if (end < 0) throw new Error('unterminated HTML comment');
      cursor = end + 3;
      continue;
    }
    if (html.startsWith('<!', start) || html.startsWith('<?', start)) {
      cursor = tagEnd(html, start + 2);
      continue;
    }

    const end = tagEnd(html, start + 1);
    const raw = html.slice(start, end);
    const match = raw.match(/^<\s*(\/?)\s*([A-Za-z][A-Za-z0-9:-]*)/);
    if (!match) {
      cursor = end;
      continue;
    }

    const closing = match[1] === '/';
    const name = match[2].toLowerCase();
    const tag = {
      start,
      end,
      name,
      closing,
      insideHead: inHead,
      insideTemplate: templateDepth > 0,
      attributes: closing ? [] : attributes(raw, match[0].length),
    };

    if (closing) {
      if (name === 'template' && templateDepth > 0) templateDepth -= 1;
      if (name === 'head') inHead = false;
      tags.push(tag);
      cursor = end;
      continue;
    }

    if (name === 'head') {
      inHead = true;
      tag.insideHead = true;
    }
    tags.push(tag);
    if (name === 'template') templateDepth += 1;

    if (RAW_TEXT_ELEMENTS.has(name)) {
      const lower = html.toLowerCase();
      const closeStart = lower.indexOf(`</${name}`, end);
      if (closeStart < 0) throw new Error(`unterminated ${name} element`);
      cursor = closeStart;
    } else {
      cursor = end;
    }
  }
  return tags;
}

function requireCanonicalDocument(tags, label) {
  const openings = (name) => tags.filter((tag) => !tag.closing && tag.name === name);
  const closings = (name) => tags.filter((tag) => tag.closing && tag.name === name);
  const htmlOpen = openings('html');
  const htmlClose = closings('html');
  const headOpen = openings('head');
  const headClose = closings('head');
  const bodyOpen = openings('body');
  const bodyClose = closings('body');
  if (
    htmlOpen.length !== 1
    || htmlClose.length !== 1
    || headOpen.length !== 1
    || headClose.length !== 1
    || bodyOpen.length !== 1
    || bodyClose.length !== 1
    || !(
      htmlOpen[0].start < headOpen[0].start
      && headOpen[0].end <= headClose[0].start
      && headClose[0].end <= bodyOpen[0].start
      && bodyOpen[0].end <= bodyClose[0].start
      && bodyClose[0].end <= htmlClose[0].start
    )
  ) {
    throw new Error(`${label} is not one canonical html/head/body document`);
  }

  const beforeHead = tags.filter((tag) => (
    !tag.closing
    && tag.start > htmlOpen[0].start
    && tag.start < headOpen[0].start
  ));
  const invalidHeadChildren = tags.filter((tag) => (
    !tag.closing
    && tag.start > headOpen[0].start
    && tag.start < headClose[0].start
    && !HEAD_ELEMENTS.has(tag.name)
  ));
  if (beforeHead.length !== 0 || invalidHeadChildren.length !== 0) {
    throw new Error(`${label} contains markup that the HTML parser would move outside head`);
  }
  if (tags.some((tag) => !tag.closing && tag.name === 'plaintext')) {
    throw new Error(`${label} contains a plaintext element that consumes the remaining document`);
  }
}

function buildMetaTags(html) {
  return tokenize(html).filter((tag) => (
    !tag.closing
    && !tag.insideTemplate
    && tag.name === 'meta'
    && firstAttribute(tag, 'name') === 'osl-build'
  ));
}

export function hasSemanticBuildMeta(html) {
  return buildMetaTags(html).length > 0;
}

export function requireExactBuildMeta(html, expectedCommit, label = 'HTML') {
  const tags = tokenize(html);
  requireCanonicalDocument(tags, label);
  const metas = tags.filter((tag) => (
    !tag.closing
    && !tag.insideTemplate
    && tag.name === 'meta'
    && firstAttribute(tag, 'name') === 'osl-build'
  ));
  const exact = metas.length === 1
    && metas[0].insideHead
    && metas[0].attributes.length === 2
    && attributeCount(metas[0], 'name') === 1
    && attributeCount(metas[0], 'content') === 1
    && firstAttribute(metas[0], 'content') === expectedCommit;
  if (!exact) {
    throw new Error(`${label} does not carry exactly one semantic full-SHA osl-build meta inside head`);
  }
  return metas[0];
}

export function insertExactBuildMeta(html, commit) {
  const tags = tokenize(html);
  requireCanonicalDocument(tags, 'source HTML');
  if (buildMetaTags(html).length !== 0) {
    throw new Error('source HTML already contains an osl-build meta');
  }
  const heads = tags.filter((tag) => !tag.closing && tag.name === 'head');
  const viewports = tags.filter((tag) => (
    !tag.closing
    && !tag.insideTemplate
    && tag.insideHead
    && tag.name === 'meta'
    && firstAttribute(tag, 'name') === 'viewport'
  ));
  if (heads.length !== 1 || viewports.length !== 1) {
    throw new Error('source HTML must contain exactly one head and one semantic viewport meta');
  }
  const viewport = viewports[0];
  return `${html.slice(0, viewport.end)}\n  <meta name="osl-build" content="${commit}">${html.slice(viewport.end)}`;
}
