import eslint from '@eslint/js';
import tseslint from '@typescript-eslint/eslint-plugin';
import tsparser from '@typescript-eslint/parser';
import importPlugin from 'eslint-plugin-import';
import jestPlugin from 'eslint-plugin-jest';
import globals from 'globals';

/**
 * AG-Claw ESLint Flat Config (ESLint v9)
 */
export default [
  // Ignore patterns
  {
    ignores: ['dist/', 'node_modules/', '.git/', 'coverage/', '*.js', '!.eslintrc.js'],
  },

  // JavaScript base
  {
    files: ['**/*.js', '**/*.mjs', '**/*.cjs'],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
    rules: {
      'no-restricted-syntax': 'off',
    },
  },

  // TypeScript files
  {
    files: ['**/*.ts'],
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
        project: './tsconfig.json',
        tsconfigRootDir: process.cwd(),
      },
      globals: {
        ...globals.node,
      },
    },
    plugins: {
      '@typescript-eslint': tseslint,
      'import': importPlugin,
      'jest': jestPlugin,
    },
    rules: {
      // TypeScript
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/no-non-null-assertion': 'off',
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
      '@typescript-eslint/consistent-type-imports': ['error', { prefer: 'type-imports', fixStyle: 'inline-type-imports' }],
      '@typescript-eslint/no-unnecessary-type-assertion': 'error',
      '@typescript-eslint/require-await': 'off',
      '@typescript-eslint/no-require-imports': 'off',
      '@typescript-eslint/no-var-requires': 'off',
      '@typescript-eslint/prefer-nullish-coalescing': 'warn',
      '@typescript-eslint/no-floating-promises': 'warn',
      '@typescript-eslint/no-use-before-define': 'off',
      '@typescript-eslint/ban-ts-comment': 'off',
      '@typescript-eslint/unbound-method': 'off',
      '@typescript-eslint/no-invalid-void-type': 'off',
      '@typescript-eslint/no-this-alias': 'off',
      '@typescript-eslint/no-misused-promises': 'warn',

      // Imports
      'import/order': ['warn', {
        'groups': ['builtin', 'external', 'internal', ['parent', 'sibling'], 'index', 'object', 'type'],
        'pathGroups': [
          { pattern: '{./,../}*.ts', group: 'sibling', position: 'after' },
          { pattern: '{./types,./types/**}', group: 'internal', position: 'before' },
        ],
        'alphabetize': { order: 'asc', caseInsensitive: false },
        'newlines-between': 'always',
        'warnOnUnassignedImports': true,
      }],
      'import/no-default-export': 'off',
      'import/no-cycle': ['error', { maxDepth: 3 }],
      'import/no-self-import': 'error',
      'import/no-useless-path-segments': 'error',
      'import/first': 'error',
      'import/newline-after-import': ['error', { count: 1 }],
      'import/no-extraneous-dependencies': ['error', { devDependencies: ['tests/**', 'scripts/**', '**/*.test.ts', '**/*.spec.ts'] }],
      'import/no-unused-modules': 'off',

      // Jest
      'jest/no-disabled-tests': 'warn',
      'jest/no-focused-tests': 'error',
      'jest/no-identical-title': 'error',
      'jest/valid-expect': 'error',
      'jest/expect-expect': ['error', { assertFunctionNames: ['expect', 'assert.*', 'should.*'] }],

      // General
      'no-console': ['warn', { allow: ['warn', 'error', 'info'] }],
      'no-process-exit': 'off',
      'no-throw-literal': 'error',
      'prefer-const': 'error',
      'prefer-destructuring': 'off',
      'prefer-object-spread': 'error',
      'prefer-template': 'error',
      'object-shorthand': 'error',
      'quote-props': 'off',
      'array-callback-return': 'error',
      'consistent-return': 'off',
      'default-case': 'off',
      'default-case-last': 'error',
      'dot-notation': ['error', { allowPattern: '^[_a-z][_a-zA-Z0-9]*$' }],
      'eqeqeq': 'off',
      'no-alert': 'error',
      'no-caller': 'error',
      'no-eval': 'error',
      'no-extra-bind': 'error',
      'no-extra-label': 'error',
      'no-global-assign': 'error',
      'no-labels': 'error',
      'no-lone-blocks': 'error',
      'no-loop-func': 'error',
      'no-multi-assign': 'error',
      'no-new': 'warn',
      'no-new-func': 'error',
      'no-new-wrappers': 'error',
      'no-octal-escape': 'error',
      'no-param-reassign': 'off',
      'no-proto': 'error',
      'no-redeclare': 'error',
      'no-return-assign': 'off',
      'no-script-url': 'error',
      'no-self-compare': 'error',
      'no-sequences': 'error',
      'no-shadow': 'off',
      'no-sync': 'off',
      'no-unused-expressions': ['error', { allowShortCircuit: true, allowTernary: true }],
      'no-useless-call': 'error',
      'no-useless-catch': 'error',
      'no-useless-concat': 'error',
      'no-useless-escape': 'off',
      'no-useless-return': 'off',
      'no-void': ['error', { allowAsStatement: true }],
      'no-with': 'error',
      'require-await': 'off',
      'yoda': 'error',
    },
  },

  // Test files
  {
    files: ['tests/**/*.ts', '**/*.test.ts', '**/*.spec.ts'],
    ...eslint.configs.recommended,
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
        project: './tsconfig.json',
      },
      globals: {
        ...globals.node,
        ...globals.jest,
      },
    },
    plugins: {
      '@typescript-eslint': tseslint,
      'jest': jestPlugin,
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/unbound-method': 'off',
      'import/no-extraneous-dependencies': 'off',
      'jest/no-disabled-tests': 'off',
    },
  },

  // Script files
  {
    files: ['scripts/**/*.ts'],
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
      },
      globals: {
        ...globals.node,
      },
    },
    plugins: {
      '@typescript-eslint': tseslint,
    },
    rules: {
      'no-console': 'off',
      'import/no-extraneous-dependencies': 'off',
    },
  },
];
