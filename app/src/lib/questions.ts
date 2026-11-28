// The home questions (D-24, D-65). Highest-value first; every question can be
// skipped with "Not sure". Answers are a `ProfileAnswers` object, the JSON
// shape of `housewright_core::profile::Profile` without its id.

import type { ProfileAnswers, StoredProfile } from './api';

export type Part =
  | { kind: 'single'; key: keyof ProfileAnswers; label?: string; options: [string, string][] }
  | { kind: 'multi'; key: 'heating' | 'cooling'; label?: string; options: [string, string][] }
  | { kind: 'checklist'; options: [string, string][] }
  | { kind: 'zip' }
  | { kind: 'count'; key: 'adults' | 'children' | 'older_adults'; label: string }
  | { kind: 'pets' };

export interface Question {
  id: string;
  title: string;
  help?: string;
  parts: Part[];
}

export const QUESTIONS: Question[] = [
  {
    id: 'home_type',
    title: 'What kind of home is it?',
    parts: [
      {
        kind: 'single',
        key: 'home_type',
        options: [
          ['house', 'A house'],
          ['townhouse', 'A townhouse or row house'],
          ['condo', 'A condo'],
          ['apartment', 'An apartment'],
          ['mobile', 'A mobile or manufactured home'],
        ],
      },
    ],
  },
  {
    id: 'alarms',
    title: 'Smoke and carbon monoxide alarms',
    help: 'Roughly is fine. These come first because they protect the people, not the house.',
    parts: [
      {
        kind: 'single',
        key: 'smoke_alarms',
        label: 'How many smoke alarms?',
        options: [
          ['none', 'None'],
          ['one', 'One'],
          ['two_to_three', '2 or 3'],
          ['four_or_more', '4 or more'],
        ],
      },
      {
        kind: 'single',
        key: 'smoke_power',
        label: 'How are they powered?',
        options: [
          ['battery', 'Battery'],
          ['hardwired', 'Wired in, with a backup battery'],
          ['sealed', 'Sealed 10-year battery'],
        ],
      },
      {
        kind: 'single',
        key: 'co_alarms',
        label: 'How many carbon monoxide (CO) alarms?',
        options: [
          ['none', 'None'],
          ['one', 'One'],
          ['two_to_three', '2 or 3'],
          ['four_or_more', '4 or more'],
        ],
      },
    ],
  },
  {
    id: 'heating',
    title: 'How is it heated?',
    help: 'Pick all that apply.',
    parts: [
      {
        kind: 'multi',
        key: 'heating',
        options: [
          ['gas_furnace', 'Gas furnace (warm air from vents)'],
          ['oil', 'Oil'],
          ['heat_pump', 'Heat pump'],
          ['electric', 'Electric baseboard or panels'],
          ['boiler', 'Boiler (radiators or hot-water pipes)'],
          ['other', 'Something else, or no heating'],
        ],
      },
    ],
  },
  {
    id: 'water_heater',
    title: 'What heats the water?',
    parts: [
      {
        kind: 'single',
        key: 'water_heater',
        options: [
          ['tank_gas', 'A gas tank'],
          ['tank_electric', 'An electric tank'],
          ['tankless', 'Tankless (on demand)'],
          ['heat_pump', 'A heat pump water heater'],
        ],
      },
    ],
  },
  {
    id: 'zip',
    title: 'What is the ZIP code?',
    help: 'Used only on this device, to look up your average frost dates for seasonal jobs. Nothing is sent anywhere. US only for now.',
    parts: [{ kind: 'zip' }],
  },
  {
    id: 'water',
    title: 'Where does the water come from and go?',
    parts: [
      {
        kind: 'single',
        key: 'water_supply',
        label: 'Water',
        options: [
          ['city', 'City or town water'],
          ['well', 'A well'],
        ],
      },
      {
        kind: 'single',
        key: 'sewer',
        label: 'Waste water',
        options: [
          ['city', 'City sewer'],
          ['septic', 'A septic tank'],
        ],
      },
    ],
  },
  {
    id: 'features',
    title: 'Which of these does it have?',
    help: 'Tick what you know about. Leave the rest; we can ask again later.',
    parts: [
      {
        kind: 'checklist',
        options: [
          ['basement', 'Basement'],
          ['sump_pump', 'Sump pump'],
          ['crawlspace', 'Crawlspace'],
          ['attic', 'Attic you can get into'],
          ['fireplace', 'Fireplace'],
          ['wood_stove', 'Wood stove'],
          ['garage', 'Garage'],
          ['gutters', 'Gutters'],
          ['yard', 'Yard or lawn'],
          ['pool', 'Pool'],
          ['deck', 'Deck'],
          ['washer', 'Washing machine'],
          ['dryer', 'Clothes dryer'],
          ['dishwasher', 'Dishwasher'],
          ['fridge_water', 'Fridge with water or ice dispenser'],
          ['disposal', 'Garbage disposal'],
          ['range_hood', 'Range hood'],
          ['dehumidifier', 'Dehumidifier'],
          ['water_softener', 'Water softener'],
          ['generator', 'Generator'],
          ['solar', 'Solar panels'],
        ],
      },
    ],
  },
  {
    id: 'cooling',
    title: 'How is it cooled?',
    help: 'Pick all that apply.',
    parts: [
      {
        kind: 'multi',
        key: 'cooling',
        options: [
          ['central_ac', 'Central air conditioning'],
          ['heat_pump', 'Heat pump'],
          ['window_units', 'Window units'],
          ['none', 'No cooling'],
        ],
      },
    ],
  },
  {
    id: 'built',
    title: 'Roughly when was it built?',
    parts: [
      {
        kind: 'single',
        key: 'built',
        options: [
          ['before_1950', 'Before 1950'],
          ['1950_1979', '1950 to 1979'],
          ['1980_1999', '1980 to 1999'],
          ['2000_2019', '2000 to 2019'],
          ['2020_on', '2020 or later'],
        ],
      },
    ],
  },
  {
    id: 'people',
    title: 'Who lives here?',
    help: 'Only counts, no names. Used to keep safety first: with children or older adults at home, the starting plan has one more safety task.',
    parts: [
      { kind: 'count', key: 'adults', label: 'Adults' },
      { kind: 'count', key: 'children', label: 'Children' },
      { kind: 'count', key: 'older_adults', label: 'Older adults' },
      { kind: 'pets' },
    ],
  },
];

/** Whether a question has any answer in `a`. */
export function answered(q: Question, a: ProfileAnswers): boolean {
  return q.parts.some((p) => {
    switch (p.kind) {
      case 'single':
        return a[p.key] != null;
      case 'multi':
        return (a[p.key]?.length ?? 0) > 0;
      case 'checklist':
        return Object.values(a.features ?? {}).some(Boolean);
      case 'zip':
        return !!a.zip;
      case 'count':
        return a.people?.[p.key] != null;
      case 'pets':
        return a.people?.pets != null;
    }
  });
}

/** Clear one question's answers ("Not sure"). */
export function clear(q: Question, a: ProfileAnswers): ProfileAnswers {
  const out: ProfileAnswers = { ...a, people: { ...(a.people ?? {}) }, features: { ...(a.features ?? {}) } };
  for (const p of q.parts) {
    switch (p.kind) {
      case 'single':
      case 'multi':
        delete out[p.key];
        break;
      case 'checklist':
        out.features = {};
        break;
      case 'zip':
        delete out.zip;
        break;
      case 'count':
        delete out.people![p.key];
        break;
      case 'pets':
        delete out.people!.pets;
        break;
    }
  }
  return out;
}

/** Turn a stored profile back into answers for "Redo the home questions". */
export function fromProfile(p: StoredProfile | null): ProfileAnswers {
  if (!p) return {};
  const { id: _id, zip, ...rest } = p;
  const out: ProfileAnswers = { ...rest };
  if (zip) out.zip = zip.value;
  return out;
}

/** What is sent to `onboard`: unticked checklist items are left out (not sure). */
export function toSend(a: ProfileAnswers): ProfileAnswers {
  const features = Object.fromEntries(Object.entries(a.features ?? {}).filter(([, v]) => v));
  const people = Object.fromEntries(Object.entries(a.people ?? {}).filter(([, v]) => v != null));
  const out: ProfileAnswers = { ...a, features, people };
  for (const k of ['heating', 'cooling'] as const) if (out[k] && out[k]!.length === 0) delete out[k];
  if (!out.zip) delete out.zip;
  return out;
}
