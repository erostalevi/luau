// ESLint flat config: TypeScript + Svelte 5. Formatting is Prettier's job.
import js from '@eslint/js';
import ts from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import svelteParser from 'svelte-eslint-parser';
import globals from 'globals';

export default ts.config(
  { ignores: ['dist/', 'target/', 'src-tauri/', 'crates/', 'node_modules/', '.claude/', 'gen/', 'docs/'] },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
    rules: {
      // Mocks and RPC payloads are deliberately loose.
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrors: 'none' }],
      'no-empty': ['error', { allowEmptyCatch: true }],
      '@typescript-eslint/no-unused-expressions': ['error', { allowShortCircuit: true, allowTernary: true }],
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parser: svelteParser, parserOptions: { parser: ts.parser, extraFileExtensions: ['.svelte'] } },
    rules: {
      // Keyed each blocks are used where identity matters; navigation is
      // handled by the app (links open in the system browser).
      'svelte/require-each-key': 'off',
      'svelte/no-navigation-without-resolve': 'off',
      // Plain Map/Set are used on purpose for caches and temporaries; reactive
      // state is declared explicitly with $state / SvelteSet / SvelteMap.
      'svelte/prefer-svelte-reactivity': 'off',
    },
  },
);
