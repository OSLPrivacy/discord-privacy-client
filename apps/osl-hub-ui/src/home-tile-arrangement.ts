export type HomeTileArrangement = {
  order: string[];
  hidden: string[];
};

export type HomeTileArrangementRead = {
  order: string[];
  visible: string[];
  hidden: string[];
};

function uniqueKnownIds(ids: readonly string[]): string[] {
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const id of ids) {
    if (!id || seen.has(id)) continue;
    seen.add(id);
    unique.push(id);
  }
  return unique;
}

export function normalizeHomeTileArrangement(
  defaults: readonly string[],
  arrangement: HomeTileArrangement,
): HomeTileArrangement {
  const defaultOrder = uniqueKnownIds(defaults);
  const known = new Set(defaultOrder);
  const order: string[] = [];
  for (const id of arrangement.order) {
    if (!known.has(id) || order.includes(id)) continue;
    order.push(id);
  }
  for (const id of defaultOrder) {
    if (!order.includes(id)) order.push(id);
  }

  const hidden = arrangement.hidden.filter((id, index) =>
    known.has(id) && arrangement.hidden.indexOf(id) === index
  );
  return { order, hidden };
}

export function readHomeTileArrangement(
  defaults: readonly string[],
  arrangement: HomeTileArrangement,
): HomeTileArrangementRead {
  const normalized = normalizeHomeTileArrangement(defaults, arrangement);
  const hidden = new Set(normalized.hidden);
  return {
    order: normalized.order,
    visible: normalized.order.filter((id) => !hidden.has(id)),
    hidden: normalized.order.filter((id) => hidden.has(id)),
  };
}

export function moveHomeTileArrangement(
  defaults: readonly string[],
  arrangement: HomeTileArrangement,
  id: string,
  delta: number,
): HomeTileArrangement {
  if (!Number.isSafeInteger(delta) || Math.abs(delta) !== 1) {
    return normalizeHomeTileArrangement(defaults, arrangement);
  }
  const normalized = normalizeHomeTileArrangement(defaults, arrangement);
  const index = normalized.order.indexOf(id);
  const target = index + delta;
  if (index < 0 || target < 0 || target >= normalized.order.length) {
    return normalized;
  }
  const order = [...normalized.order];
  [order[index], order[target]] = [order[target], order[index]];
  return { ...normalized, order };
}

export function dragHomeTileArrangement(
  defaults: readonly string[],
  arrangement: HomeTileArrangement,
  sourceId: string | null,
  targetId: string | null,
): HomeTileArrangement {
  if (!sourceId || !targetId || sourceId === targetId) {
    return normalizeHomeTileArrangement(defaults, arrangement);
  }
  const normalized = normalizeHomeTileArrangement(defaults, arrangement);
  const source = normalized.order.indexOf(sourceId);
  const target = normalized.order.indexOf(targetId);
  if (source < 0 || target < 0) return normalized;

  const order = [...normalized.order];
  order.splice(source, 1);
  order.splice(target, 0, sourceId);
  return { ...normalized, order };
}

export function setHomeTileVisibility(
  defaults: readonly string[],
  arrangement: HomeTileArrangement,
  id: string,
  visible: boolean,
): HomeTileArrangement {
  const normalized = normalizeHomeTileArrangement(defaults, arrangement);
  if (!normalized.order.includes(id)) return normalized;
  const hidden = new Set(normalized.hidden);
  if (visible) hidden.delete(id);
  else hidden.add(id);
  return { ...normalized, hidden: normalized.order.filter((candidate) => hidden.has(candidate)) };
}

export function toggleHomeTileVisibility(
  defaults: readonly string[],
  arrangement: HomeTileArrangement,
  id: string,
): HomeTileArrangement {
  const normalized = normalizeHomeTileArrangement(defaults, arrangement);
  return setHomeTileVisibility(defaults, normalized, id, normalized.hidden.includes(id));
}
