/** Minor units to text in the household's currency: 129999, "USD" -> "$1,299.99". */
export function money(minor: number, currency = 'USD'): string {
  try {
    return new Intl.NumberFormat(undefined, { style: 'currency', currency }).format(minor / 100);
  } catch {
    return `${(minor / 100).toFixed(2)} ${currency}`;
  }
}

export const CURRENCIES = ['USD', 'CAD', 'EUR', 'GBP', 'AUD', 'NZD'];

export const COST_KINDS = [
  ['parts', 'Parts'],
  ['labour', 'Labour'],
  ['contractor', 'Contractor'],
  ['other', 'Other'],
] as const;
