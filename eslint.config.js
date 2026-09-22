import js from '@eslint/js';
import globals from 'globals';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import tseslint from 'typescript-eslint';
import eslintConfigPrettier from 'eslint-config-prettier';
import { defineConfig, globalIgnores } from 'eslint/config';

export default defineConfig([
  globalIgnores(['dist', 'node_modules', 'src-tauri/target', 'src-tauri/gen']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
      eslintConfigPrettier,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      'react-hooks/refs': 'off',
      'react-hooks/immutability': 'off',
      'react-hooks/purity': 'off',
      'react-hooks/preserve-manual-memoization': 'off',
      'react-hooks/set-state-in-effect': 'off',
      // Theme tokens are the only styling source: no manual shadows and no
      // ripple re-enables outside the theme factory.
      'no-restricted-syntax': [
        'error',
        {
          selector: 'Property[key.name="boxShadow"]',
          message: 'Use theme elevation tokens instead of manual boxShadow.',
        },
        {
          selector:
            "JSXAttribute[name.name='disableRipple'] > JSXExpressionContainer > Literal[value=false]",
          message: 'Ripples stay disabled globally; do not re-enable per component.',
        },
        {
          selector: 'Property[key.name="disableRipple"][value.value=false]',
          message: 'Ripples stay disabled globally; do not re-enable per component.',
        },
      ],
    },
  },
]);
