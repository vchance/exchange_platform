// Every test runs twice, once as an iOS build would load the app and once as
// an Android build would: each preset resolves that platform's own files
// (`.ios`, `.android`, `.native`) and never the `.web` ones, which exist only
// for the browser test harness.
const shared = {
  testMatch: ['**/__tests__/**/*.test.[jt]s?(x)'],
  transformIgnorePatterns: [
    // The Expo preset's list, plus `make-plural`, which ships as ES modules.
    '/node_modules/(?!(.pnpm|react-native|@react-native|@react-native-community|expo|@expo|@expo-google-fonts|react-navigation|@react-navigation|standard-navigation|make-plural))',
    '/node_modules/react-native-reanimated/plugin/',
    '/node_modules/@react-native/babel-preset/',
  ],
};

module.exports = {
  projects: [
    { preset: 'jest-expo/ios', ...shared },
    { preset: 'jest-expo/android', ...shared },
  ],
};
