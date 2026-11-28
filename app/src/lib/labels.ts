/** Human-readable name for a template category key such as "heating_cooling". */
const CATEGORY_LABELS: Record<string, string> = {
  heating_cooling: 'heating and cooling',
};

export function categoryLabel(key: string | null | undefined): string {
  const k = key ?? 'general';
  return CATEGORY_LABELS[k] ?? k.replaceAll('_', ' ');
}
