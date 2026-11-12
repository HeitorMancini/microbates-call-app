// Tons das "válvulas" de cada amigo: todos passam 4.5:1 com texto escuro.
const HUES = ['#ffb547', '#7fe0be', '#9fc2ff', '#ff9fb2', '#d6b8ff', '#f3e37c', '#8fe3f0'];

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  const first = [...parts[0]][0] ?? '';
  const last = parts.length > 1 ? ([...parts[parts.length - 1]][0] ?? '') : ([...parts[0]][1] ?? '');
  return (first + last).toLocaleUpperCase('pt-BR');
}

export function avatarColor(seed: string): string {
  let h = 0;
  for (const ch of seed) h = (h * 31 + (ch.codePointAt(0) ?? 0)) >>> 0;
  return HUES[h % HUES.length];
}
