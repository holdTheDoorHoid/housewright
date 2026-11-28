/** Human-readable names for the fixed template categories. Must match
 * `CATEGORIES` in crates/core/tests/templates.rs (a vitest checks it). */
export const CATEGORY_LABELS: Record<string, string> = {
  safety: 'Safety',
  heating_cooling: 'Heating and cooling',
  plumbing: 'Plumbing and water',
  appliances: 'Appliances',
  electrical: 'Electrical and power',
  exterior: 'Outside the house',
  structure: 'Roof, attic and structure',
  yard: 'Yard, garden and pool',
};

export function categoryLabel(key: string | null | undefined): string {
  const k = key ?? 'general';
  return CATEGORY_LABELS[k] ?? k.replaceAll('_', ' ');
}
