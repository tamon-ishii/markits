export function groupHistory<T extends { id: string; operation_group?: string | null }>(items: T[]): T[][] {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const key = item.operation_group ?? item.id;
    const group = groups.get(key);
    if (group) group.push(item);
    else groups.set(key, [item]);
  }
  return [...groups.values()].map(group => group.sort((a, b) => a.id.localeCompare(b.id, undefined, { numeric: true })));
}
