# Vendored VSCode color themes

Third-party color themes, each MIT-licensed (full texts in `LICENSES/`). Files are JSONC as shipped upstream (comments, trailing commas); the tier-1 reader must tolerate both. Files whose upstream lacks a `type` field resolve dark/light from background luminance.

| File | Source | Pin | License |
| --- | --- | --- | --- |
| nord.jsonc | nordtheme/visual-studio-code `develop` themes/nord-color-theme.json | c67473a | MIT © 2016-present Sven Greb |
| night-owl.jsonc | sdras/night-owl-vscode-theme `main` themes/Night Owl-color-theme.json | 729b426 | MIT © 2018 Sarah Drasner |
| owl-light.jsonc | sdras/night-owl-vscode-theme `main` themes/Night Owl-Light-color-theme.json | e2f9bf3 | MIT © 2018 Sarah Drasner |
| one-dark-pro.jsonc | binaryify/OneDark-Pro `master` themes/OneDark-Pro.json | cb215dd | MIT © 2013-2022 Binaryify |
| onedark.jsonc | akamud/vscode-theme-onedark `master` themes/OneDark.json | b8c574c | MIT © 2015 Mahmoud Ali |
| cobalt2.jsonc | wesbos/cobalt2-vscode `master` theme/cobalt2.json | e9f5093 | MIT © 2018 Wes Bos, Roberto Achar |
| solarized-dark.jsonc | braver/vscode-solarized `master` themes/braver-solarized-dark-with-workbench-color-theme.json | 8cdaf58 | MIT © 2024 Koen Lageveen |
| solarized-light.jsonc | braver/vscode-solarized `master` themes/braver-solarized-light-with-workbench-color-theme.json | 78c9177 | MIT © 2024 Koen Lageveen |
| synthwave84.jsonc | robb0wen/synthwave-vscode `master` themes/synthwave-color-theme.json | a52a908 | MIT © 2019 Robb Owen |
| dracula.jsonc | official vsix dracula-theme/theme-dracula 2.25.1 via open-vsx (repo dracula/visual-studio-code, MIT) | vsix-2.25.1 | MIT © 2016 Dracula Theme |
| noctis.jsonc | liviuschera/noctis `master` themes/noctis.json | 797f1a6 | MIT © 2018 Liviu Schera |
| noctis-lux.jsonc | liviuschera/noctis `master` themes/lux.json | aa44a8b | MIT © 2018 Liviu Schera |
| winter-dark.jsonc | johnpapa/vscode-winteriscoming `main` themes/WinterIsComing-dark-blue-color-theme.json | ff6b429 | MIT © 2015-2017 JohnPapa.net, LLC |
| winter-light.jsonc | johnpapa/vscode-winteriscoming `main` themes/WinterIsComing-light-color-theme.json | 637650e | MIT © 2015-2017 JohnPapa.net, LLC |
| github-dark.jsonc | official vsix GitHub/github-vscode-theme 6.3.5 via open-vsx (repo primer/github-vscode-theme, MIT) | vsix-6.3.5 | MIT © 2020 Primer |
| github-light.jsonc | official vsix GitHub/github-vscode-theme 6.3.5 via open-vsx (repo primer/github-vscode-theme, MIT) | vsix-6.3.5 | MIT © 2020 Primer |
| catppuccin-mocha.jsonc | official vsix Catppuccin/catppuccin-vsc 3.19.0 via open-vsx (shipped LICENSE is MIT) | vsix-3.19.0 | MIT © 2021 Catppuccin |
| catppuccin-latte.jsonc | official vsix Catppuccin/catppuccin-vsc 3.19.0 via open-vsx (shipped LICENSE is MIT) | vsix-3.19.0 | MIT © 2021 Catppuccin |

Only color data is vendored. SynthWave's glow hack (custom CSS injection) is excluded; Dracula's YAML source and the build-generated GitHub / Catppuccin themes are not vendored.
