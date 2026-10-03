// https://docs.expo.dev/guides/using-eslint/
const { defineConfig } = require('eslint/config');
const expoConfig = require("eslint-config-expo/flat");

module.exports = defineConfig([
  expoConfig,
  {
    // The end-to-end tests' output: the app's web export, reports and traces.
    ignores: ["dist/*", "e2e/.output/*", "playwright-report/*", "test-results/*"],
  }
]);
