import { createI18n, wordingFor, type BuildIdentity } from '@yuppers/shared';
import { render, screen } from '@testing-library/react-native';

import { I18nContext } from '../../lib/context';
import { appBuild } from '../../lib/client-identity';
import { BuildVersion } from '../BuildVersion';

const SHA = '0123456789abcdef0123456789abcdef01234567';

async function show(build: BuildIdentity | null, language: 'en' | 'es' = 'en') {
  const i18n = createI18n(language, wordingFor(language), () => {});
  await render(
    <I18nContext.Provider value={i18n}>
      <BuildVersion build={build} />
    </I18nContext.Provider>,
  );
}

test('names the version, the store build number and the commit', async () => {
  await show({ version: '0.1.0', build: '12', commit: SHA });
  expect(screen.getByTestId('build-version').props.children).toBe('Version 0.1.0 (build 12, 0123456)');
});

test('leaves out what the build does not say', async () => {
  await show({ version: '0.1.0', build: '12', commit: null });
  expect(screen.getByTestId('build-version').props.children).toBe('Version 0.1.0 (build 12)');
});

test('in the reader’s language', async () => {
  await show({ version: '0.1.0', build: '12', commit: null }, 'es');
  expect(screen.getByTestId('build-version').props.children).toBe('Versión 0.1.0 (compilación 12)');
});

test('shows nothing for a build that cannot read its version', async () => {
  await show(null);
  expect(screen.queryByTestId('build-version')).toBeNull();
});

test('the build is read from the installed app and the bundled config', () => {
  expect(appBuild({ native: '0.1.0', config: '0.0.9', nativeBuild: ' 12 ', commit: SHA })).toEqual({
    version: '0.1.0',
    build: '12',
    commit: SHA,
  });
  expect(appBuild({ native: null, config: '0.1.0', nativeBuild: null, commit: undefined })).toEqual({
    version: '0.1.0',
    build: null,
    commit: null,
  });
  expect(appBuild({ native: null, config: null, nativeBuild: '12', commit: SHA })).toBeNull();
});
