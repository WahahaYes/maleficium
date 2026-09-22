// themeCatalog.ts — vendored theme index with bundled sources.
//
// One entry per file under src/assets/themes/; sources inline at build
// so themes resolve offline with no fetch.

import nord from '../assets/themes/nord.jsonc?raw';
import nightOwl from '../assets/themes/night-owl.jsonc?raw';
import owlLight from '../assets/themes/owl-light.jsonc?raw';
import oneDarkPro from '../assets/themes/one-dark-pro.jsonc?raw';
import onedark from '../assets/themes/onedark.jsonc?raw';
import cobalt2 from '../assets/themes/cobalt2.jsonc?raw';
import solarizedDark from '../assets/themes/solarized-dark.jsonc?raw';
import solarizedLight from '../assets/themes/solarized-light.jsonc?raw';
import synthwave84 from '../assets/themes/synthwave84.jsonc?raw';
import dracula from '../assets/themes/dracula.jsonc?raw';
import noctis from '../assets/themes/noctis.jsonc?raw';
import noctisLux from '../assets/themes/noctis-lux.jsonc?raw';
import winterDark from '../assets/themes/winter-dark.jsonc?raw';
import winterLight from '../assets/themes/winter-light.jsonc?raw';
import githubDark from '../assets/themes/github-dark.jsonc?raw';
import githubLight from '../assets/themes/github-light.jsonc?raw';
import catppuccinMocha from '../assets/themes/catppuccin-mocha.jsonc?raw';
import catppuccinLatte from '../assets/themes/catppuccin-latte.jsonc?raw';

export interface ThemeEntry {
  /** Value stored in AppearancePrefs.accent. */
  id: string;
  name: string;
  type: 'dark' | 'light';
  /** Picker grouping; pairs share a family. */
  family: string;
  source: string;
}

export const THEME_CATALOG: ThemeEntry[] = [
  { id: 'nord', name: 'Nord', type: 'dark', family: 'Nord', source: nord },
  { id: 'night-owl', name: 'Night Owl', type: 'dark', family: 'Night Owl', source: nightOwl },
  {
    id: 'owl-light',
    name: 'Night Owl Light',
    type: 'light',
    family: 'Night Owl',
    source: owlLight,
  },
  {
    id: 'one-dark-pro',
    name: 'One Dark Pro',
    type: 'dark',
    family: 'One Dark Pro',
    source: oneDarkPro,
  },
  { id: 'onedark', name: 'OneDark', type: 'dark', family: 'OneDark', source: onedark },
  { id: 'cobalt2', name: 'Cobalt2', type: 'dark', family: 'Cobalt2', source: cobalt2 },
  {
    id: 'solarized-dark',
    name: 'Solarized Dark',
    type: 'dark',
    family: 'Solarized',
    source: solarizedDark,
  },
  {
    id: 'solarized-light',
    name: 'Solarized Light',
    type: 'light',
    family: 'Solarized',
    source: solarizedLight,
  },
  {
    id: 'synthwave84',
    name: 'SynthWave 84',
    type: 'dark',
    family: 'SynthWave 84',
    source: synthwave84,
  },
  { id: 'dracula', name: 'Dracula', type: 'dark', family: 'Dracula', source: dracula },
  { id: 'noctis', name: 'Noctis', type: 'dark', family: 'Noctis', source: noctis },
  { id: 'noctis-lux', name: 'Noctis Lux', type: 'light', family: 'Noctis', source: noctisLux },
  {
    id: 'winter-dark',
    name: 'Winter Dark Blue',
    type: 'dark',
    family: 'Winter is Coming',
    source: winterDark,
  },
  {
    id: 'winter-light',
    name: 'Winter Light',
    type: 'light',
    family: 'Winter is Coming',
    source: winterLight,
  },
  { id: 'github-dark', name: 'GitHub Dark', type: 'dark', family: 'GitHub', source: githubDark },
  {
    id: 'github-light',
    name: 'GitHub Light',
    type: 'light',
    family: 'GitHub',
    source: githubLight,
  },
  {
    id: 'catppuccin-mocha',
    name: 'Catppuccin Mocha',
    type: 'dark',
    family: 'Catppuccin',
    source: catppuccinMocha,
  },
  {
    id: 'catppuccin-latte',
    name: 'Catppuccin Latte',
    type: 'light',
    family: 'Catppuccin',
    source: catppuccinLatte,
  },
];
