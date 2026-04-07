/**
 * ESLint Flat Configuration for AG-Claw (ESLint 10+)
 *
 * Migrated from .eslintrc.js to flat config format.
 * Targets TypeScript with strict rules, JSDoc enforcement,
 * and plugin-based rules for import/order, promise correctness, etc.
 */
import tseslint from 'typescript-eslint';
import importPlugin from 'eslint-plugin-import-x';
import jestPlugin from 'eslint-plugin-jest';
import js from '@eslint/js';

export default tseslint.config(
  // ── Global ignores ──────────────────────────────────────────────────────
  {
    ignores: [
      'dist/',
      'node_modules/',
      '.git/',
      'coverage/',
      'data/',
      'src/features/browser-automation/',
      '**/*.js',
      '!eslint.config.mjs',
    ],
  },

  // ── Base configs ────────────────────────────────────────────────────────
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...tseslint.configs.strict,

  // ── Main config for all files ───────────────────────────────────────────
  {
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: 2022,
      sourceType: 'module',
      parserOptions: {
        project: './tsconfig.json',
        tsconfigRootDir: import.meta.dirname,
      },
    },

    plugins: {
      import: importPlugin,
      jest: jestPlugin,
    },

    settings: {
      'import/resolver': {
        typescript: {
          project: './tsconfig.json',
          alwaysTryTypes: true,
        },
      },
    },

    rules: {
      // ── TypeScript ──────────────────────────────────────────────────────
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-unnecessary-type-assertion': 'error',
      '@typescript-eslint/no-require-imports': 'error',
      '@typescript-eslint/no-use-before-define': 'off',
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/prefer-optional-chain': 'error',
      '@typescript-eslint/prefer-nullish-coalescing': 'error',
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/explicit-module-boundary-types': 'off',
      '@typescript-eslint/no-empty-object-type': 'off',
      '@typescript-eslint/no-dynamic-delete': 'warn',
      '@typescript-eslint/consistent-type-imports': [
        'error',
        { prefer: 'type-imports', fixStyle: 'inline-type-imports' },
      ],

      // ── Imports ─────────────────────────────────────────────────────────
      'import/order': [
        'error',
        {
          groups: [
            'builtin',
            'external',
            'internal',
            ['parent', 'sibling'],
            'index',
            'object',
            'type',
          ],
          pathGroups: [
            {
              pattern: '{@clack/prompts,chalk,consola,debug,pino}',
              group: 'external',
              position: 'before',
            },
            { pattern: '{./,../}*.js', group: 'sibling', position: 'after' },
            { pattern: '{./,../}*.ts', group: 'sibling', position: 'after' },
            { pattern: '{./types,./types/**}', group: 'internal', position: 'before' },
          ],
          alphabetize: { order: 'asc', caseInsensitive: false },
          'newlines-between': 'always',
          warnOnUnassignedImports: true,
        },
      ],
      'import/no-default-export': 'error',
      'import/no-cycle': ['error', { maxDepth: 3 }],
      'import/no-self-import': 'error',
      'import/no-useless-path-segments': 'error',
      'import/first': 'error',
      'import/newline-after-import': ['error', { count: 1 }],
      'import/no-extraneous-dependencies': [
        'error',
        { devDependencies: ['tests/**', 'scripts/**', '**/*.test.ts', '**/*.spec.ts'] },
      ],

      // ── General ─────────────────────────────────────────────────────────
      'no-console': ['warn', { allow: ['warn', 'error', 'info'] }],
      'no-process-exit': 'error',
      'no-throw-literal': 'error',
      'prefer-const': 'error',
      'prefer-destructuring': 'off',
      'prefer-object-spread': 'error',
      'prefer-template': 'error',
      'object-shorthand': 'error',
      'quote-props': 'off',
      'array-callback-return': 'error',
      'consistent-return': 'error',
      'default-case': 'error',
      'default-case-last': 'error',
      'dot-notation': ['error', { allowPattern: '^[_a-z][_a-zA-Z0-9]*$' }],
      eqeqeq: ['error', 'always'],
      'no-alert': 'error',
      'no-caller': 'error',
      'no-eval': 'error',
      'no-extend-native': 'error',
      'no-extra-bind': 'error',
      'no-extra-label': 'error',
      'no-global-assign': 'error',
      'no-labels': 'error',
      'no-lone-blocks': 'error',
      'no-loop-func': 'error',
      'no-multi-assign': 'error',
      'no-new': 'error',
      'no-new-func': 'error',
      'no-new-wrappers': 'error',
      'no-octal-escape': 'error',
      'no-param-reassign': 'error',
      'no-proto': 'error',
      'no-redeclare': 'error',
      'no-return-assign': ['error', 'always'],
      'no-script-url': 'error',
      'no-self-compare': 'error',
      'no-sequences': 'error',
      'no-shadow': 'off',
      'no-sync': 'off',
      'no-unused-expressions': ['error', { allowShortCircuit: true, allowTernary: true }],
      'no-useless-call': 'error',
      'no-useless-catch': 'error',
      'no-useless-concat': 'error',
      'no-useless-escape': 'error',
      'no-useless-return': 'error',
      'no-void': ['error', { allowAsStatement: true }],
      'no-with': 'error',
      'require-await': 'error',
      yoda: 'error',

      // ── Jest ──────────────────────────────────────────────────────────────
      'jest/no-disabled-tests': 'warn',
      'jest/no-focused-tests': 'error',
      'jest/no-identical-title': 'error',
      'jest/valid-expect': 'error',
      'jest/expect-expect': ['error', { assertFunctionNames: ['expect', 'assert.*', 'should.*'] }],
    },

    linterOptions: {
      reportUnusedDisableDirectives: true,
    },
  },

  // ── Override: JS files ──────────────────────────────────────────────────
  {
    files: ['**/*.js', '**/*.mjs', '**/*.cjs'],
    rules: {
      '@typescript-eslint/no-require-imports': 'off',
      '@typescript-eslint/no-var-requires': 'off',
      '@typescript-eslint/consistent-type-imports': 'off',
      'import/no-default-export': 'off',
    },
  },

  // ── Override: Test files ────────────────────────────────────────────────
  {
    files: ['tests/**/*.ts', 'tests/**/*.js', '**/*.test.ts', '**/*.spec.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/unbound-method': 'off',
      'import/no-extraneous-dependencies': 'off',
    },
  },

  // ── Override: Script files ──────────────────────────────────────────────
  {
    files: ['scripts/**/*.ts', 'scripts/**/*.js'],
    rules: {
      'no-console': 'off',
      'import/no-extraneous-dependencies': 'off',
    },
  },

  // ── Override: Bin files ─────────────────────────────────────────────────
  {
    files: ['bin/**/*.js'],
    rules: {
      'no-console': 'off',
      '@typescript-eslint/no-require-imports': 'off',
      'import/no-default-export': 'off',
    },
  },

  // ── Override: Source files (relaxed rules) ───────────────────────────────
  {
    files: ['src/**/*.ts'],
    rules: {
      'require-await': 'off',
      '@typescript-eslint/require-await': 'off',
      'import/no-default-export': 'off',
      '@typescript-eslint/no-require-imports': 'off',
      '@typescript-eslint/no-var-requires': 'off',
      '@typescript-eslint/prefer-nullish-coalescing': 'warn',
      '@typescript-eslint/no-floating-promises': 'warn',
      '@typescript-eslint/no-unused-vars': 'off',
      '@typescript-eslint/no-use-before-define': 'off',
      '@typescript-eslint/ban-ts-comment': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      'no-unused-vars': 'off',
      'consistent-return': 'off',
      '@typescript-eslint/consistent-return': 'off',
      'no-process-exit': 'off',
      '@typescript-eslint/no-process-exit': 'off',
      'no-empty': 'warn',
      'no-return-assign': 'off',
      '@typescript-eslint/no-return-assign': 'off',
      'no-useless-escape': 'off',
      '@typescript-eslint/no-useless-escape': 'off',
      'no-useless-return': 'off',
      '@typescript-eslint/no-useless-return': 'off',
      'default-case': 'off',
      'no-duplicate-case': 'off',
      'no-param-reassign': 'off',
      'no-self-assign': 'off',
      '@typescript-eslint/await-thenable': 'warn',
      '@typescript-eslint/no-invalid-void-type': 'off',
      '@typescript-eslint/no-this-alias': 'off',
      'no-new': 'warn',
      'import/order': 'warn',
      eqeqeq: 'off',
      '@typescript-eslint/no-misused-promises': 'warn',
      'preserve-caught-error': 'off',
      'no-useless-assignment': 'off',
      '@typescript-eslint/no-unused-expressions': 'off',
    },
  },
);
