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

export interface ThemeEntry {
  /** Value stored in AppearancePrefs.accent. */
  id: string;
  name: string;
  type: 'dark' | 'light';
  source: string;
}

export const THEME_CATALOG: ThemeEntry[] = [
  { id: 'nord', name: 'Nord', type: 'dark', source: nord },
  { id: 'night-owl', name: 'Night Owl', type: 'dark', source: nightOwl },
  { id: 'owl-light', name: 'Owl Light', type: 'light', source: owlLight },
  { id: 'one-dark-pro', name: 'One Dark Pro', type: 'dark', source: oneDarkPro },
  { id: 'onedark', name: 'OneDark', type: 'dark', source: onedark },
  { id: 'cobalt2', name: 'Cobalt2', type: 'dark', source: cobalt2 },
  { id: 'solarized-dark', name: 'Solarized Dark', type: 'dark', source: solarizedDark },
  { id: 'solarized-light', name: 'Solarized Light', type: 'light', source: solarizedLight },
  { id: 'synthwave84', name: 'SynthWave 84', type: 'dark', source: synthwave84 },
  { id: 'dracula', name: 'Dracula', type: 'dark', source: dracula },
];
